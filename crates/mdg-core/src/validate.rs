//! Deterministic structural validation.
//!
//! Errors make a document invalid. Warnings (e.g. unconnected ports) are
//! informational and never make a document invalid. No semantic
//! interpretation of Markdown contracts happens here.

use std::collections::{HashMap, HashSet};

use indexmap::IndexSet;
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};

use crate::ids::is_valid_id;
use crate::model::{is_valid_color, Document, Endpoint, Module, PortDirection, PortKind};
use crate::queries::{find_module, find_port};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IssueCode {
    // errors
    RootMissing,
    RootHasParent,
    RootHasPorts,
    MultipleRoots,
    ModuleIdMismatch,
    InvalidId,
    MissingParent,
    ParentCycle,
    UnreachableModule,
    MissingChild,
    DuplicateChild,
    ChildParentMismatch,
    ParentChildMismatch,
    DuplicatePortId,
    DuplicateConnectionId,
    ConnectionModuleMissing,
    ConnectionPortMissing,
    ConnectionOutOfScope,
    InvalidSourceDirection,
    InvalidDestinationDirection,
    PortKindMismatch,
    ResponseOnEventPort,
    DuplicateTagId,
    InvalidTagColor,
    UnknownTag,
    DuplicateModuleTag,
    // warnings
    UnconnectedPort,
    DuplicateConnection,
}

impl IssueCode {
    pub fn as_str(self) -> &'static str {
        match self {
            IssueCode::RootMissing => "ROOT_MISSING",
            IssueCode::RootHasParent => "ROOT_HAS_PARENT",
            IssueCode::RootHasPorts => "ROOT_HAS_PORTS",
            IssueCode::MultipleRoots => "MULTIPLE_ROOTS",
            IssueCode::ModuleIdMismatch => "MODULE_ID_MISMATCH",
            IssueCode::InvalidId => "INVALID_ID",
            IssueCode::MissingParent => "MISSING_PARENT",
            IssueCode::ParentCycle => "PARENT_CYCLE",
            IssueCode::UnreachableModule => "UNREACHABLE_MODULE",
            IssueCode::MissingChild => "MISSING_CHILD",
            IssueCode::DuplicateChild => "DUPLICATE_CHILD",
            IssueCode::ChildParentMismatch => "CHILD_PARENT_MISMATCH",
            IssueCode::ParentChildMismatch => "PARENT_CHILD_MISMATCH",
            IssueCode::DuplicatePortId => "DUPLICATE_PORT_ID",
            IssueCode::DuplicateConnectionId => "DUPLICATE_CONNECTION_ID",
            IssueCode::ConnectionModuleMissing => "CONNECTION_MODULE_MISSING",
            IssueCode::ConnectionPortMissing => "CONNECTION_PORT_MISSING",
            IssueCode::ConnectionOutOfScope => "CONNECTION_OUT_OF_SCOPE",
            IssueCode::InvalidSourceDirection => "INVALID_SOURCE_DIRECTION",
            IssueCode::InvalidDestinationDirection => "INVALID_DESTINATION_DIRECTION",
            IssueCode::PortKindMismatch => "PORT_KIND_MISMATCH",
            IssueCode::ResponseOnEventPort => "RESPONSE_ON_EVENT_PORT",
            IssueCode::DuplicateTagId => "DUPLICATE_TAG_ID",
            IssueCode::InvalidTagColor => "INVALID_TAG_COLOR",
            IssueCode::UnknownTag => "UNKNOWN_TAG",
            IssueCode::DuplicateModuleTag => "DUPLICATE_MODULE_TAG",
            IssueCode::UnconnectedPort => "UNCONNECTED_PORT",
            IssueCode::DuplicateConnection => "DUPLICATE_CONNECTION",
        }
    }
}

/// Order of the keys when an issue is written as JSON. The original
/// implementation built issues with differently ordered object literals;
/// keeping that order keeps `--json` output byte-identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueLayout {
    /// severity, code, moduleId, portId, connectionId, message
    Standard,
    /// severity, moduleId, connectionId, portId, code, message
    Endpoint,
    /// severity, code, message, moduleId
    Container,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub code: IssueCode,
    pub message: String,
    /// The module the issue belongs to (for connections: the containing module).
    pub module_id: Option<String>,
    pub port_id: Option<String>,
    pub connection_id: Option<String>,
    pub layout: IssueLayout,
}

