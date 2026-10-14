//! Structural message tracing across parent boundaries.
//!
//! A trace lists the routes a message *could* take through the stored
//! connections. It never implies runtime execution order, timing or
//! delivery guarantees.

use std::collections::HashMap;

use serde::Serialize;

use crate::error::Result;
use crate::model::{Connection, Document, Endpoint, PortDirection};
use crate::queries::{get_module, get_port};
use crate::validate::endpoint_key;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TraceDirection {
    Forward,
    Backward,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceHop {
    pub module_id: String,
    pub port_id: String,
    pub module_name: String,
    pub port_name: String,
    pub direction: PortDirection,
    /// Connection used to reach this hop (absent on the starting hop).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    /// Module whose graph stores that connection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TraceRouteEnd {
    /// No further connections from the last hop.
    Terminal,
    /// The next hop is already on this route; tracing stopped.
    Cycle,
    /// The maximum depth was reached.
    MaxDepth,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceRoute {
    pub hops: Vec<TraceHop>,
    pub end: TraceRouteEnd,
    /// For `cycle`: index of the hop the route loops back to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cycle_index: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
pub struct TraceOptions {
    pub direction: TraceDirection,
    /// Maximum number of connections per route. Default 64.
    pub max_depth: usize,
    /// Maximum number of routes to report. Default 1000.
    pub max_routes: usize,
}

impl Default for TraceOptions {
    fn default() -> Self {
        TraceOptions { direction: TraceDirection::Forward, max_depth: 64, max_routes: 1000 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TraceResult {
    pub start: Endpoint,
    pub direction: TraceDirection,
    pub routes: Vec<TraceRoute>,
    /// True when at least one route ends in a cycle.
    pub cyclic: bool,
    /// True when `max_routes` stopped the search early.
    pub truncated: bool,
}

struct Edge<'a> {
    connection: &'a Connection,
    container_id: &'a str,
    next: &'a Endpoint,
}

fn build_adjacency(doc: &Document, direction: TraceDirection) -> HashMap<String, Vec<Edge<'_>>> {
    let mut adjacency: HashMap<String, Vec<Edge<'_>>> = HashMap::new();
    for container in doc.modules.values() {
        for connection in &container.connections {
            let (origin, next) = match direction {
                TraceDirection::Forward => (&connection.from, &connection.to),
                TraceDirection::Backward => (&connection.to, &connection.from),
            };
            adjacency.entry(endpoint_key(&origin.module_id, &origin.port_id)).or_default().push(Edge {
                connection,
                container_id: &container.id,
                next,
            });
        }
    }
    adjacency
}

fn hop_for(doc: &Document, ep: &Endpoint, edge: Option<&Edge<'_>>) -> Result<TraceHop> {
    let module = get_module(doc, &ep.module_id)?;
    let lookup = get_port(doc, &ep.module_id, &ep.port_id)?;
    Ok(TraceHop {
        module_id: ep.module_id.clone(),
        port_id: ep.port_id.clone(),
        module_name: module.name.clone(),
        port_name: lookup.port.name.clone(),
        direction: lookup.direction,
        connection_id: edge.map(|e| e.connection.id.clone()),
        container_id: edge.map(|e| e.container_id.to_string()),
    })
}

/// Enumerates structural routes from a port, iteratively and cycle-safe.
pub fn trace(doc: &Document, module_id: &str, port_id: &str, options: TraceOptions) -> Result<TraceResult> {
    let start = Endpoint::new(module_id, port_id);
    let adjacency = build_adjacency(doc, options.direction);

    let mut routes: Vec<TraceRoute> = Vec::new();
    let mut truncated = false;
    let mut stack: Vec<(Vec<TraceHop>, Vec<String>)> =
        vec![(vec![hop_for(doc, &start, None)?], vec![endpoint_key(module_id, port_id)])];

    while let Some((hops, keys)) = stack.pop() {
        if routes.len() >= options.max_routes {
            truncated = true;
            break;
        }
        let last = hops.last().expect("routes have at least one hop");
        let edges = adjacency.get(&endpoint_key(&last.module_id, &last.port_id)).map(Vec::as_slice).unwrap_or(&[]);
        if edges.is_empty() {
            if hops.len() > 1 {
                routes.push(TraceRoute { hops, end: TraceRouteEnd::Terminal, cycle_index: None });
            }
            continue;
        }
        if hops.len() > options.max_depth {
            routes.push(TraceRoute { hops, end: TraceRouteEnd::MaxDepth, cycle_index: None });
            continue;
        }
        let mut pending = Vec::new();
        for edge in edges {
            let key = endpoint_key(&edge.next.module_id, &edge.next.port_id);
            let mut next_hops = hops.clone();
            next_hops.push(hop_for(doc, edge.next, Some(edge))?);
            if let Some(cycle_index) = keys.iter().position(|k| *k == key) {
                if routes.len() < options.max_routes {
                    routes.push(TraceRoute {
                        hops: next_hops,
                        end: TraceRouteEnd::Cycle,
                        cycle_index: Some(cycle_index),
                    });
                } else {
                    truncated = true;
                }
                continue;
            }
            let mut next_keys = keys.clone();
            next_keys.push(key);
            pending.push((next_hops, next_keys));
        }
        // Push in reverse so routes come out in stored connection order.
        stack.extend(pending.into_iter().rev());
    }

    let cyclic = routes.iter().any(|r| r.end == TraceRouteEnd::Cycle);
    Ok(TraceResult { start, direction: options.direction, routes, cyclic, truncated })
}
