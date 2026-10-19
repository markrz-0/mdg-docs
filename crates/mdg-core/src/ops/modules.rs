use std::collections::HashSet;
use std::sync::Arc;

use super::{insert_at, require_name, update_module, CreateResult};
use crate::error::{MdgError, Result};
use crate::ids::{assert_valid_id, with_generator, IdGenerator, IdKind};
use crate::model::{Document, Layout, Module, Position};
use crate::queries::{
    connections_referencing, find_module, get_module, is_ancestor_or_self, walk_descendants, WalkOptions,
};

#[derive(Default)]
pub struct AddModuleOptions<'a> {
    pub parent_id: String,
    pub name: String,
    pub description: Option<String>,
    /// Explicit ID. Generated when omitted.
    pub id: Option<String>,
    /// Position in the parent's child list. Appended when omitted.
    pub index: Option<usize>,
    /// Optional canvas position (presentation only).
    pub position: Option<Position>,
    pub id_generator: Option<&'a mut dyn IdGenerator>,
}

/// Adds a new leaf module under `parent_id`.
pub fn add_module(doc: &Document, options: AddModuleOptions<'_>) -> Result<CreateResult> {
    let parent = get_module(doc, &options.parent_id)?;
    let id = match options.id {
        Some(id) => id,
        None => with_generator(options.id_generator, |g| g.generate(IdKind::Module)),
    };
    assert_valid_id(&id, "module")?;
    if find_module(doc, &id).is_some() {
        return Err(MdgError::argument(format!("A module with ID \"{id}\" already exists.")));
    }
    let module = Module {
        id: id.clone(),
        parent_id: Some(parent.id.clone()),
        name: require_name(&options.name, "Module")?,
        description: options.description.unwrap_or_default(),
        inputs: vec![],
        outputs: vec![],
        child_ids: vec![],
        connections: vec![],
        tags: vec![],
    };
    let mut parent = parent.clone();
    insert_at(&mut parent.child_ids, id.clone(), options.index);
    let mut document = doc.clone();
    document.modules.insert(id.clone(), Arc::new(module));
    document.modules.insert(parent.id.clone(), Arc::new(parent));
    if let Some(position) = options.position {
        document = set_module_position(&document, &id, position)?;
    }
    Ok(CreateResult { document, id })
}

/// Removes a module. Fails safely unless `cascade` is requested: with
/// `cascade`, also removes all descendants and every connection in the
/// parent graph that references the module.
pub fn remove_module(doc: &Document, module_id: &str, cascade: bool) -> Result<Document> {
    let module = get_module(doc, module_id)?;
    let Some(parent_id) = &module.parent_id else {
        return Err(MdgError::operation("The root module cannot be removed."));
    };
    let parent = get_module(doc, parent_id)?;
    let referencing: Vec<&str> = parent
        .connections
        .iter()
        .filter(|c| c.from.module_id == module_id || c.to.module_id == module_id)
        .map(|c| c.id.as_str())
        .collect();
    if !cascade {
        if !module.child_ids.is_empty() {
            return Err(MdgError::operation(format!(
                "Module \"{module_id}\" has {} child module(s). Remove them first or use cascade.",
                module.child_ids.len()
            )));
        }
        if !referencing.is_empty() {
            return Err(MdgError::operation(format!(
                "Module \"{module_id}\" is connected in \"{}\" by {} connection(s): {}. Disconnect them first or use cascade.",
                parent.id,
                referencing.len(),
                referencing.join(", ")
            )));
        }
    }
    let removed: Vec<String> =
        walk_descendants(doc, module_id, WalkOptions { include_self: true, ..Default::default() })?
            .into_iter()
            .map(|v| v.module.id.clone())
            .collect();
    let referencing: HashSet<&str> = referencing.into_iter().collect();
    let mut parent = parent.clone();
    parent.child_ids.retain(|id| id != module_id);
    parent.connections.retain(|c| !referencing.contains(c.id.as_str()));
    let mut document = doc.clone();
    for id in &removed {
        document.modules.remove(id);
    }
    document.modules.insert(parent.id.clone(), Arc::new(parent));
    Ok(remove_positions(document, &removed, true))
}

