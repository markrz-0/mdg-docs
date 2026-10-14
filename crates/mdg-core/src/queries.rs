//! Read-only queries over a document. All traversals are iterative so that
//! very deep hierarchies cannot overflow the call stack.

use std::collections::HashSet;

use crate::error::{MdgError, Result};
use crate::model::{Connection, Document, Endpoint, Module, Port, PortDirection};

/// Returns the module, or `None`.
pub fn find_module<'a>(doc: &'a Document, module_id: &str) -> Option<&'a Module> {
    doc.modules.get(module_id).map(|m| &**m)
}

/// Returns the module or a `NOT_FOUND` error.
pub fn get_module<'a>(doc: &'a Document, module_id: &str) -> Result<&'a Module> {
    find_module(doc, module_id).ok_or_else(|| MdgError::not_found(format!("Module \"{module_id}\" does not exist.")))
}

pub fn get_root(doc: &Document) -> Result<&Module> {
    get_module(doc, &doc.root_id)
}

/// Returns the parent module, or `None` for the root.
pub fn get_parent<'a>(doc: &'a Document, module_id: &str) -> Result<Option<&'a Module>> {
    match &get_module(doc, module_id)?.parent_id {
        None => Ok(None),
        Some(parent) => get_module(doc, parent).map(Some),
    }
}

/// Returns the direct children in their stored order.
pub fn get_children<'a>(doc: &'a Document, module_id: &str) -> Result<Vec<&'a Module>> {
    get_module(doc, module_id)?.child_ids.iter().map(|id| get_module(doc, id)).collect()
}

/// Returns ancestors from the direct parent up to the root.
pub fn get_ancestors<'a>(doc: &'a Document, module_id: &str) -> Result<Vec<&'a Module>> {
    let mut result = Vec::new();
    let mut seen: HashSet<&str> = HashSet::from([module_id]);
    let mut current = get_module(doc, module_id)?.parent_id.as_deref();
    while let Some(id) = current {
        if !seen.insert(id) {
            return Err(MdgError::operation(format!("Parent cycle detected at module \"{id}\".")));
        }
        let module = get_module(doc, id)?;
        result.push(module);
        current = module.parent_id.as_deref();
    }
    Ok(result)
}

/// Returns the path from the root down to (and including) the module.
pub fn get_module_path<'a>(doc: &'a Document, module_id: &str) -> Result<Vec<&'a Module>> {
    let mut path = get_ancestors(doc, module_id)?;
    path.reverse();
    path.push(get_module(doc, module_id)?);
    Ok(path)
}

#[derive(Debug, Clone, Copy)]
pub struct DescendantVisit<'a> {
    pub module: &'a Module,
    /// Depth relative to the starting module (direct children are depth 1).
    pub depth: usize,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct WalkOptions {
    /// Maximum relative depth to include. Unlimited when `None`.
    pub max_depth: Option<usize>,
    /// Include the starting module at depth 0.
    pub include_self: bool,
}

/// Pre-order, iterative walk of a module's subtree.
pub fn walk_descendants<'a>(
    doc: &'a Document,
    module_id: &str,
    options: WalkOptions,
) -> Result<Vec<DescendantVisit<'a>>> {
    let max_depth = options.max_depth.unwrap_or(usize::MAX);
    let mut result = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut stack: Vec<(&str, usize)> = vec![(module_id, 0)];
    while let Some((id, depth)) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let module = get_module(doc, id)?;
        if depth > 0 || options.include_self {
            result.push(DescendantVisit { module, depth });
        }
        if depth >= max_depth {
            continue;
        }
        for child in module.child_ids.iter().rev() {
            stack.push((child, depth + 1));
        }
    }
    Ok(result)
}

/// Returns all descendants (pre-order), optionally limited in depth.
pub fn get_descendants<'a>(doc: &'a Document, module_id: &str, max_depth: Option<usize>) -> Result<Vec<&'a Module>> {
    Ok(walk_descendants(doc, module_id, WalkOptions { max_depth, include_self: false })?
        .into_iter()
        .map(|v| v.module)
        .collect())
}