impl Serialize for ValidationIssue {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        let opt = |map: &mut S::SerializeMap, key: &str, value: &Option<String>| match value {
            Some(v) => map.serialize_entry(key, v),
            None => Ok(()),
        };
        map.serialize_entry("severity", &self.severity)?;
        match self.layout {
            IssueLayout::Standard => {
                map.serialize_entry("code", &self.code)?;
                opt(&mut map, "moduleId", &self.module_id)?;
                opt(&mut map, "portId", &self.port_id)?;
                opt(&mut map, "connectionId", &self.connection_id)?;
                map.serialize_entry("message", &self.message)?;
            }
            IssueLayout::Endpoint => {
                opt(&mut map, "moduleId", &self.module_id)?;
                opt(&mut map, "connectionId", &self.connection_id)?;
                opt(&mut map, "portId", &self.port_id)?;
                map.serialize_entry("code", &self.code)?;
                map.serialize_entry("message", &self.message)?;
            }
            IssueLayout::Container => {
                map.serialize_entry("code", &self.code)?;
                map.serialize_entry("message", &self.message)?;
                opt(&mut map, "moduleId", &self.module_id)?;
            }
        }
        map.end()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ValidationResult {
    /// True when there are no errors. Warnings do not affect validity.
    pub valid: bool,
    pub errors: Vec<ValidationIssue>,
    pub warnings: Vec<ValidationIssue>,
}

#[derive(Debug, Clone, Copy)]
pub struct ValidationOptions {
    /// Compute warnings. Default true.
    pub warnings: bool,
}

impl Default for ValidationOptions {
    fn default() -> Self {
        ValidationOptions { warnings: true }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Source,
    Destination,
}

/// Collision-free key for an endpoint.
pub(crate) fn endpoint_key(module_id: &str, port_id: &str) -> String {
    format!("{module_id}\u{0}{port_id}")
}

fn issue(severity: Severity, code: IssueCode, module_id: &str, message: String) -> ValidationIssue {
    ValidationIssue {
        severity,
        code,
        message,
        module_id: Some(module_id.to_string()),
        port_id: None,
        connection_id: None,
        layout: IssueLayout::Standard,
    }
}

/// Checks one endpoint of a connection stored in `container`.
///
/// Valid sources: an input of the container, or an output of a direct child.
/// Valid destinations: an output of the container, or an input of a direct child.
fn check_endpoint(
    doc: &Document,
    container: &Module,
    ep: &Endpoint,
    role: Role,
    connection_id: Option<&str>,
) -> Option<ValidationIssue> {
    let label = match connection_id {
        None => "Connection".to_string(),
        Some(id) => format!("Connection \"{id}\""),
    };
    let make = |code: IssueCode, port_id: Option<&str>, message: String| ValidationIssue {
        severity: Severity::Error,
        code,
        message,
        module_id: Some(container.id.clone()),
        port_id: port_id.map(str::to_string),
        connection_id: connection_id.map(str::to_string),
        layout: IssueLayout::Endpoint,
    };
    let cid = &container.id;
    let Some(target) = find_module(doc, &ep.module_id) else {
        return Some(make(
            IssueCode::ConnectionModuleMissing,
            None,
            format!("{label} in \"{cid}\" references missing module \"{}\".", ep.module_id),
        ));
    };
    let is_boundary = ep.module_id == container.id;
    if !is_boundary && target.parent_id.as_deref() != Some(cid.as_str()) {
        return Some(make(
            IssueCode::ConnectionOutOfScope,
            None,
            format!(
                "{label} in \"{cid}\" references \"{ep}\", but module \"{}\" is neither \"{cid}\" nor one of its direct children. Cross-level connections must be routed through boundary ports.",
                ep.module_id
            ),
        ));
    }
    let Some(lookup) = find_port(target, &ep.port_id) else {
        return Some(make(
            IssueCode::ConnectionPortMissing,
            Some(&ep.port_id),
            format!("{label} in \"{cid}\" references missing port \"{ep}\"."),
        ));
    };
    let required = match (role, is_boundary) {
        (Role::Source, true) | (Role::Destination, false) => PortDirection::Input,
        (Role::Source, false) | (Role::Destination, true) => PortDirection::Output,
    };
    if lookup.direction != required {
        let place =
            if is_boundary { format!("the boundary of \"{cid}\"") } else { format!("child \"{}\"", ep.module_id) };
        let (code, role_name, rule) = match role {
            Role::Source => (
                IssueCode::InvalidSourceDirection,
                "source",
                format!("A source must be an input of \"{cid}\" or an output of one of its direct children."),
            ),
            Role::Destination => (
                IssueCode::InvalidDestinationDirection,
                "destination",
                format!("A destination must be an output of \"{cid}\" or an input of one of its direct children."),
            ),
        };
        return Some(make(
            code,
            Some(&ep.port_id),
            format!("{label} in \"{cid}\" uses {} \"{ep}\" on {place} as a {role_name}. {rule}", lookup.direction),
        ));
    }
    None
}

/// Checks whether a connection between `from` and `to` would be structurally
/// valid inside `container_id`. Returns an empty list when valid.
pub fn check_connection(
    doc: &Document,
    container_id: &str,
    from: &Endpoint,
    to: &Endpoint,
    connection_id: Option<&str>,
) -> Vec<ValidationIssue> {
    let Some(container) = find_module(doc, container_id) else {
        return vec![ValidationIssue {
            severity: Severity::Error,
            code: IssueCode::ConnectionModuleMissing,
            message: format!("Container module \"{container_id}\" does not exist."),
            module_id: Some(container_id.to_string()),
            port_id: None,
            connection_id: None,
            layout: IssueLayout::Container,
        }];
    };
    let mut issues = Vec::new();
    issues.extend(check_endpoint(doc, container, from, Role::Source, connection_id));
    issues.extend(check_endpoint(doc, container, to, Role::Destination, connection_id));
    if issues.is_empty() {
        issues.extend(check_kinds(doc, container_id, from, to, connection_id));
    }
    issues
}

/// Both ends of a connection must carry the same kind of message: events
/// connect to events, requests to requests. Only called once both endpoints
/// are known to exist.
fn check_kinds(
    doc: &Document,
    container_id: &str,
    from: &Endpoint,
    to: &Endpoint,
    connection_id: Option<&str>,
) -> Option<ValidationIssue> {
    let kind =
        |ep: &Endpoint| find_module(doc, &ep.module_id).and_then(|m| find_port(m, &ep.port_id)).map(|p| p.port.kind);
    let (from_kind, to_kind) = (kind(from)?, kind(to)?);
    if from_kind == to_kind {
        return None;
    }
    let label = match connection_id {
        None => "Connection".to_string(),
        Some(id) => format!("Connection \"{id}\""),
    };
    Some(ValidationIssue {
        severity: Severity::Error,
        code: IssueCode::PortKindMismatch,
        message: format!(
            "{label} in \"{container_id}\" joins {from_kind} port \"{from}\" to {to_kind} port \"{to}\". Both ends must be the same kind (event or request)."
        ),
        module_id: Some(container_id.to_string()),
        port_id: None,
        connection_id: connection_id.map(str::to_string),
        layout: IssueLayout::Endpoint,
    })
}

#[derive(Default)]
struct Context {
    errors: Vec<ValidationIssue>,
    warnings: Vec<ValidationIssue>,
    /// connection ID -> containing module ID, for document-wide uniqueness.
    connection_owners: HashMap<String, String>,
}

impl Context {
    fn error(&mut self, code: IssueCode, module_id: &str, message: String) -> &mut ValidationIssue {
        self.errors.push(issue(Severity::Error, code, module_id, message));
        self.errors.last_mut().unwrap()
    }

