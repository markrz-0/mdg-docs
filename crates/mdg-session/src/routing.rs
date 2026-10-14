//! Cross-level routing for the editor's "Route to…" dialog: the candidate
//! ports, a preview of core's plan, and committing exactly that preview as
//! one undo step.

use mdg_core::{
    get_module, get_module_path, get_port, js, route_connection, walk_descendants, Document, Endpoint, PortDirection,
    Result, RouteOptions, RouteStep, WalkOptions,
};
use serde::{Deserialize, Serialize};

use crate::session::{PendingRoute, Session};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RouteMode {
    /// From the dialog's port to an input elsewhere.
    Send,
    /// From an output elsewhere into the dialog's port.
    Receive,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Candidate {
    pub endpoint: Endpoint,
    pub label: String,
    /// Lower-case text the dialog's search matches against.
    pub search: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteChoices {
    /// "Path › Port" of the dialog's port.
    pub source_label: String,
    pub source_direction: PortDirection,
    pub candidates: Vec<Candidate>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum RoutePreview {
    #[serde(rename_all = "camelCase")]
    Ok {
        token: u64,
        /// Name of the module whose graph joins both sides.
        join_name: String,
        steps: Vec<String>,
        /// Every step reuses existing ports and connections: nothing to commit.
        already_routed: bool,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteRequest {
    pub source: Endpoint,
    pub target: Endpoint,
    pub mode: Option<RouteMode>,
    #[serde(default = "default_true")]
    pub reuse: bool,
    #[serde(default)]
    pub output_name: String,
    #[serde(default)]
    pub input_name: String,
}

fn default_true() -> bool {
    true
}

fn module_path_label(doc: &Document, module_id: &str) -> Result<String> {
    let path = get_module_path(doc, module_id)?;
    let shown = if path.len() > 1 { &path[1..] } else { &path[..] };
    Ok(shown.iter().map(|m| m.name.as_str()).collect::<Vec<_>>().join(" / "))
}

fn endpoint_label(doc: &Document, ep: &Endpoint) -> Result<String> {
    Ok(format!("{} › {}", get_module(doc, &ep.module_id)?.name, get_port(doc, &ep.module_id, &ep.port_id)?.port.name))
}

fn describe_step(doc: &Document, after: &Document, step: &RouteStep) -> Result<String> {
    Ok(match step {
        RouteStep::CreatePort { module_id, direction, port } => {
            format!("Create {direction} “{}” on {}", port.name, get_module(doc, module_id)?.name)
        }
        RouteStep::ReusePort { module_id, direction, port_id } => format!(
            "Reuse {direction} “{}” on {}",
            get_port(doc, module_id, port_id)?.port.name,
            get_module(doc, module_id)?.name
        ),
        RouteStep::CreateConnection { container_id, from, to, .. } => format!(
            "Connect {} → {} in {}",
            endpoint_label(after, from)?,
            endpoint_label(after, to)?,
            get_module(doc, container_id)?.name
        ),
        RouteStep::ReuseConnection { from, to, .. } => {
            format!("Already connected: {} → {}", endpoint_label(after, from)?, endpoint_label(after, to)?)
        }
    })
}

impl Session {
    /// Ports the dialog can route to (`Send`: every input) or from
    /// (`Receive`: every output), excluding the source port's own module.
    pub fn route_choices(&self, source: &Endpoint, mode: Option<RouteMode>) -> Result<RouteChoices> {
        let doc = &self.document;
        let source_direction = get_port(doc, &source.module_id, &source.port_id)?.direction;
        let mode = mode.unwrap_or(if source_direction == PortDirection::Output {
            RouteMode::Send
        } else {
            RouteMode::Receive
        });
        let wanted = if mode == RouteMode::Send { PortDirection::Input } else { PortDirection::Output };
        let mut candidates = Vec::new();
        for visit in walk_descendants(doc, &doc.root_id, WalkOptions::default())? {
            let module = visit.module;
            if module.id == source.module_id {
                continue;
            }
            let path = module_path_label(doc, &module.id)?;
            for port in module.ports(wanted) {
                let label = format!("{path} › {}", port.name);
                let search = format!("{label} {} {}", module.id, port.id).to_lowercase();
                candidates.push(Candidate {
                    endpoint: Endpoint::new(module.id.clone(), port.id.clone()),
                    label,
                    search,
                });
            }
        }
        let source_label = format!(
            "{} › {}",
            module_path_label(doc, &source.module_id)?,
            get_port(doc, &source.module_id, &source.port_id)?.port.name
        );
        Ok(RouteChoices { source_label, source_direction, candidates })
    }

    /// Plans a route without committing it. The plan can be committed with
    /// [`Session::commit_route`] while the document stays unchanged.
    pub fn preview_route(&mut self, request: &RouteRequest) -> RoutePreview {
        let mode = request.mode.unwrap_or(RouteMode::Send);
        let (from, to) = match mode {
            RouteMode::Send => (&request.source, &request.target),
            RouteMode::Receive => (&request.target, &request.source),
        };
        let name = |s: &str| Some(js::trim(s).to_string()).filter(|s| !s.is_empty());
        let options = RouteOptions {
            reuse: request.reuse,
            output_name: name(&request.output_name),
            input_name: name(&request.input_name),
            ..Default::default()
        };
        let base = self.document.clone();
        let planned = route_connection(&base, from, to, options).and_then(|result| {
            let steps = result
                .plan
                .steps
                .iter()
                .map(|s| describe_step(&base, &result.document, s))
                .collect::<Result<Vec<_>>>()?;
            let join_name = get_module(&base, &result.plan.lowest_common_ancestor_id)?.name.clone();
            Ok((result, steps, join_name))
        });
        match planned {
            Err(error) => {
                self.pending_route = None;
                RoutePreview::Error { message: error.message }
            }
            Ok((result, steps, join_name)) => {
                self.next_token += 1;
                let token = self.next_token;
                let already_routed = result.plan.steps.iter().all(RouteStep::is_reuse);
                self.pending_route = Some(PendingRoute { token, base, result });
                RoutePreview::Ok { token, join_name, steps, already_routed }
            }
        }
    }

    /// Commits the preview identified by `token` as one undo step. Refused
    /// when the document changed after the preview, or when the route
    /// already exists.
    pub fn commit_route(&mut self, token: u64) -> bool {
        let pending = match self.pending_route.take() {
            Some(p) if p.token == token => p,
            other => {
                self.pending_route = other;
                self.error = Some("The document changed since the route was previewed. Preview it again.".into());
                return false;
            }
        };
        if pending.result.plan.steps.iter().all(RouteStep::is_reuse) {
            return false;
        }
        self.apply_route(pending)
    }
}
