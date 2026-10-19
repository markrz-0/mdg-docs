//! Automatic boundary routing: connects two ports at arbitrary hierarchy
//! depths by creating boundary ports and *local* connections at every level.
//! The stored result never contains cross-level connections.

use serde::Serialize;

use super::{add_connection, add_port, assert_valid, AddConnectionOptions, AddPortOptions};
use crate::error::{MdgError, Result};
use crate::ids::{IdGenerator, UuidIdGenerator};
use crate::jsmap::JsMap;
use crate::model::{Document, Endpoint, Port, PortDirection, PortKind};
use crate::queries::{get_ancestors, get_module, get_port, lowest_common_ancestor, same_endpoint};

pub struct RouteOptions<'a> {
    /// Explicitly chosen boundary ports, keyed by intermediate module ID. When
    /// present, that port is used at that level instead of creating one.
    pub via: JsMap<String>,
    /// Reuse an existing boundary port when reuse is unambiguous: exactly one
    /// existing connection already carries the current source to a boundary
    /// port of the required module. Default true. Names are never used to match.
    pub reuse: bool,
    /// Name for generated output ports (source side). Defaults to the source port's name.
    pub output_name: Option<String>,
    /// Name for generated input ports (destination side). Defaults to the destination port's name.
    pub input_name: Option<String>,
    pub id_generator: Option<&'a mut dyn IdGenerator>,
}

