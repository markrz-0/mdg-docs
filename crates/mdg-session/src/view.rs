//! What the editor UI renders: a serializable snapshot of the session plus
//! values derived by core (validation summary, breadcrumbs, details for the
//! selection, valid connection endpoints). The UI never re-implements rules.

use std::sync::Arc;

use mdg_core::{
    check_connection, connections_referencing, describe_connection, find_connection, format_endpoint, get_module,
    get_module_path, same_endpoint, validate_document, Endpoint, JsMap, Module, ModuleRef, Result, ValidationOptions,
};
use serde::{Deserialize, Serialize};

use crate::layout::Positions;
use crate::session::{Selection, Session};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentView {
    pub root_id: String,
    pub title: String,
    pub modules: JsMap<Arc<Module>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ValidationSummary {
    pub valid: bool,
    pub errors: usize,
    pub warnings: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConnectionInfo {
    pub id: String,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Details {
    /// For a selected port: how many connections use it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port_connections: Option<usize>,
    /// For a selected connection: its endpoints in words.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection: Option<ConnectionInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub document: DocumentView,
    pub positions: Arc<Positions>,
    pub file_name: String,
    pub file_path: Option<String>,
    pub dirty: bool,
    pub current_module_id: String,
    pub selection: Option<Selection>,
    pub error: Option<String>,
    pub can_undo: bool,
    pub can_redo: bool,
    pub layout_version: u64,
    pub validation: ValidationSummary,
    /// Root-to-current module path (breadcrumbs).
    pub path: Vec<ModuleRef>,
    pub details: Details,
}

/// Which end of a connection is being dragged from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectRole {
    Source,
    Target,
}

impl Session {
    pub fn view(&self) -> View {
        let doc = &self.document;
        let result = validate_document(doc, ValidationOptions::default());
        let path = get_module_path(doc, &self.current_module_id)
            .map(|p| p.into_iter().map(|m| ModuleRef { id: m.id.clone(), name: m.name.clone() }).collect())
            .unwrap_or_default();
        let mut details = Details::default();
        match &self.selection {
            Some(Selection::Port { module_id, port_id }) => {
                details.port_connections = connections_referencing(doc, module_id, Some(port_id)).ok().map(|c| c.len());
            }
            Some(Selection::Connection { connection_id }) => {
                details.connection = find_connection(doc, connection_id).map(|lookup| {
                    let c = describe_connection(doc, &lookup.container.id, lookup.connection);
                    ConnectionInfo {
                        id: c.id.clone(),
                        from: format_endpoint(&c.from).replace('`', ""),
                        to: format_endpoint(&c.to).replace('`', ""),
                    }
                });
            }
            _ => {}
        }
        View {
            document: DocumentView {
                root_id: doc.root_id.clone(),
                title: doc.metadata.title.clone(),
                modules: doc.modules.clone(),
            },
            positions: self.positions.clone(),
            file_name: self.file_name.clone(),
            file_path: self.file_path.as_ref().map(|p| p.display().to_string()),
            dirty: self.dirty,
            current_module_id: self.current_module_id.clone(),
            selection: self.selection.clone(),
            error: self.error.clone(),
            can_undo: !self.past.is_empty(),
            can_redo: !self.future.is_empty(),
            layout_version: self.layout_version,
            validation: ValidationSummary {
                valid: result.valid,
                errors: result.errors.len(),
                warnings: result.warnings.len(),
            },
            path,
            details,
        }
    }

    /// Endpoints in the open module's graph that a connection dragged from
    /// `endpoint` may end at: exactly what `connect` would accept (valid
    /// direction and scope, not a duplicate).
    pub fn connection_partners(&self, endpoint: &Endpoint, role: ConnectRole) -> Result<Vec<Endpoint>> {
        let doc = &self.document;
        let container = get_module(doc, &self.current_module_id)?;
        let mut candidates: Vec<Endpoint> = Vec::new();
        let mut push = |module: &Module, ports: &[mdg_core::Port]| {
            candidates.extend(ports.iter().map(|p| Endpoint::new(module.id.clone(), p.id.clone())));
        };
        // Sources: the container's inputs and the children's outputs.
        // Destinations: the container's outputs and the children's inputs.
        let want_destinations = role == ConnectRole::Source;
        push(container, if want_destinations { &container.outputs } else { &container.inputs });
        for child_id in &container.child_ids {
            let child = get_module(doc, child_id)?;
            push(child, if want_destinations { &child.inputs } else { &child.outputs });
        }
        Ok(candidates
            .into_iter()
            .filter(|other| {
                let (from, to) = if want_destinations { (endpoint, other) } else { (other, endpoint) };
                check_connection(doc, &container.id, from, to, None).is_empty()
                    && !container.connections.iter().any(|c| same_endpoint(&c.from, from) && same_endpoint(&c.to, to))
            })
            .collect())
    }
}
