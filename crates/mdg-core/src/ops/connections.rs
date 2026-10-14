use super::update_module;
use crate::error::{MdgError, Result};
use crate::ids::{assert_valid_id, with_generator, IdGenerator, IdKind};
use crate::model::{Connection, Document, Endpoint};
use crate::queries::{find_connection, get_module, get_port, same_endpoint};
use crate::validate::check_connection;

#[derive(Default)]
pub struct AddConnectionOptions<'a> {
    pub from: Endpoint,
    pub to: Endpoint,
    /// Module whose internal graph stores the connection. Inferred from the
    /// endpoints when omitted.
    pub container_id: Option<String>,
    /// Explicit ID, unique in the document. Generated when omitted.
    pub id: Option<String>,
    pub id_generator: Option<&'a mut dyn IdGenerator>,
}

#[derive(Debug, Clone)]
pub struct AddConnectionResult {
    pub document: Document,
    pub id: String,
    pub container_id: String,
}

fn join_messages(issues: &[crate::validate::ValidationIssue]) -> String {
    issues.iter().map(|i| i.message.as_str()).collect::<Vec<_>>().join(" ")
}

/// Infers which module's internal graph can hold a connection between two
/// endpoints. Returns the container ID, or an error with an explanation.
pub fn resolve_connection_container(doc: &Document, from: &Endpoint, to: &Endpoint) -> Result<String> {
    let from_module = get_module(doc, &from.module_id)?;
    let to_module = get_module(doc, &to.module_id)?;
    get_port(doc, &from.module_id, &from.port_id)?;
    get_port(doc, &to.module_id, &to.port_id)?;
    let to_candidates = [Some(to_module.id.as_str()), to_module.parent_id.as_deref()];
    let mut candidates: Vec<&str> = Vec::new();
    for candidate in [Some(from_module.id.as_str()), from_module.parent_id.as_deref()].into_iter().flatten() {
        if to_candidates.contains(&Some(candidate)) && !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }
    let valid: Vec<&str> =
        candidates.iter().copied().filter(|c| check_connection(doc, c, from, to, None).is_empty()).collect();
    if valid.len() == 1 {
        return Ok(valid[0].to_string());
    }
    if valid.len() > 1 {
        return Err(MdgError::argument(format!(
            "Connection container is ambiguous ({}); pass containerId explicitly.",
            valid.join(", ")
        )));
    }
    let Some(first) = candidates.first() else {
        return Err(MdgError::operation(format!(
            "\"{from}\" and \"{to}\" are not on the same hierarchy level. Use cross-level routing (routeConnection / `mdg connect --route`) to create boundary ports."
        )));
    };
    Err(MdgError::operation(join_messages(&check_connection(doc, first, from, to, None))))
}

/// Adds a directed connection between two ports at the same hierarchy level.
pub fn add_connection(doc: &Document, options: AddConnectionOptions<'_>) -> Result<AddConnectionResult> {
    let AddConnectionOptions { from, to, container_id, id, id_generator } = options;
    let container_id = match container_id {
        Some(c) => c,
        None => resolve_connection_container(doc, &from, &to)?,
    };
    let issues = check_connection(doc, &container_id, &from, &to, None);
    if !issues.is_empty() {
        return Err(MdgError::operation(join_messages(&issues)));
    }
    let container = get_module(doc, &container_id)?;
    if let Some(duplicate) =
        container.connections.iter().find(|c| same_endpoint(&c.from, &from) && same_endpoint(&c.to, &to))
    {
        return Err(MdgError::operation(format!(
            "A connection from \"{from}\" to \"{to}\" already exists (\"{}\").",
            duplicate.id
        )));
    }
    let id = match id {
        Some(id) => id,
        None => with_generator(id_generator, |g| g.generate(IdKind::Connection)),
    };
    assert_valid_id(&id, "connection")?;
    if find_connection(doc, &id).is_some() {
        return Err(MdgError::argument(format!("A connection with ID \"{id}\" already exists.")));
    }
    let connection = Connection { id: id.clone(), from, to };
    let document = update_module(doc, &container_id, |m| m.connections.push(connection))?;
    Ok(AddConnectionResult { document, id, container_id })
}

/// Removes a connection by ID.
pub fn remove_connection(doc: &Document, connection_id: &str) -> Result<Document> {
    let container_id = find_connection(doc, connection_id)
        .ok_or_else(|| MdgError::not_found(format!("Connection \"{connection_id}\" does not exist.")))?
        .container
        .id
        .clone();
    update_module(doc, &container_id, |m| m.connections.retain(|c| c.id != connection_id))
}
