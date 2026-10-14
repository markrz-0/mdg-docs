use std::collections::HashSet;
use std::sync::Arc;

use indexmap::IndexSet;

use super::{insert_at, require_name, update_module, CreateResult};
use crate::error::{MdgError, Result};
use crate::ids::{assert_valid_id, with_generator, IdGenerator, IdKind};
use crate::model::{Document, Port, PortDirection};
use crate::queries::{connections_referencing, find_port, get_module, get_port};

#[derive(Default)]
pub struct AddPortOptions<'a> {
    pub name: String,
    pub description: Option<String>,
    /// Explicit ID, unique within the module. Generated when omitted.
    pub id: Option<String>,
    /// Position in the inputs/outputs list. Appended when omitted.
    pub index: Option<usize>,
    pub id_generator: Option<&'a mut dyn IdGenerator>,
}

/// Adds an input or output port to a non-root module.
pub fn add_port(
    doc: &Document,
    module_id: &str,
    direction: PortDirection,
    options: AddPortOptions<'_>,
) -> Result<CreateResult> {
    let module = get_module(doc, module_id)?;
    if module.parent_id.is_none() {
        return Err(MdgError::operation("The root module cannot have input or output ports."));
    }
    let id = match options.id {
        Some(id) => id,
        None => with_generator(options.id_generator, |g| g.generate(IdKind::Port)),
    };
    assert_valid_id(&id, "port")?;
    if find_port(module, &id).is_some() {
        return Err(MdgError::argument(format!("Module \"{module_id}\" already has a port with ID \"{id}\".")));
    }
    let port = Port {
        id: id.clone(),
        name: require_name(&options.name, "Port")?,
        description: options.description.unwrap_or_default(),
    };
    let document = update_module(doc, module_id, |m| insert_at(m.ports_mut(direction), port, options.index))?;
    Ok(CreateResult { document, id })
}

pub fn add_input(doc: &Document, module_id: &str, options: AddPortOptions<'_>) -> Result<CreateResult> {
    add_port(doc, module_id, PortDirection::Input, options)
}

pub fn add_output(doc: &Document, module_id: &str, options: AddPortOptions<'_>) -> Result<CreateResult> {
    add_port(doc, module_id, PortDirection::Output, options)
}

/// Removes a port. Fails if it is connected, unless `cascade` is requested,
/// which also removes the connections referencing it.
pub fn remove_port(doc: &Document, module_id: &str, port_id: &str, cascade: bool) -> Result<Document> {
    get_port(doc, module_id, port_id)?;
    let referencing = connections_referencing(doc, module_id, Some(port_id))?;
    if !referencing.is_empty() && !cascade {
        let ids: Vec<&str> = referencing.iter().map(|r| r.connection.id.as_str()).collect();
        return Err(MdgError::operation(format!(
            "Port \"{module_id}:{port_id}\" is used by {} connection(s): {}. Disconnect them first or use cascade.",
            ids.len(),
            ids.join(", ")
        )));
    }
    let removed_ids: HashSet<String> = referencing.iter().map(|r| r.connection.id.clone()).collect();
    let containers: IndexSet<String> = referencing.iter().map(|r| r.container.id.clone()).collect();
    let mut document = doc.clone();
    for container in containers {
        let mut m = (**document.modules.get(&container).expect("container exists")).clone();
        m.connections.retain(|c| !removed_ids.contains(&c.id));
        document.modules.insert(container, Arc::new(m));
    }
    let mut module = (**document.modules.get(module_id).expect("module exists")).clone();
    module.inputs.retain(|p| p.id != port_id);
    module.outputs.retain(|p| p.id != port_id);
    document.modules.insert(module_id, Arc::new(module));
    Ok(document)
}

fn update_port(doc: &Document, module_id: &str, port_id: &str, update: impl FnOnce(&mut Port)) -> Result<Document> {
    let direction = get_port(doc, module_id, port_id)?.direction;
    update_module(doc, module_id, |m| {
        if let Some(port) = m.ports_mut(direction).iter_mut().find(|p| p.id == port_id) {
            update(port);
        }
    })
}

pub fn rename_port(doc: &Document, module_id: &str, port_id: &str, name: &str) -> Result<Document> {
    let trimmed = require_name(name, "Port")?;
    update_port(doc, module_id, port_id, |p| p.name = trimmed)
}

pub fn update_port_description(doc: &Document, module_id: &str, port_id: &str, description: &str) -> Result<Document> {
    update_port(doc, module_id, port_id, |p| p.description = description.to_string())
}