    fn warn(&mut self, code: IssueCode, module_id: &str, message: String) -> &mut ValidationIssue {
        self.warnings.push(issue(Severity::Warning, code, module_id, message));
        self.warnings.last_mut().unwrap()
    }
}

/// All checks that only concern a single module record and its connections.
fn check_module_local(doc: &Document, key: &str, module: &Module, ctx: &mut Context) {
    if module.id != key {
        ctx.error(
            IssueCode::ModuleIdMismatch,
            key,
            format!("Module stored under key \"{key}\" declares ID \"{}\".", module.id),
        );
    }
    if !is_valid_id(key) {
        ctx.error(IssueCode::InvalidId, key, format!("Module ID \"{key}\" is not a valid ID."));
    }

    if key == doc.root_id {
        if let Some(parent) = &module.parent_id {
            ctx.error(
                IssueCode::RootHasParent,
                key,
                format!("Root module \"{key}\" must not have a parent (found \"{parent}\")."),
            );
        }
        if !module.inputs.is_empty() || !module.outputs.is_empty() {
            ctx.error(
                IssueCode::RootHasPorts,
                key,
                format!("Root module \"{key}\" must not have input or output ports."),
            );
        }
    } else if let Some(parent_id) = &module.parent_id {
        match find_module(doc, parent_id) {
            None => {
                ctx.error(
                    IssueCode::MissingParent,
                    key,
                    format!("Module \"{key}\" references missing parent \"{parent_id}\"."),
                );
            }
            Some(parent) if !parent.child_ids.iter().any(|c| c == key) => {
                ctx.error(
                    IssueCode::ParentChildMismatch,
                    key,
                    format!(
                        "Module \"{key}\" names \"{parent_id}\" as parent, but the parent does not list it as a child."
                    ),
                );
            }
            Some(_) => {}
        }
    } else {
        ctx.error(
            IssueCode::MultipleRoots,
            key,
            format!("Module \"{key}\" has no parent but is not the document root \"{}\".", doc.root_id),
        );
    }

    let mut seen_children: HashSet<&str> = HashSet::new();
    for child_id in &module.child_ids {
        if !seen_children.insert(child_id) {
            ctx.error(
                IssueCode::DuplicateChild,
                key,
                format!("Module \"{key}\" lists child \"{child_id}\" more than once."),
            );
            continue;
        }
        match find_module(doc, child_id) {
            None => {
                ctx.error(
                    IssueCode::MissingChild,
                    key,
                    format!("Module \"{key}\" lists missing child \"{child_id}\"."),
                );
            }
            Some(child) if child.parent_id.as_deref() != Some(key) => {
                // JavaScript renders a null parent as "null".
                let parent = child.parent_id.as_deref().unwrap_or("null");
                ctx.error(
                    IssueCode::ChildParentMismatch,
                    key,
                    format!("Module \"{key}\" lists child \"{child_id}\", but that module's parent is \"{parent}\"."),
                );
            }
            Some(_) => {}
        }
    }

    let mut port_ids: HashSet<&str> = HashSet::new();
    for port in module.all_ports() {
        if !port_ids.insert(&port.id) {
            ctx.error(
                IssueCode::DuplicatePortId,
                key,
                format!(
                    "Module \"{key}\" has more than one port with ID \"{}\" (inputs and outputs share one namespace).",
                    port.id
                ),
            )
            .port_id = Some(port.id.clone());
        }
        if !is_valid_id(&port.id) {
            ctx.error(
                IssueCode::InvalidId,
                key,
                format!("Port ID \"{}\" on module \"{key}\" is not a valid ID.", port.id),
            )
            .port_id = Some(port.id.clone());
        }
        if port.kind == PortKind::Event && !port.response.is_empty() {
            ctx.error(
                IssueCode::ResponseOnEventPort,
                key,
                format!(
                    "Event port \"{key}:{}\" has a response contract. Only request ports have a response.",
                    port.id
                ),
            )
            .port_id = Some(port.id.clone());
        }
    }

    let mut tags: HashSet<&str> = HashSet::new();
    for tag_id in &module.tags {
        if !tags.insert(tag_id) {
            ctx.error(
                IssueCode::DuplicateModuleTag,
                key,
                format!("Module \"{key}\" carries tag \"{tag_id}\" more than once."),
            );
        } else if doc.find_tag(tag_id).is_none() {
            ctx.error(
                IssueCode::UnknownTag,
                key,
                format!("Module \"{key}\" carries tag \"{tag_id}\", which is not in the document's tag list."),
            );
        }
    }

    let mut pairs: HashSet<String> = HashSet::new();
    for connection in &module.connections {
        let cid = &connection.id;
        match ctx.connection_owners.get(cid).cloned() {
            Some(previous) => {
                let message = if previous == key {
                    format!("Module \"{key}\" has more than one connection with ID \"{cid}\".")
                } else {
                    format!("Connection ID \"{cid}\" is used in both \"{previous}\" and \"{key}\".")
                };
                ctx.error(IssueCode::DuplicateConnectionId, key, message).connection_id = Some(cid.clone());
            }
            None => {
                ctx.connection_owners.insert(cid.clone(), key.to_string());
            }
        }
        if !is_valid_id(cid) {
            ctx.error(
                IssueCode::InvalidId,
                key,
                format!("Connection ID \"{cid}\" in module \"{key}\" is not a valid ID."),
            )
            .connection_id = Some(cid.clone());
        }
        ctx.errors.extend(check_connection(doc, key, &connection.from, &connection.to, Some(cid)));
        let pair = format!(
            "{}\u{1}{}",
            endpoint_key(&connection.from.module_id, &connection.from.port_id),
            endpoint_key(&connection.to.module_id, &connection.to.port_id)
        );
        if pairs.contains(&pair) {
            ctx.warn(
                IssueCode::DuplicateConnection,
                key,
                format!("Connection \"{cid}\" in \"{key}\" duplicates another connection between the same ports."),
            )
            .connection_id = Some(cid.clone());
        }
        pairs.insert(pair);
    }
}

/// Detects parent cycles and modules not reachable from the root. Iterative, O(n).
fn check_hierarchy(doc: &Document, ctx: &mut Context) {
    // Modules known to reach the root via their parent chain.
    let mut reaches_root: HashSet<&str> = HashSet::new();
    // Modules known not to reach the root (cycle, missing parent, foreign root).
    // Insertion-ordered: unreachable modules are reported in discovery order.
    let mut broken: IndexSet<&str> = IndexSet::new();
    let mut in_cycle: HashSet<&str> = HashSet::new();
    if find_module(doc, &doc.root_id).is_some() {
        reaches_root.insert(&doc.root_id);
    }

    for key in doc.modules.keys() {
        if reaches_root.contains(key) || broken.contains(key) {
            continue;
        }
        let mut path: Vec<&str> = Vec::new();
        let mut on_path: HashMap<&str, usize> = HashMap::new();
        let mut current: Option<&str> = Some(key);
        let mut ok = false;
        // `None` ends the chain at a non-root parentless module.
        while let Some(id) = current {
            if reaches_root.contains(id) {
                ok = true;
                break;
            }
            if broken.contains(id) {
                break;
            }
            if let Some(&index) = on_path.get(id) {
                let cycle = path[index..].to_vec();
                for member in &cycle {
                    if !in_cycle.insert(member) {
                        continue;
                    }
                    let mut chain: Vec<&str> = cycle.clone();
                    chain.push(cycle[0]);
                    ctx.error(
                        IssueCode::ParentCycle,
                        member,
                        format!("Module \"{member}\" is part of a parent cycle: {}.", chain.join(" -> ")),
                    );
                }
                break;
            }
            let Some(module) = find_module(doc, id) else {
                break; // missing parent, reported elsewhere
            };
            on_path.insert(id, path.len());
            path.push(id);
            current = module.parent_id.as_deref();
        }
        for id in path {
            if ok {
                reaches_root.insert(id);
            } else {
                broken.insert(id);
            }
        }
    }

    for id in broken {
        if in_cycle.contains(id) {
            continue;
        }
        let module = find_module(doc, id).expect("broken modules exist");
        // Direct problems (missing parent / extra root) are already reported.
        match &module.parent_id {
            Some(parent) if find_module(doc, parent).is_some() => {}
            _ => continue,
        }
        ctx.error(
            IssueCode::UnreachableModule,
            id,
            format!("Module \"{id}\" is not reachable from the root \"{}\".", doc.root_id),
        );
    }
}

fn check_unconnected_ports(doc: &Document, ctx: &mut Context) {
    let mut referenced: HashSet<String> = HashSet::new();
    for module in doc.modules.values() {
        for c in &module.connections {
            referenced.insert(endpoint_key(&c.from.module_id, &c.from.port_id));
            referenced.insert(endpoint_key(&c.to.module_id, &c.to.port_id));
        }
    }
    for (key, module) in doc.modules.iter() {
        for direction in [PortDirection::Input, PortDirection::Output] {
            for port in module.ports(direction) {
                if !referenced.contains(&endpoint_key(key, &port.id)) {
                    ctx.warn(
                        IssueCode::UnconnectedPort,
                        key,
                        format!("The {direction} \"{}\" ({key}:{}) is not connected.", port.name, port.id),
                    )
                    .port_id = Some(port.id.clone());
                }
            }
        }
    }
}

/// Checks the document's tag list: unique, valid IDs and `#rrggbb` colours.
/// Tag issues belong to no module.
fn check_tags(doc: &Document, ctx: &mut Context) {
    let mut seen: HashSet<&str> = HashSet::new();
    for tag in &doc.tags {
        let mut push = |code: IssueCode, message: String| {
            ctx.errors.push(ValidationIssue {
                severity: Severity::Error,
                code,
                message,
                module_id: None,
                port_id: None,
                connection_id: None,
                layout: IssueLayout::Standard,
            })
        };
        if !seen.insert(&tag.id) {
            push(IssueCode::DuplicateTagId, format!("The document defines tag \"{}\" more than once.", tag.id));
        }
        if !is_valid_id(&tag.id) {
            push(IssueCode::InvalidId, format!("Tag ID \"{}\" is not a valid ID.", tag.id));
        }
        if !is_valid_color(&tag.color) {
            push(
                IssueCode::InvalidTagColor,
                format!("Tag \"{}\" has colour \"{}\"; colours must be #rrggbb.", tag.id, tag.color),
            );
        }
    }
}

/// Validates the entire document.
pub fn validate_document(doc: &Document, options: ValidationOptions) -> ValidationResult {
    let mut ctx = Context::default();
    if find_module(doc, &doc.root_id).is_none() {
        ctx.error(IssueCode::RootMissing, &doc.root_id, format!("Root module \"{}\" does not exist.", doc.root_id));
    }
    check_tags(doc, &mut ctx);
    for (key, module) in doc.modules.iter() {
        check_module_local(doc, key, module, &mut ctx);
    }
    check_hierarchy(doc, &mut ctx);
    if options.warnings {
        check_unconnected_ports(doc, &mut ctx);
    }
    ValidationResult { valid: ctx.errors.is_empty(), errors: ctx.errors, warnings: ctx.warnings }
}

/// Validates a single module: its own record, its ports, the connections in
/// its internal graph and its parent chain. Does not scan the whole document.
pub fn validate_module(doc: &Document, module_id: &str, options: ValidationOptions) -> ValidationResult {
    let mut ctx = Context::default();
    let Some(module) = find_module(doc, module_id) else {
        ctx.error(IssueCode::MissingChild, module_id, format!("Module \"{module_id}\" does not exist."));
        return ValidationResult { valid: false, errors: ctx.errors, warnings: ctx.warnings };
    };
    check_module_local(doc, module_id, module, &mut ctx);

    let mut seen: HashSet<&str> = HashSet::from([module_id]);
    let mut current = module.parent_id.as_deref();
    while let Some(id) = current {
        if !seen.insert(id) {
            ctx.error(
                IssueCode::ParentCycle,
                module_id,
                format!("Module \"{module_id}\" has a parent cycle through \"{id}\"."),
            );
            break;
        }
        // A missing parent is reported by the local check or the ancestor's own check.
        let Some(ancestor) = find_module(doc, id) else { break };
        if ancestor.parent_id.is_none() && ancestor.id != doc.root_id {
            ctx.error(
                IssueCode::UnreachableModule,
                module_id,
                format!("Module \"{module_id}\" is not reachable from the root \"{}\".", doc.root_id),
            );
        }
        current = ancestor.parent_id.as_deref();
    }

    if options.warnings {
        let mut referenced: HashSet<String> = HashSet::new();
        let mut containers = vec![module];
        if let Some(parent) = module.parent_id.as_deref().and_then(|p| find_module(doc, p)) {
            containers.push(parent);
        }
        for container in containers {
            for c in &container.connections {
                referenced.insert(endpoint_key(&c.from.module_id, &c.from.port_id));
                referenced.insert(endpoint_key(&c.to.module_id, &c.to.port_id));
            }
        }
        for port in module.all_ports() {
            if !referenced.contains(&endpoint_key(module_id, &port.id)) {
                ctx.warn(
                    IssueCode::UnconnectedPort,
                    module_id,
                    format!("Port \"{}\" ({module_id}:{}) is not connected.", port.name, port.id),
                )
                .port_id = Some(port.id.clone());
            }
        }
    }
    ValidationResult { valid: ctx.errors.is_empty(), errors: ctx.errors, warnings: ctx.warnings }
}
