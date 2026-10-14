//! Read-only commands: info, tree, describe, inputs/outputs, connections,
//! trace, validate and export.

use std::path::Path;

use mdg_core::{
    build_tree, count_connections, count_ports, describe_connection, describe_module, describe_module_ids,
    export_document_markdown, export_module_files, format_endpoint, get_module, js, read_mdg, render_describe_markdown,
    render_tree, trace, validate_document, MdgError, PortDirection, Result, TraceDirection, TraceOptions, TraceResult,
    TraceRouteEnd, TreeOptions, ValidationIssue, ValidationOptions,
};
use serde::Serialize;

use crate::commander::Parsed;
use crate::program::Action;
use crate::resolve::{resolve_module_id, resolve_port_id};
use crate::shared::{exit, fs_message, read_partial, read_skeleton, write_text_file, Context};

type Args = Parsed<Action>;

pub fn info(ctx: &mut Context<'_>, p: &Args) -> Result<()> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Info {
        title: String,
        schema_version: String,
        container_version: String,
        root_id: String,
        module_count: usize,
        port_count: usize,
        connection_count: usize,
        file_size: u64,
    }
    let read = read_skeleton(&p.args[0])?;
    let doc = &read.doc;
    let header = &read.read.header;
    let info = Info {
        title: doc.metadata.title.clone(),
        schema_version: read.read.index.schema_version.clone(),
        container_version: format!("{}.{}", header.container_major, header.container_minor),
        root_id: doc.root_id.clone(),
        module_count: doc.modules.len(),
        port_count: count_ports(doc),
        connection_count: count_connections(doc),
        file_size: read.read.size,
    };
    if p.flag("json") {
        ctx.print_json(&info);
        return Ok(());
    }
    ctx.print(
        &[
            format!("Title:             {}", info.title),
            format!("Schema version:    {}", info.schema_version),
            format!("Container version: {}", info.container_version),
            format!("Root ID:           {}", info.root_id),
            format!("Modules:           {}", info.module_count),
            format!("Ports:             {}", info.port_count),
            format!("Connections:       {}", info.connection_count),
            format!("File size:         {} bytes", info.file_size),
        ]
        .join("\n"),
    );
    Ok(())
}

pub fn tree(ctx: &mut Context<'_>, p: &Args) -> Result<()> {
    let read = read_skeleton(&p.args[0])?;
    let options = TreeOptions { root_id: None, depth: p.int("depth").map(|d| d as usize), ids: p.flag("ids") };
    if p.flag("json") {
        ctx.print_json(&build_tree(&read.doc, &options)?);
    } else {
        ctx.print(&render_tree(&read.doc, &options)?);
    }
    Ok(())
}

pub fn describe(ctx: &mut Context<'_>, p: &Args) -> Result<()> {
    let depth = p.int("depth").map(|d| d as usize);
    let mut module_id = String::new();
    let read = read_partial(&p.args[0], |skeleton| {
        module_id = resolve_module_id(skeleton, &p.args[1])?;
        describe_module_ids(skeleton, &module_id, depth)
    })?;
    let result = describe_module(&read.doc, &module_id, depth)?;
    let json = p.flag("json");
    let text = if json { format!("{}\n", js::to_pretty_json(&result)) } else { render_describe_markdown(&result) };
    if let Some(output) = p.string("output") {
        write_text_file(Path::new(&output), &text)?;
        if !json {
            ctx.print(&format!("Wrote {output}"));
        }
        return Ok(());
    }
    ctx.io.out(&text);
    Ok(())
}

pub fn ports(ctx: &mut Context<'_>, p: &Args, direction: PortDirection) -> Result<()> {
    #[derive(Serialize)]
    struct PortJson {
        id: String,
        name: String,
        description: String,
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct PortsJson {
        module_id: String,
        module_name: String,
        direction: &'static str,
        ports: Vec<PortJson>,
    }
    let mut module_id = String::new();
    let read = read_partial(&p.args[0], |skeleton| {
        module_id = resolve_module_id(skeleton, &p.args[1])?;
        Ok(vec![module_id.clone()])
    })?;
    let module = get_module(&read.doc, &module_id)?;
    let ports: Vec<PortJson> = module
        .ports(direction)
        .iter()
        .map(|port| PortJson { id: port.id.clone(), name: port.name.clone(), description: port.description.clone() })
        .collect();
    let plural = match direction {
        PortDirection::Input => "inputs",
        PortDirection::Output => "outputs",
    };
    if p.flag("json") {
        ctx.print_json(&PortsJson { module_id, module_name: module.name.clone(), direction: plural, ports });
        return Ok(());
    }
    let title = match direction {
        PortDirection::Input => "Inputs",
        PortDirection::Output => "Outputs",
    };
    let mut lines = vec![format!("# {title} of {} (`{module_id}`)", module.name), String::new()];
    if ports.is_empty() {
        lines.push("_None._".to_string());
    }
    for port in &ports {
        let body = js::trim(&port.description);
        lines.extend([
            format!("## {}", port.name),
            format!("- ID: `{}`", port.id),
            String::new(),
            if body.is_empty() { "_No description._".to_string() } else { body.to_string() },
            String::new(),
        ]);
    }
    ctx.print(js::trim_end(&lines.join("\n")));
    Ok(())
}

pub fn connections(ctx: &mut Context<'_>, p: &Args) -> Result<()> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct ConnectionsJson {
        module_id: String,
        module_name: String,
        connections: Vec<mdg_core::ConnectionDescription>,
    }
    let read = read_skeleton(&p.args[0])?;
    let doc = &read.doc;
    let module_id = resolve_module_id(doc, &p.args[1])?;
    let module = get_module(doc, &module_id)?;
    let connections: Vec<_> = module.connections.iter().map(|c| describe_connection(doc, &module_id, c)).collect();
    if p.flag("json") {
        ctx.print_json(&ConnectionsJson { module_id, module_name: module.name.clone(), connections });
        return Ok(());
    }
    let mut lines = vec![format!("Internal connections of {} ({module_id}):", module.name)];
    if connections.is_empty() {
        lines.push("  (none)".to_string());
    }
    for c in &connections {
        lines.push(format!("- {}: {} -> {}", c.id, format_endpoint(&c.from), format_endpoint(&c.to)));
    }
    ctx.print(&lines.join("\n").replace('`', ""));
    Ok(())
}

