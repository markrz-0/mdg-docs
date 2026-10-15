//! Canonical MDG data model.
//!
//! The document is stored as a flat module map keyed by module ID. The
//! hierarchy is expressed through `parent_id` / `child_ids`; connections are
//! stored on the module whose internal graph contains them.
//!
//! Modules are reference-counted so that an edit copies only the module map
//! and the touched modules. Snapshots of unchanged modules are shared, which
//! keeps undo history cheap.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::jsmap::JsMap;

/// Schema (data model) version of new documents. Documents that use no 1.1
/// features keep this version, so they stay readable by 1.0 readers.
pub const SCHEMA_VERSION: &str = "1.0.0";
/// Schema version that introduced request/response ports. Written whenever a
/// document contains one.
pub const REQUEST_PORTS_SCHEMA_VERSION: &str = "1.1.0";
/// Schema major version this implementation understands.
pub const SCHEMA_MAJOR: f64 = 1.0;

/// Direction of a port, derived from which list (`inputs`/`outputs`) holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PortDirection {
    Input,
    Output,
}

impl PortDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            PortDirection::Input => "input",
            PortDirection::Output => "output",
        }
    }
}

impl std::fmt::Display for PortDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What travels through a port.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PortKind {
    /// A one-way message: outputs emit it, inputs listen for it.
    #[default]
    Event,
    /// A request answered by a response over the same connection. An output
    /// sends the request (the caller); an input handles it (the handler). The
    /// connection runs caller → handler; the response flows back implicitly.
    Request,
}

impl PortKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PortKind::Event => "event",
            PortKind::Request => "request",
        }
    }

    pub fn is_event(&self) -> bool {
        *self == PortKind::Event
    }
}

impl std::fmt::Display for PortKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A named input or output on a module boundary.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Port {
    /// Stable ID, unique within its module (inputs and outputs share one namespace).
    pub id: String,
    /// Human-readable name. Not required to be unique.
    pub name: String,
    /// Omitted from JSON for events, so event-only output is unchanged.
    #[serde(default, skip_serializing_if = "PortKind::is_event")]
    pub kind: PortKind,
    /// Natural-language Markdown contract. For a request port: the request.
    pub description: String,
    /// Request ports only: Markdown contract of the response. Always empty
    /// on event ports.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub response: String,
}

/// A reference to a port on a module.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub module_id: String,
    pub port_id: String,
}

impl Endpoint {
    pub fn new(module_id: impl Into<String>, port_id: impl Into<String>) -> Self {
        Endpoint { module_id: module_id.into(), port_id: port_id.into() }
    }
}

impl std::fmt::Display for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.module_id, self.port_id)
    }
}

/// A directed, possible message flow between two ports.
///
/// Stored inside module M. Each endpoint references either M itself (a
/// boundary port) or a direct child of M. Direction is never stored: it is
/// derived from the referenced port.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Connection {
    pub id: String,
    pub from: Endpoint,
    pub to: Endpoint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Module {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    /// Markdown.
    pub description: String,
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
    /// Ordered list of direct children.
    pub child_ids: Vec<String>,
    /// Connections in this module's internal graph.
    pub connections: Vec<Connection>,
}

impl Module {
    pub fn ports(&self, direction: PortDirection) -> &[Port] {
        match direction {
            PortDirection::Input => &self.inputs,
            PortDirection::Output => &self.outputs,
        }
    }

    pub fn ports_mut(&mut self, direction: PortDirection) -> &mut Vec<Port> {
        match direction {
            PortDirection::Input => &mut self.inputs,
            PortDirection::Output => &mut self.outputs,
        }
    }

    /// Inputs then outputs.
    pub fn all_ports(&self) -> impl Iterator<Item = &Port> {
        self.inputs.iter().chain(self.outputs.iter())
    }
}

impl Document {
    /// True when any module has a request port (a schema 1.1 feature).
    pub fn has_request_ports(&self) -> bool {
        self.modules.values().any(|m| m.all_ports().any(|p| p.kind == PortKind::Request))
    }
}

/// The schema version to write for `doc`: its own version, raised to
/// [`REQUEST_PORTS_SCHEMA_VERSION`] when it uses request ports but is older.
pub fn schema_version_for(doc: &Document) -> String {
    let older_than_1_1 = {
        let mut parts = doc.format_version.split('.').map(|p| p.parse::<u64>().ok());
        matches!((parts.next(), parts.next()), (Some(Some(1)), Some(Some(0))))
    };
    if older_than_1_1 && doc.has_request_ports() {
        REQUEST_PORTS_SCHEMA_VERSION.to_string()
    } else {
        doc.format_version.clone()
    }
}

/// Canvas position (presentation only).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

impl Position {
    pub fn new(x: f64, y: f64) -> Self {
        Position { x, y }
    }
}

/// Positions of a module's own boundary panels inside its internal graph.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct BoundaryLayout {
    /// Panel listing the module's inputs (sources inside its graph).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub input: Option<Position>,
    /// Panel listing the module's outputs (destinations inside its graph).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub output: Option<Position>,
}

/// Presentation metadata. Never affects message-flow semantics and is
/// ignored by structural validation.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Layout {
    /// Module ID -> position within its parent's canvas.
    pub positions: JsMap<Position>,
    /// Module ID -> positions of its boundary panels within its own canvas.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boundaries: Option<JsMap<BoundaryLayout>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Metadata {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Unknown optional metadata, preserved across read/write.
    #[serde(skip)]
    pub extra: JsMap<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    /// Schema version of the data model (semver, e.g. "1.0.0").
    pub format_version: String,
    pub root_id: String,
    pub modules: JsMap<Arc<Module>>,
    pub metadata: Metadata,
    /// Optional presentation metadata (node positions).
    pub layout: Option<Layout>,
    /// Unknown optional top-level index fields, preserved verbatim.
    pub extensions: Option<JsMap<Value>>,
}