/// True when `ancestor_id` equals `module_id` or is one of its ancestors.
pub fn is_ancestor_or_self(doc: &Document, ancestor_id: &str, module_id: &str) -> Result<bool> {
    if ancestor_id == module_id {
        return Ok(true);
    }
    Ok(get_ancestors(doc, module_id)?.iter().any(|m| m.id == ancestor_id))
}

/// Lowest common ancestor (ancestor-or-self) of two modules.
pub fn lowest_common_ancestor(doc: &Document, a: &str, b: &str) -> Result<String> {
    let mut a_chain: HashSet<&str> = HashSet::from([a]);
    let ancestors = get_ancestors(doc, a)?;
    a_chain.extend(ancestors.iter().map(|m| m.id.as_str()));
    if a_chain.contains(b) {
        return Ok(b.to_string());
    }
    for module in get_ancestors(doc, b)? {
        if a_chain.contains(module.id.as_str()) {
            return Ok(module.id.clone());
        }
    }
    Err(MdgError::operation(format!("Modules \"{a}\" and \"{b}\" share no common ancestor.")))
}

#[derive(Debug, Clone, Copy)]
pub struct PortLookup<'a> {
    pub port: &'a Port,
    pub direction: PortDirection,
}

pub fn find_port<'a>(module: &'a Module, port_id: &str) -> Option<PortLookup<'a>> {
    if let Some(port) = module.inputs.iter().find(|p| p.id == port_id) {
        return Some(PortLookup { port, direction: PortDirection::Input });
    }
    module.outputs.iter().find(|p| p.id == port_id).map(|port| PortLookup { port, direction: PortDirection::Output })
}

/// Returns the port and its direction or a `NOT_FOUND` error.
pub fn get_port<'a>(doc: &'a Document, module_id: &str, port_id: &str) -> Result<PortLookup<'a>> {
    find_port(get_module(doc, module_id)?, port_id)
        .ok_or_else(|| MdgError::not_found(format!("Port \"{port_id}\" does not exist on module \"{module_id}\".")))
}

#[derive(Debug, Clone, Copy)]
pub struct ConnectionLookup<'a> {
    /// The module whose internal graph stores the connection.
    pub container: &'a Module,
    pub connection: &'a Connection,
}

pub fn find_connection<'a>(doc: &'a Document, connection_id: &str) -> Option<ConnectionLookup<'a>> {
    doc.modules.values().find_map(|container| {
        container
            .connections
            .iter()
            .find(|c| c.id == connection_id)
            .map(|connection| ConnectionLookup { container, connection })
    })
}

pub fn get_connection<'a>(doc: &'a Document, connection_id: &str) -> Result<ConnectionLookup<'a>> {
    find_connection(doc, connection_id)
        .ok_or_else(|| MdgError::not_found(format!("Connection \"{connection_id}\" does not exist.")))
}

pub fn same_endpoint(a: &Endpoint, b: &Endpoint) -> bool {
    a.module_id == b.module_id && a.port_id == b.port_id
}

fn references(connection: &Connection, module_id: &str, port_id: Option<&str>) -> bool {
    let hit = |ep: &Endpoint| ep.module_id == module_id && port_id.is_none_or(|p| ep.port_id == p);
    hit(&connection.from) || hit(&connection.to)
}

/// Connections that reference a module (optionally a single port of it).
/// Only the module's own graph and its parent's graph can hold such references.
pub fn connections_referencing<'a>(
    doc: &'a Document,
    module_id: &str,
    port_id: Option<&str>,
) -> Result<Vec<ConnectionLookup<'a>>> {
    let module = get_module(doc, module_id)?;
    let mut containers = vec![module];
    if let Some(parent) = &module.parent_id {
        containers.push(get_module(doc, parent)?);
    }
    let mut result = Vec::new();
    for container in containers {
        for connection in &container.connections {
            if references(connection, module_id, port_id) {
                result.push(ConnectionLookup { container, connection });
            }
        }
    }
    Ok(result)
}

pub fn count_connections(doc: &Document) -> usize {
    doc.modules.values().map(|m| m.connections.len()).sum()
}

pub fn count_ports(doc: &Document) -> usize {
    doc.modules.values().map(|m| m.inputs.len() + m.outputs.len()).sum()
}
