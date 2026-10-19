//! Structured, progressive-disclosure descriptions of modules. Used by the
//! CLI (`describe`, `export`) and the editor's Markdown export.

use serde::Serialize;

use crate::error::Result;
use crate::js;
use crate::model::{Connection, Document, Endpoint, Module, Port, PortDirection, PortKind, Tag};
use crate::queries::{find_module, find_port, get_module, get_module_path, walk_descendants, WalkOptions};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModuleRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PortDescription {
    pub id: String,
    pub name: String,
    /// Omitted for events.
    #[serde(skip_serializing_if = "PortKind::is_event")]
    pub kind: PortKind,
    /// For a request port: the request contract.
    pub description: String,
    /// Request ports only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointDescription {
    pub module_id: String,
    pub port_id: String,
    pub module_name: String,
    pub port_name: String,
    /// True when the endpoint is the containing module's own boundary port.
    pub boundary: bool,
    pub direction: Option<PortDirection>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConnectionDescription {
    pub id: String,
    pub from: EndpointDescription,
    pub to: EndpointDescription,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModuleDescription {
    pub id: String,
    pub name: String,
    /// Depth relative to the described module (0 = the module itself).
    pub depth: usize,
    pub parent: Option<ModuleRef>,
    pub path: Vec<ModuleRef>,
    /// Tags the module carries, resolved from the document's tag list.
    /// Omitted when empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<Tag>,
    pub description: String,
    pub inputs: Vec<PortDescription>,
    pub outputs: Vec<PortDescription>,
    /// Present when the module's internal graph is included.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<ModuleRef>>,
    /// Present when the module's internal graph is included.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connections: Option<Vec<ConnectionDescription>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DescribeResult {
    pub module: ModuleDescription,
    /// Descendants within the requested depth, pre-order.
    pub descendants: Vec<ModuleDescription>,
}

pub fn describe_endpoint(doc: &Document, container_id: &str, ep: &Endpoint) -> EndpointDescription {
    let module = find_module(doc, &ep.module_id);
    let port = module.and_then(|m| find_port(m, &ep.port_id));
    EndpointDescription {
        module_id: ep.module_id.clone(),
        port_id: ep.port_id.clone(),
        module_name: module.map_or_else(|| "(missing module)".to_string(), |m| m.name.clone()),
        port_name: port.map_or_else(|| "(missing port)".to_string(), |p| p.port.name.clone()),
        boundary: ep.module_id == container_id,
        direction: port.map(|p| p.direction),
    }
}

pub fn describe_connection(doc: &Document, container_id: &str, c: &Connection) -> ConnectionDescription {
    ConnectionDescription {
        id: c.id.clone(),
        from: describe_endpoint(doc, container_id, &c.from),
        to: describe_endpoint(doc, container_id, &c.to),
    }
}

fn module_ref(m: &Module) -> ModuleRef {
    ModuleRef { id: m.id.clone(), name: m.name.clone() }
}

pub fn port_description(p: &Port) -> PortDescription {
    PortDescription {
        id: p.id.clone(),
        name: p.name.clone(),
        kind: p.kind,
        description: p.description.clone(),
        response: (p.kind == PortKind::Request).then(|| p.response.clone()),
    }
}

/// The tags a module carries, in its order. Unknown tag IDs (an invalid
/// document) are shown with their ID as the name.
pub fn module_tags(doc: &Document, module: &Module) -> Vec<Tag> {
    module
        .tags
        .iter()
        .map(|id| doc.find_tag(id).cloned().unwrap_or_else(|| Tag::new(id.clone(), id.clone(), String::new())))
        .collect()
}

fn describe_one(doc: &Document, module: &Module, depth: usize, include_internals: bool) -> Result<ModuleDescription> {
    let parent = module.parent_id.as_deref().and_then(|p| find_module(doc, p));
    let mut description = ModuleDescription {
        id: module.id.clone(),
        name: module.name.clone(),
        depth,
        parent: parent.map(module_ref),
        path: get_module_path(doc, &module.id)?.into_iter().map(module_ref).collect(),
        tags: module_tags(doc, module),
        description: module.description.clone(),
        inputs: module.inputs.iter().map(port_description).collect(),
        outputs: module.outputs.iter().map(port_description).collect(),
        children: None,
        connections: None,
    };
    if include_internals {
        description.children =
            Some(module.child_ids.iter().map(|id| get_module(doc, id).map(module_ref)).collect::<Result<_>>()?);
        description.connections =
            Some(module.connections.iter().map(|c| describe_connection(doc, &module.id, c)).collect());
    }
    Ok(description)
}

/// Describes a module.
///
/// depth 0: public contract only (name, description, ports).
/// depth 1: plus immediate children and internal connections.
/// depth N: recursively N levels; a module at relative depth d includes its
/// internals when d < N. `None` means unlimited. Sibling and unrelated
/// modules are never included.
pub fn describe_module(doc: &Document, module_id: &str, depth: Option<usize>) -> Result<DescribeResult> {
    let visits = walk_descendants(doc, module_id, WalkOptions { include_self: true, max_depth: depth })?;
    let limit = depth.unwrap_or(usize::MAX);
    let mut described = visits
        .into_iter()
        .map(|v| describe_one(doc, v.module, v.depth, v.depth < limit))
        .collect::<Result<Vec<_>>>()?
        .into_iter();
    let module = described.next().expect("the walk includes the start module");
    Ok(DescribeResult { module, descendants: described.collect() })
}

/// IDs of the modules whose Markdown a depth-limited describe needs.
pub fn describe_module_ids(doc: &Document, module_id: &str, depth: Option<usize>) -> Result<Vec<String>> {
    Ok(walk_descendants(doc, module_id, WalkOptions { include_self: true, max_depth: depth })?
        .into_iter()
        .map(|v| v.module.id.clone())
        .collect())
}

// ---------------------------------------------------------------------------
// Markdown rendering

fn body(text: &str) -> String {
    let trimmed = js::trim(text);
    if trimmed.is_empty() {
        "_No description._".to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn format_endpoint(ep: &EndpointDescription) -> String {
    let id = format!("`{}:{}`", ep.module_id, ep.port_id);
    if ep.boundary {
        let direction = ep.direction.map_or("port", PortDirection::as_str);
        return format!("{} (boundary {direction}, {id})", ep.port_name);
    }
    format!("{} › {} ({id})", ep.module_name, ep.port_name)
}

/// How a request port takes part in the exchange, from its direction.
pub fn request_role(direction: PortDirection) -> &'static str {
    match direction {
        PortDirection::Input => "request/response, handled here",
        PortDirection::Output => "request/response, sent from here",
    }
}

/// Appends one port's section. Event ports have a single contract; request
/// ports have a request and a response contract.
pub fn render_port(lines: &mut Vec<String>, p: &PortDescription, direction: PortDirection) {
    lines.push(format!("### {}", p.name));
    lines.push(format!("- ID: `{}`", p.id));
    let Some(response) = &p.response else {
        lines.push(String::new());
        lines.push(body(&p.description));
        lines.push(String::new());
        return;
    };
    lines.push(format!("- Kind: {}", request_role(direction)));
    lines.push(String::new());
    lines.extend(["#### Request".to_string(), String::new(), body(&p.description), String::new()]);
    lines.extend(["#### Response".to_string(), String::new(), body(response), String::new()]);
}

fn render_ports(lines: &mut Vec<String>, title: &str, ports: &[PortDescription], direction: PortDirection) {
    lines.push(format!("## {title}"));
    lines.push(String::new());
    if ports.is_empty() {
        lines.push("_None._".to_string());
        lines.push(String::new());
        return;
    }
    for p in ports {
        render_port(lines, p, direction);
    }
}

/// Renders one module section. Consistent headings for humans and agents.
/// `link_for` turns submodule entries into Markdown links.
pub fn render_module_section(m: &ModuleDescription, link_for: Option<&dyn Fn(&str) -> String>) -> String {
    let mut lines: Vec<String> =
        vec![format!("# {}", m.name), String::new(), "## Identity".to_string(), format!("- ID: `{}`", m.id)];
    lines.push(match &m.parent {
        Some(parent) => format!("- Parent: {} (`{}`)", parent.name, parent.id),
        None => "- Parent: _none (root)_".to_string(),
    });
    let path: Vec<&str> = m.path.iter().map(|p| p.name.as_str()).collect();
    lines.push(format!("- Path: {}", path.join(" / ")));
    if !m.tags.is_empty() {
        let tags: Vec<String> = m.tags.iter().map(|t| format!("{} (`{}`)", t.name, t.id)).collect();
        lines.push(format!("- Tags: {}", tags.join(", ")));
    }
    lines.push(String::new());
    lines.extend(["## Description".to_string(), String::new(), body(&m.description), String::new()]);
    render_ports(&mut lines, "Inputs", &m.inputs, PortDirection::Input);
    render_ports(&mut lines, "Outputs", &m.outputs, PortDirection::Output);
    if let Some(children) = &m.children {
        lines.extend(["## Submodules".to_string(), String::new()]);
        if children.is_empty() {
            lines.push("_None._".to_string());
        }
        for c in children {
            let name = match link_for {
                Some(link) => format!("[{}]({})", c.name, link(&c.id)),
                None => c.name.clone(),
            };
            lines.push(format!("- {name} (`{}`)", c.id));
        }
        lines.push(String::new());
    }
    if let Some(connections) = &m.connections {
        lines.extend(["## Internal Connections".to_string(), String::new()]);
        if connections.is_empty() {
            lines.push("_None._".to_string());
        }
        for c in connections {
            lines.push(format!("- `{}`: {} -> {}", c.id, format_endpoint(&c.from), format_endpoint(&c.to)));
        }
        lines.push(String::new());
    }
    format!("{}\n", js::trim_end(&lines.join("\n")))
}

pub fn render_describe_markdown(result: &DescribeResult) -> String {
    std::iter::once(&result.module)
        .chain(result.descendants.iter())
        .map(|m| render_module_section(m, None))
        .collect::<Vec<_>>()
        .join("\n---\n\n")
}

/// Markdown documentation for a module, limited to `depth` levels.
pub fn describe_module_markdown(doc: &Document, module_id: &str, depth: Option<usize>) -> Result<String> {
    Ok(render_describe_markdown(&describe_module(doc, module_id, depth)?))
}

/// Markdown documentation of the whole hierarchy in one string.
pub fn export_document_markdown(doc: &Document) -> Result<String> {
    let mut header = vec![format!("<!-- Generated by MDG from \"{}\". -->", doc.metadata.title), String::new()];
    if let Some(description) = doc.metadata.description.as_deref().filter(|d| !d.is_empty()) {
        header.push(js::trim(description).to_string());
        header.push(String::new());
    }
    let result = describe_module(doc, &doc.root_id, None)?;
    Ok(format!("{}\n{}", header.join("\n"), render_describe_markdown(&result)))
}

fn is_windows_reserved(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    if matches!(lower.as_str(), "con" | "prn" | "aux" | "nul") {
        return true;
    }
    let digit_suffix =
        |prefix: &str| lower.len() == 4 && lower.starts_with(prefix) && lower.as_bytes()[3].is_ascii_digit();
    digit_suffix("com") || digit_suffix("lpt")
}

pub fn module_file_name(module_id: &str) -> String {
    if is_windows_reserved(module_id) {
        format!("{module_id}_.md")
    } else {
        format!("{module_id}.md")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportedFile {
    pub path: String,
    pub content: String,
}

/// One Markdown file per module plus `index.md` with the hierarchy. Each
/// module file includes its public contract, children and internal graph.
pub fn export_module_files(doc: &Document) -> Result<Vec<ExportedFile>> {
    let visits = walk_descendants(doc, &doc.root_id, WalkOptions { include_self: true, max_depth: None })?;
    let mut files = Vec::new();
    let mut index_lines = vec![format!("# {}", doc.metadata.title), String::new()];
    if let Some(description) = doc.metadata.description.as_deref().filter(|d| !d.is_empty()) {
        index_lines.push(js::trim(description).to_string());
        index_lines.push(String::new());
    }
    let link: &dyn Fn(&str) -> String = &module_file_name;
    for visit in visits {
        let module = visit.module;
        let file = module_file_name(&module.id);
        index_lines.push(format!("{}- [{}]({file}) (`{}`)", "  ".repeat(visit.depth), module.name, module.id));
        let section = render_module_section(&describe_one(doc, module, 0, true)?, Some(link));
        files.push(ExportedFile { path: file, content: section });
    }
    files.insert(0, ExportedFile { path: "index.md".to_string(), content: format!("{}\n", index_lines.join("\n")) });
    Ok(files)
}
