//! Editing commands. Every edit reads the whole document, applies one core
//! operation, validates the result and only then writes atomically. Invalid
//! results never reach the disk.

use std::path::Path;

use mdg_core::{
    add_connection, add_module, add_port, assert_valid, create_document, get_connection, get_module, move_module,
    read_mdg, remove_connection, remove_module, remove_port, rename_module, rename_port, route_connection,
    set_port_kind, update_module_description, update_port_description, update_port_response, write_mdg,
    AddConnectionOptions, AddModuleOptions, AddPortOptions, CreateDocumentOptions, Document, JsMap, MdgError,
    PortDirection, PortKind, Result, RouteOptions, RouteStep,
};
use serde_json::{json, Map, Value};

use crate::commander::Parsed;
use crate::program::Action;
use crate::resolve::{resolve_endpoint, resolve_module_id, resolve_port_id};
use crate::shared::{read_description, read_markdown, Context};

type Args = Parsed<Action>;

struct Outcome {
    document: Document,
    /// Machine-readable details merged into the JSON result.
    result: Vec<(&'static str, Value)>,
    /// Human-readable summary.
    message: String,
}

/// Shared edit pipeline: read, apply a pure core operation, validate, and
/// only then write atomically (unless `--dry-run`).
fn run_edit(ctx: &mut Context<'_>, p: &Args, edit: impl FnOnce(&Document) -> Result<Outcome>) -> Result<()> {
    let file = Path::new(&p.args[0]);
    let doc = read_mdg(file)?;
    let outcome = edit(&doc)?;
    assert_valid(&outcome.document)?;
    let dry_run = p.flag("dryRun");
    if !dry_run {
        write_mdg(file, &outcome.document)?;
    }
    if p.flag("json") {
        let mut map = Map::new();
        map.insert("ok".into(), Value::Bool(true));
        map.insert("dryRun".into(), Value::Bool(dry_run));
        for (key, value) in outcome.result {
            map.insert(key.into(), value);
        }
        ctx.print_json(&Value::Object(map));
        return Ok(());
    }
    ctx.print(&format!("{}{}", if dry_run { "[dry run] " } else { "" }, outcome.message));
    Ok(())
}

fn require_description(description: Option<String>) -> Result<String> {
    description.ok_or_else(|| MdgError::argument("Provide --description <markdown> or --description-file <path>."))
}

fn description_options(ctx: &mut Context<'_>, p: &Args) -> Result<Option<String>> {
    read_description(ctx, p.string("description"), p.string("descriptionFile"))
}

fn response_options(ctx: &mut Context<'_>, p: &Args) -> Result<Option<String>> {
    read_markdown(ctx, "response", p.string("response"), p.string("responseFile"))
}

fn parse_kind(name: &str) -> Result<PortKind> {
    match name {
        "event" => Ok(PortKind::Event),
        "request" => Ok(PortKind::Request),
        other => Err(MdgError::argument(format!("Unknown port kind \"{other}\". Use \"event\" or \"request\"."))),
    }
}

pub fn new(ctx: &mut Context<'_>, p: &Args) -> Result<()> {
    let file = &p.args[0];
    if !p.flag("force") && Path::new(file).exists() {
        return Err(MdgError::io(format!("{file} already exists. Use --force to overwrite it.")));
    }
    let description = description_options(ctx, p)?;
    let doc = create_document(CreateDocumentOptions {
        title: p.string("title"),
        root_name: p.string("rootName"),
        root_id: p.string("rootId"),
        root_description: description,
        ..Default::default()
    })?;
    write_mdg(Path::new(file), &doc)?;
    if p.flag("json") {
        ctx.print_json(&json!({ "ok": true, "file": file, "rootId": doc.root_id }));
    } else {
        ctx.print(&format!("Created {file} (root {})", doc.root_id));
    }
    Ok(())
}

pub fn module_add(ctx: &mut Context<'_>, p: &Args) -> Result<()> {
    let description = description_options(ctx, p)?;
    run_edit(ctx, p, |doc| {
        let parent_id = resolve_module_id(doc, &p.string("parent").unwrap_or_default())?;
        let result = add_module(
            doc,
            AddModuleOptions {
                parent_id: parent_id.clone(),
                name: p.string("name").unwrap_or_default(),
                description,
                id: p.string("id"),
                index: p.int("index").map(|i| i as usize),
                ..Default::default()
            },
        )?;
        Ok(Outcome {
            document: result.document,
            message: format!("Added module {} under {parent_id}", result.id),
            result: vec![("moduleId", json!(result.id)), ("parentId", json!(parent_id))],
        })
    })
}

pub fn module_remove(ctx: &mut Context<'_>, p: &Args) -> Result<()> {
    run_edit(ctx, p, |doc| {
        let module_id = resolve_module_id(doc, &p.args[1])?;
        let parent_id = get_module(doc, &module_id)?.parent_id.clone();
        let document = remove_module(doc, &module_id, p.flag("cascade"))?;
        let removed_modules: Vec<&str> =
            doc.modules.keys().into_iter().filter(|id| !document.modules.contains_key(id)).collect();
        let removed_connections: Vec<&str> = match &parent_id {
            None => vec![],
            Some(parent) => {
                let after = get_module(&document, parent)?;
                get_module(doc, parent)?
                    .connections
                    .iter()
                    .map(|c| c.id.as_str())
                    .filter(|id| !after.connections.iter().any(|c| c.id == *id))
                    .collect()
            }
        };
        let mut message = format!("Removed {} module(s): {}", removed_modules.len(), removed_modules.join(", "));
        if !removed_connections.is_empty() {
            message.push_str(&format!(
                "; {} connection(s): {}",
                removed_connections.len(),
                removed_connections.join(", ")
            ));
        }
        let result =
            vec![("removedModules", json!(removed_modules)), ("removedConnections", json!(removed_connections))];
        Ok(Outcome { document, result, message })
    })
}

pub fn module_rename(ctx: &mut Context<'_>, p: &Args) -> Result<()> {
    let name = &p.args[2];
    run_edit(ctx, p, |doc| {
        let module_id = resolve_module_id(doc, &p.args[1])?;
        let document = rename_module(doc, &module_id, name)?;
        let trimmed = mdg_core::js::trim(name);
        Ok(Outcome {
            document,
            message: format!("Renamed {module_id} to \"{trimmed}\""),
            result: vec![("moduleId", json!(module_id)), ("name", json!(trimmed))],
        })
    })
}

pub fn module_move(ctx: &mut Context<'_>, p: &Args) -> Result<()> {
    run_edit(ctx, p, |doc| {
        let module_id = resolve_module_id(doc, &p.args[1])?;
        let parent_id = resolve_module_id(doc, &p.string("to").unwrap_or_default())?;
        let document = move_module(doc, &module_id, &parent_id, p.int("index").map(|i| i as usize))?;
        Ok(Outcome {
            document,
            message: format!("Moved {module_id} under {parent_id}"),
            result: vec![("moduleId", json!(module_id)), ("parentId", json!(parent_id))],
        })
    })
}

pub fn module_set_description(ctx: &mut Context<'_>, p: &Args) -> Result<()> {
    let description = require_description(description_options(ctx, p)?)?;
    run_edit(ctx, p, |doc| {
        let module_id = resolve_module_id(doc, &p.args[1])?;
        Ok(Outcome {
            document: update_module_description(doc, &module_id, &description)?,
            message: format!("Updated description of {module_id}"),
            result: vec![("moduleId", json!(module_id))],
        })
    })
}

pub fn port_add(ctx: &mut Context<'_>, p: &Args) -> Result<()> {
    let description = description_options(ctx, p)?;
    let response = response_options(ctx, p)?;
    let direction_name = p.string("direction").unwrap_or_default();
    let direction = if direction_name == "input" { PortDirection::Input } else { PortDirection::Output };
    let kind = parse_kind(p.string("kind").as_deref().unwrap_or("event"))?;
    if kind == PortKind::Event && response.is_some() {
        return Err(MdgError::argument("--response and --response-file require --kind request."));
    }
    run_edit(ctx, p, |doc| {
        let module_id = resolve_module_id(doc, &p.args[1])?;
        let result = add_port(
            doc,
            &module_id,
            direction,
            AddPortOptions {
                name: p.string("name").unwrap_or_default(),
                kind,
                description,
                response,
                id: p.string("id"),
                ..Default::default()
            },
        )?;
        let mut details =
            vec![("moduleId", json!(module_id)), ("portId", json!(result.id)), ("direction", json!(direction))];
        let what = match kind {
            PortKind::Event => direction.to_string(),
            PortKind::Request => {
                details.push(("kind", json!(kind)));
                format!("request {direction}")
            }
        };
        Ok(Outcome {
            document: result.document,
            message: format!("Added {what} {module_id}:{}", result.id),
            result: details,
        })
    })
}

pub fn port_set_response(ctx: &mut Context<'_>, p: &Args) -> Result<()> {
    let response = response_options(ctx, p)?
        .ok_or_else(|| MdgError::argument("Provide --response <markdown> or --response-file <path>."))?;
    run_edit(ctx, p, |doc| {
        let module_id = resolve_module_id(doc, &p.args[1])?;
        let port_id = resolve_port_id(doc, &module_id, &p.args[2])?;
        Ok(Outcome {
            document: update_port_response(doc, &module_id, &port_id, &response)?,
            message: format!("Updated response of {module_id}:{port_id}"),
            result: vec![("moduleId", json!(module_id)), ("portId", json!(port_id))],
        })
    })
}

pub fn port_set_kind(ctx: &mut Context<'_>, p: &Args) -> Result<()> {
    let kind = parse_kind(&p.args[3])?;
    run_edit(ctx, p, |doc| {
        let module_id = resolve_module_id(doc, &p.args[1])?;
        let port_id = resolve_port_id(doc, &module_id, &p.args[2])?;
        Ok(Outcome {
            document: set_port_kind(doc, &module_id, &port_id, kind)?,
            message: format!(
                "{module_id}:{port_id} is now {} port",
                if kind == PortKind::Event { "an event" } else { "a request/response" }
            ),
            result: vec![("moduleId", json!(module_id)), ("portId", json!(port_id)), ("kind", json!(kind))],
        })
    })
}

pub fn port_remove(ctx: &mut Context<'_>, p: &Args) -> Result<()> {
    run_edit(ctx, p, |doc| {
        let module_id = resolve_module_id(doc, &p.args[1])?;
        let port_id = resolve_port_id(doc, &module_id, &p.args[2])?;
        Ok(Outcome {
            document: remove_port(doc, &module_id, &port_id, p.flag("cascade"))?,
            message: format!("Removed port {module_id}:{port_id}"),
            result: vec![("moduleId", json!(module_id)), ("portId", json!(port_id))],
        })
    })
}

pub fn port_rename(ctx: &mut Context<'_>, p: &Args) -> Result<()> {
    let name = &p.args[3];
    run_edit(ctx, p, |doc| {
        let module_id = resolve_module_id(doc, &p.args[1])?;
        let port_id = resolve_port_id(doc, &module_id, &p.args[2])?;
        let document = rename_port(doc, &module_id, &port_id, name)?;
        let trimmed = mdg_core::js::trim(name);
        Ok(Outcome {
            document,
            message: format!("Renamed {module_id}:{port_id} to \"{trimmed}\""),
            result: vec![("moduleId", json!(module_id)), ("portId", json!(port_id)), ("name", json!(trimmed))],
        })
    })
}

pub fn port_set_description(ctx: &mut Context<'_>, p: &Args) -> Result<()> {
    let description = require_description(description_options(ctx, p)?)?;
    run_edit(ctx, p, |doc| {
        let module_id = resolve_module_id(doc, &p.args[1])?;
        let port_id = resolve_port_id(doc, &module_id, &p.args[2])?;
        Ok(Outcome {
            document: update_port_description(doc, &module_id, &port_id, &description)?,
            message: format!("Updated description of {module_id}:{port_id}"),
            result: vec![("moduleId", json!(module_id)), ("portId", json!(port_id))],
        })
    })
}

fn describe_step(step: &RouteStep) -> String {
    match step {
        RouteStep::CreatePort { module_id, direction, port } => {
            let kind = if port.kind == PortKind::Request { "request " } else { "" };
            format!("  + {kind}{direction} {module_id}:{} \"{}\"", port.id, port.name)
        }
        RouteStep::ReusePort { module_id, direction, port_id } => {
            format!("  = {direction} {module_id}:{port_id} (reused)")
        }
        RouteStep::CreateConnection { container_id, connection_id, from, to } => {
            format!("  + {connection_id} in {container_id}: {from} -> {to}")
        }
        RouteStep::ReuseConnection { container_id, connection_id, .. } => {
            format!("  = {connection_id} in {container_id} (reused)")
        }
    }
}

pub fn connect(ctx: &mut Context<'_>, p: &Args) -> Result<()> {
    let route = p.flag("route");
    let via_specs = p.list("via");
    let output_name = p.string("outputName");
    let input_name = p.string("inputName");
    let reuse = p.flag("reuse");
    // Empty strings are "not given", like JavaScript's truthiness checks.
    let given = |v: &Option<String>| v.as_deref().is_some_and(|s| !s.is_empty());
    if !route && (via_specs.is_some() || given(&output_name) || given(&input_name) || !reuse) {
        return Err(MdgError::argument("--via, --no-reuse, --output-name and --input-name require --route."));
    }
    if route && (given(&p.string("id")) || given(&p.string("container"))) {
        return Err(MdgError::argument("--id and --container cannot be combined with --route."));
    }
    let (from_spec, to_spec) = (&p.args[1], &p.args[2]);
    run_edit(ctx, p, |doc| {
        let from = resolve_endpoint(doc, from_spec)?;
        let to = resolve_endpoint(doc, to_spec)?;
        if !route {
            let container_id = match p.string("container") {
                Some(c) => Some(resolve_module_id(doc, &c)?),
                None => None,
            };
            let result = add_connection(
                doc,
                AddConnectionOptions {
                    from: from.clone(),
                    to: to.clone(),
                    container_id,
                    id: p.string("id"),
                    ..Default::default()
                },
            )?;
            return Ok(Outcome {
                document: result.document,
                message: format!("Connected {from} -> {to} ({} in {})", result.id, result.container_id),
                result: vec![
                    ("connectionId", json!(result.id)),
                    ("containerId", json!(result.container_id)),
                    ("from", json!(from)),
                    ("to", json!(to)),
                ],
            });
        }
        let mut via = JsMap::new();
        for spec in via_specs.iter().flatten() {
            let endpoint = resolve_endpoint(doc, spec)?;
            via.insert(endpoint.module_id, endpoint.port_id);
        }
        let result = route_connection(
            doc,
            &from,
            &to,
            RouteOptions { via, reuse, output_name, input_name, id_generator: None },
        )?;
        let mut lines = vec![format!("Routed {from_spec} -> {to_spec} via {}:", result.plan.lowest_common_ancestor_id)];
        lines.extend(result.plan.steps.iter().map(describe_step));
        Ok(Outcome {
            document: result.document,
            message: lines.join("\n"),
            result: vec![("plan", mdg_core::js::to_value(&result.plan))],
        })
    })
}

pub fn disconnect(ctx: &mut Context<'_>, p: &Args) -> Result<()> {
    let connection_id = &p.args[1];
    run_edit(ctx, p, |doc| {
        let container_id = get_connection(doc, connection_id)?.container.id.clone();
        Ok(Outcome {
            document: remove_connection(doc, connection_id)?,
            message: format!("Removed connection {connection_id} from {container_id}"),
            result: vec![("connectionId", json!(connection_id)), ("containerId", json!(container_id))],
        })
    })
}