fn format_trace(result: &TraceResult) -> String {
    let direction = match result.direction {
        TraceDirection::Forward => "Forward",
        TraceDirection::Backward => "Reverse",
    };
    let mut lines = vec![
        format!("{direction} trace from {}", result.start),
        "Routes describe possible structural message flow, not execution order.".to_string(),
        String::new(),
    ];
    if result.routes.is_empty() {
        lines.push("No connections from this port.".to_string());
    }
    for (i, route) in result.routes.iter().enumerate() {
        let end = match route.end {
            TraceRouteEnd::Terminal => "ends at a port with no further connections".to_string(),
            TraceRouteEnd::Cycle => format!("cycle: returns to step {}", route.cycle_index.unwrap_or(0) + 1),
            TraceRouteEnd::MaxDepth => "stopped at maximum depth".to_string(),
        };
        lines.push(format!("Route {} ({end}):", i + 1));
        for (j, hop) in route.hops.iter().enumerate() {
            let via = match (&hop.connection_id, &hop.container_id) {
                (Some(c), Some(container)) => format!("  via {c} in {container}"),
                _ => String::new(),
            };
            lines.push(format!(
                "  {}. {} › {} [{}] ({}:{}){via}",
                j + 1,
                hop.module_name,
                hop.port_name,
                hop.direction,
                hop.module_id,
                hop.port_id
            ));
        }
        lines.push(String::new());
    }
    if result.truncated {
        lines.push("Output truncated by --max-routes.".to_string());
    }
    js::trim_end(&lines.join("\n")).to_string()
}

pub fn trace_command(ctx: &mut Context<'_>, p: &Args) -> Result<()> {
    let read = read_skeleton(&p.args[0])?;
    let doc = &read.doc;
    let module_id = resolve_module_id(doc, &p.args[1])?;
    let port_id = resolve_port_id(doc, &module_id, &p.args[2])?;
    let options = TraceOptions {
        direction: if p.flag("reverse") { TraceDirection::Backward } else { TraceDirection::Forward },
        max_depth: p.int("maxDepth").unwrap_or(64) as usize,
        max_routes: p.int("maxRoutes").unwrap_or(1000) as usize,
    };
    let result = trace(doc, &module_id, &port_id, options)?;
    if p.flag("json") {
        ctx.print_json(&result);
    } else {
        ctx.print(&format_trace(&result));
    }
    Ok(())
}

pub fn format_issue(issue: &ValidationIssue) -> String {
    let parts: Vec<String> =
        [("module", &issue.module_id), ("port", &issue.port_id), ("connection", &issue.connection_id)]
            .into_iter()
            .filter_map(|(label, value)| value.as_ref().map(|v| format!("{label}={v}")))
            .collect();
    format!("{:<7} {} {}\n        {}", issue.severity.as_str(), issue.code.as_str(), parts.join(" "), issue.message)
}

pub fn validate(ctx: &mut Context<'_>, p: &Args) -> Result<()> {
    let doc = read_mdg(Path::new(&p.args[0]))?;
    let result = validate_document(&doc, ValidationOptions { warnings: p.flag("warnings") });
    if !result.valid || (p.flag("strict") && !result.warnings.is_empty()) {
        ctx.exit_code = exit::FAILURE;
    }
    if p.flag("json") {
        ctx.print_json(&result);
        return Ok(());
    }
    let mut lines = vec![format!(
        "{}: {} error(s), {} warning(s)",
        if result.valid { "VALID" } else { "INVALID" },
        result.errors.len(),
        result.warnings.len()
    )];
    lines.extend(result.errors.iter().chain(result.warnings.iter()).map(format_issue));
    ctx.print(&lines.join("\n"));
    Ok(())
}

pub fn export(ctx: &mut Context<'_>, p: &Args) -> Result<()> {
    let doc = read_mdg(Path::new(&p.args[0]))?;
    if let Some(split) = p.string("split") {
        let files = export_module_files(&doc)?;
        let dir = Path::new(&split);
        std::fs::create_dir_all(dir)
            .map_err(|e| MdgError::io(format!("Cannot create {split}: {}", fs_message(&e, "mkdir", dir))))?;
        for file in &files {
            write_text_file(&dir.join(&file.path), &file.content)?;
        }
        ctx.print(&format!("Wrote {} files to {split}", files.len()));
        return Ok(());
    }
    let markdown = export_document_markdown(&doc)?;
    if let Some(output) = p.string("output") {
        write_text_file(Path::new(&output), &markdown)?;
        ctx.print(&format!("Wrote {output}"));
        return Ok(());
    }
    ctx.io.out(&markdown);
    Ok(())
}