pub fn rename_module(doc: &Document, module_id: &str, name: &str) -> Result<Document> {
    let trimmed = require_name(name, "Module")?;
    update_module(doc, module_id, |m| m.name = trimmed)
}

pub fn update_module_description(doc: &Document, module_id: &str, description: &str) -> Result<Document> {
    update_module(doc, module_id, |m| m.description = description.to_string())
}

/// Moves a module (with its subtree) under a new parent, preserving all IDs.
///
/// Rejects moving the root, moves that create a parent cycle, and moves that
/// would invalidate connections in the old parent. Connections are never
/// silently rewritten or deleted. `index` is the position in the new
/// parent's child list (appended when omitted).
pub fn move_module(doc: &Document, module_id: &str, new_parent_id: &str, index: Option<usize>) -> Result<Document> {
    let module = get_module(doc, module_id)?;
    let new_parent = get_module(doc, new_parent_id)?;
    let Some(old_parent_id) = &module.parent_id else {
        return Err(MdgError::operation("The root module cannot be moved."));
    };
    if is_ancestor_or_self(doc, module_id, new_parent_id)? {
        return Err(MdgError::operation(format!(
            "Cannot move \"{module_id}\" into \"{new_parent_id}\": that would create a parent cycle."
        )));
    }
    let old_parent = get_module(doc, old_parent_id)?;
    let mut document = doc.clone();
    if old_parent.id == new_parent.id {
        let mut parent = old_parent.clone();
        parent.child_ids.retain(|id| id != module_id);
        insert_at(&mut parent.child_ids, module_id.to_string(), index);
        document.modules.insert(parent.id.clone(), Arc::new(parent));
        return Ok(document);
    }
    let blocking: Vec<&str> = connections_referencing(doc, module_id, None)?
        .into_iter()
        .filter(|lookup| lookup.container.id == old_parent.id)
        .map(|lookup| lookup.connection.id.as_str())
        .collect();
    if !blocking.is_empty() {
        return Err(MdgError::operation(format!(
            "Cannot move \"{module_id}\": {} connection(s) in \"{}\" reference it ({}) and would become invalid. Disconnect them first.",
            blocking.len(),
            old_parent.id,
            blocking.join(", ")
        )));
    }
    let mut old = old_parent.clone();
    old.child_ids.retain(|id| id != module_id);
    let mut new = new_parent.clone();
    insert_at(&mut new.child_ids, module_id.to_string(), index);
    let mut moved = module.clone();
    moved.parent_id = Some(new.id.clone());
    document.modules.insert(old.id.clone(), Arc::new(old));
    document.modules.insert(new.id.clone(), Arc::new(new));
    document.modules.insert(module_id, Arc::new(moved));
    // The old canvas position is meaningless inside a different parent.
    Ok(remove_positions(document, &[module_id.to_string()], false))
}

/// Sets a module's canvas position (presentation metadata only).
pub fn set_module_position(doc: &Document, module_id: &str, position: Position) -> Result<Document> {
    get_module(doc, module_id)?;
    if !position.x.is_finite() || !position.y.is_finite() {
        return Err(MdgError::argument("Positions must be finite numbers."));
    }
    let mut document = doc.clone();
    let layout = document.layout.get_or_insert_with(Layout::default);
    layout.positions.insert(module_id, position);
    Ok(document)
}

/// Drops canvas positions of the given modules. With `boundaries`, also
/// drops the positions of their own boundary panels (used when the modules
/// are removed; a moved module keeps its internal layout).
fn remove_positions(mut doc: Document, module_ids: &[String], boundaries: bool) -> Document {
    let Some(layout) = &doc.layout else { return doc };
    let mut positions = layout.positions.clone();
    let mut boundary_layout = layout.boundaries.clone().unwrap_or_default();
    let mut changed = false;
    for id in module_ids {
        if positions.remove(id).is_some() {
            changed = true;
        }
        if boundaries && boundary_layout.remove(id).is_some() {
            changed = true;
        }
    }
    if changed {
        let boundaries = if boundary_layout.is_empty() { None } else { Some(boundary_layout) };
        doc.layout = Some(Layout { positions, boundaries });
    }
    doc
}