impl Default for RouteOptions<'_> {
    fn default() -> Self {
        RouteOptions { via: JsMap::new(), reuse: true, output_name: None, input_name: None, id_generator: None }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RouteStep {
    #[serde(rename_all = "camelCase")]
    CreatePort { module_id: String, direction: PortDirection, port: Port },
    #[serde(rename_all = "camelCase")]
    ReusePort { module_id: String, direction: PortDirection, port_id: String },
    #[serde(rename_all = "camelCase")]
    CreateConnection { container_id: String, connection_id: String, from: Endpoint, to: Endpoint },
    #[serde(rename_all = "camelCase")]
    ReuseConnection { container_id: String, connection_id: String, from: Endpoint, to: Endpoint },
}

impl RouteStep {
    /// True for steps that change nothing.
    pub fn is_reuse(&self) -> bool {
        matches!(self, RouteStep::ReusePort { .. } | RouteStep::ReuseConnection { .. })
    }

    /// The connection a connection step creates or reuses.
    pub fn connection_id(&self) -> Option<&str> {
        match self {
            RouteStep::CreateConnection { connection_id, .. } | RouteStep::ReuseConnection { connection_id, .. } => {
                Some(connection_id)
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutePlan {
    pub from: Endpoint,
    pub to: Endpoint,
    /// Module whose graph joins the source side to the destination side.
    pub lowest_common_ancestor_id: String,
    pub steps: Vec<RouteStep>,
}

#[derive(Debug, Clone)]
pub struct RouteResult {
    /// The resulting document. For a dry run, simply do not commit it.
    pub document: Document,
    pub plan: RoutePlan,
}

/// Module whose internal graph can use `ep` as a connection source.
fn source_context(doc: &Document, ep: &Endpoint) -> Result<String> {
    if get_port(doc, &ep.module_id, &ep.port_id)?.direction == PortDirection::Input {
        return Ok(ep.module_id.clone());
    }
    get_module(doc, &ep.module_id)?
        .parent_id
        .clone()
        .ok_or_else(|| MdgError::operation(format!("\"{}\" has no parent to route through.", ep.module_id)))
}

/// Module whose internal graph can use `ep` as a connection destination.
fn destination_context(doc: &Document, ep: &Endpoint) -> Result<String> {
    if get_port(doc, &ep.module_id, &ep.port_id)?.direction == PortDirection::Output {
        return Ok(ep.module_id.clone());
    }
    get_module(doc, &ep.module_id)?
        .parent_id
        .clone()
        .ok_or_else(|| MdgError::operation(format!("\"{}\" has no parent to route through.", ep.module_id)))
}

/// Module chain from `from_id` (inclusive) up to `ancestor_id` (exclusive).
fn chain_up_to(doc: &Document, from_id: &str, ancestor_id: &str) -> Result<Vec<String>> {
    if from_id == ancestor_id {
        return Ok(vec![]);
    }
    let mut chain = vec![from_id.to_string()];
    for module in get_ancestors(doc, from_id)? {
        if module.id == ancestor_id {
            return Ok(chain);
        }
        chain.push(module.id.clone());
    }
    Err(MdgError::operation(format!("\"{ancestor_id}\" is not an ancestor of \"{from_id}\".")))
}

/// Contracts of a generated boundary port.
struct Contracts {
    description: String,
    /// Request routes only.
    response: Option<String>,
}

struct Router<'a, 'g> {
    document: Document,
    steps: Vec<RouteStep>,
    current: Endpoint,
    /// Kind of the routed ports; every boundary port on the route has it.
    kind: PortKind,
    via: &'a JsMap<String>,
    reuse: bool,
    ids: &'a mut (dyn IdGenerator + 'g),
}

impl Router<'_, '_> {
    fn connect(&mut self, container_id: &str, target: Endpoint) -> Result<()> {
        let container = get_module(&self.document, container_id)?;
        if let Some(existing) = container
            .connections
            .iter()
            .find(|c| same_endpoint(&c.from, &self.current) && same_endpoint(&c.to, &target))
        {
            self.steps.push(RouteStep::ReuseConnection {
                container_id: container_id.to_string(),
                connection_id: existing.id.clone(),
                from: self.current.clone(),
                to: target,
            });
            return Ok(());
        }
        let result = add_connection(
            &self.document,
            AddConnectionOptions {
                from: self.current.clone(),
                to: target.clone(),
                container_id: Some(container_id.to_string()),
                id: None,
                id_generator: Some(&mut *self.ids),
            },
        )?;
        self.document = result.document;
        self.steps.push(RouteStep::CreateConnection {
            container_id: container_id.to_string(),
            connection_id: result.id,
            from: self.current.clone(),
            to: target,
        });
        Ok(())
    }

    /// Picks or creates the boundary port on `module_id` that continues the route.
    fn boundary_port(
        &mut self,
        module_id: &str,
        container_id: &str,
        direction: PortDirection,
        name: &str,
        contracts: Contracts,
    ) -> Result<String> {
        if let Some(explicit) = self.via.get(module_id) {
            let lookup = get_port(&self.document, module_id, explicit)?;
            if lookup.direction != direction {
                return Err(MdgError::argument(format!(
                    "\"via\" port \"{module_id}:{explicit}\" is an {}; the route needs an {direction} here.",
                    lookup.direction
                )));
            }
            if lookup.port.kind != self.kind {
                return Err(MdgError::argument(format!(
                    "\"via\" port \"{module_id}:{explicit}\" is a {} port; the route needs a {} port here.",
                    lookup.port.kind, self.kind
                )));
            }
            self.steps.push(RouteStep::ReusePort {
                module_id: module_id.to_string(),
                direction,
                port_id: explicit.clone(),
            });
            return Ok(explicit.clone());
        }
        if self.reuse {
            let container = get_module(&self.document, container_id)?;
            let ports = get_module(&self.document, module_id)?.ports(direction);
            let candidates: Vec<&str> = container
                .connections
                .iter()
                .filter(|c| {
                    same_endpoint(&c.from, &self.current)
                        && c.to.module_id == module_id
                        && ports.iter().any(|p| p.id == c.to.port_id)
                })
                .map(|c| c.to.port_id.as_str())
                .collect();
            if let [port_id] = candidates[..] {
                let port_id = port_id.to_string();
                self.steps.push(RouteStep::ReusePort {
                    module_id: module_id.to_string(),
                    direction,
                    port_id: port_id.clone(),
                });
                return Ok(port_id);
            }
        }
        let result = add_port(
            &self.document,
            module_id,
            direction,
            AddPortOptions {
                name: name.to_string(),
                kind: self.kind,
                description: Some(contracts.description),
                response: contracts.response,
                id: None,
                index: None,
                id_generator: Some(&mut *self.ids),
            },
        )?;
        self.document = result.document;
        let port = get_port(&self.document, module_id, &result.id)?.port.clone();
        self.steps.push(RouteStep::CreatePort { module_id: module_id.to_string(), direction, port });
        Ok(result.id)
    }
}

/// Plans and applies a cross-level route from `from` to `to`.
///
/// 1. Find the lowest common ancestor of the source and destination contexts.
/// 2. Walk up from the source, adding an output boundary port per level.
/// 3. Walk down to the destination, adding an input boundary port per level.
/// 4. Connect each level locally, validate, and return atomically.
///
/// The input document is never modified; on any failure an error is returned.
pub fn route_connection(
    doc: &Document,
    from: &Endpoint,
    to: &Endpoint,
    options: RouteOptions<'_>,
) -> Result<RouteResult> {
    let mut uuid = UuidIdGenerator;
    let ids: &mut dyn IdGenerator = match options.id_generator {
        Some(g) => g,
        None => &mut uuid,
    };
    let source_port = get_port(doc, &from.module_id, &from.port_id)?.port;
    let destination_port = get_port(doc, &to.module_id, &to.port_id)?.port;
    if source_port.kind != destination_port.kind {
        return Err(MdgError::operation(format!(
            "Cannot route a {} port (\"{from}\") to a {} port (\"{to}\"). Both ends must be the same kind.",
            source_port.kind, destination_port.kind
        )));
    }
    let kind = source_port.kind;
    let source_module = get_module(doc, &from.module_id)?;
    let destination_module = get_module(doc, &to.module_id)?;

    let source_ctx = source_context(doc, from)?;
    let destination_ctx = destination_context(doc, to)?;
    let lca = lowest_common_ancestor(doc, &source_ctx, &destination_ctx)?;
    let up_chain = chain_up_to(doc, &source_ctx, &lca)?;
    let mut down_chain = chain_up_to(doc, &destination_ctx, &lca)?;
    down_chain.reverse();

    for module_id in options.via.keys() {
        if !up_chain.iter().any(|m| m == module_id) && !down_chain.iter().any(|m| m == module_id) {
            return Err(MdgError::argument(format!(
                "\"via\" names module \"{module_id}\", which is not on the route."
            )));
        }
    }

    let source_label = format!("\"{}\" from {} (`{from}`)", source_port.name, source_module.name);
    let destination_label = format!("{} \"{}\" (`{to}`)", destination_module.name, destination_port.name);
    let output_name = options.output_name.clone().unwrap_or_else(|| source_port.name.clone());
    let input_name = options.input_name.clone().unwrap_or_else(|| destination_port.name.clone());

    let mut router = Router {
        document: doc.clone(),
        steps: Vec::new(),
        current: from.clone(),
        kind,
        via: &options.via,
        reuse: options.reuse,
        ids,
    };

    // A request route also carries the response back along the same ports.
    let response = (kind == PortKind::Request).then(|| {
        format!("Returns the response of {destination_label} to {source_label}. Generated by boundary routing.")
    });

    // Source side: climb from the source context up to (excluding) the LCA.
    for module_id in &up_chain {
        let contracts = Contracts {
            description: format!(
                "Forwards {source_label} out of this module towards {destination_label}. Generated by boundary routing."
            ),
            response: response.clone(),
        };
        let port_id = router.boundary_port(module_id, module_id, PortDirection::Output, &output_name, contracts)?;
        let target = Endpoint::new(module_id.clone(), port_id);
        router.connect(module_id, target.clone())?;
        router.current = target;
    }

    // Destination side: descend from the LCA into the destination context.
    let mut container_id = lca.clone();
    for module_id in &down_chain {
        let contracts = Contracts {
            description: format!(
                "Receives {source_label} and forwards it to {destination_label}. Generated by boundary routing."
            ),
            response: response.clone(),
        };
        let port_id = router.boundary_port(module_id, &container_id, PortDirection::Input, &input_name, contracts)?;
        let target = Endpoint::new(module_id.clone(), port_id);
        router.connect(&container_id, target.clone())?;
        router.current = target;
        container_id = module_id.clone();
    }

    router.connect(&container_id, to.clone())?;
    assert_valid(&router.document)?;

    Ok(RouteResult {
        document: router.document,
        plan: RoutePlan { from: from.clone(), to: to.clone(), lowest_common_ancestor_id: lca, steps: router.steps },
    })
}
