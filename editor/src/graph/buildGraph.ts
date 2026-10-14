import type { Edge, Node } from '@xyflow/react';
import type { DocumentView, Endpoint, Module, Port, Position, Selection } from '../api';
import { boundaryKey, type BoundarySide } from '../state/layoutKeys';

/**
 * Pure projection of (document, current module, positions) to React Flow
 * nodes and edges. Never mutates the document.
 *
 * Inside a non-root module, its own ports are shown on two boundary nodes:
 * inputs on the left (graph sources), outputs on the right (graph
 * destinations). The parent itself is never rendered as a child node.
 */

/** Boundary node IDs cannot collide with module IDs (those start alphanumeric). */
export const BOUNDARY_IN = '__boundary_in__';
export const BOUNDARY_OUT = '__boundary_out__';

/** Horizontal room reserved for a node when placing boundary nodes. */
export const NODE_WIDTH = 300;

export interface PortView {
  id: string;
  name: string;
}

export type ModuleNodeData = {
  moduleId: string;
  name: string;
  inputs: PortView[];
  outputs: PortView[];
  childCount: number;
  selectedPortId: string | null;
};

export type BoundaryNodeData = {
  side: 'in' | 'out';
  moduleId: string;
  moduleName: string;
  ports: PortView[];
  selectedPortId: string | null;
};

export type ModuleFlowNode = Node<ModuleNodeData, 'module'>;
export type BoundaryFlowNode = Node<BoundaryNodeData, 'boundary'>;
export type FlowNode = ModuleFlowNode | BoundaryFlowNode;

export interface Graph {
  nodes: FlowNode[];
  edges: Edge[];
}

const portView = (p: Port): PortView => ({ id: p.id, name: p.name });

export function getModule(doc: DocumentView, moduleId: string): Module {
  const module = Object.hasOwn(doc.modules, moduleId) ? doc.modules[moduleId] : undefined;
  if (!module) throw new Error(`Module "${moduleId}" does not exist.`);
  return module;
}

export function defaultPosition(index: number): Position {
  return { x: (index % 3) * 380, y: Math.floor(index / 3) * 220 };
}

/** Boundary side shown by a boundary node, or null for module nodes. */
export function boundarySideOf(nodeId: string): BoundarySide | null {
  return nodeId === BOUNDARY_IN ? 'input' : nodeId === BOUNDARY_OUT ? 'output' : null;
}

/** Maps a React Flow node ID back to the module it represents. */
export function nodeToModuleId(nodeId: string, currentModuleId: string): string {
  return nodeId === BOUNDARY_IN || nodeId === BOUNDARY_OUT ? currentModuleId : nodeId;
}

/** Maps an endpoint inside the current module's graph to a React Flow node ID. */
export function endpointNodeId(ep: Endpoint, currentModuleId: string, role: 'source' | 'target'): string {
  if (ep.moduleId !== currentModuleId) return ep.moduleId;
  return role === 'source' ? BOUNDARY_IN : BOUNDARY_OUT;
}

export function buildGraph(
  doc: DocumentView,
  currentModuleId: string,
  positions: Record<string, Position>,
  selection: Selection,
): Graph {
  const current = getModule(doc, currentModuleId);
  const selectedPort = (moduleId: string) =>
    selection?.kind === 'port' && selection.moduleId === moduleId ? selection.portId : null;

  const nodes: FlowNode[] = current.childIds.map((childId, index) => {
    const child = getModule(doc, childId);
    return {
      id: child.id,
      type: 'module',
      position: positions[child.id] ?? defaultPosition(index),
      data: {
        moduleId: child.id,
        name: child.name,
        inputs: child.inputs.map(portView),
        outputs: child.outputs.map(portView),
        childCount: child.childIds.length,
        selectedPortId: selectedPort(child.id),
      },
      selected: selection?.kind === 'module' && selection.moduleId === child.id,
    };
  });

  if (current.parentId !== null) {
    const xs = nodes.map((n) => n.position.x);
    const ys = nodes.map((n) => n.position.y);
    const minX = xs.length ? Math.min(...xs) : 0;
    const maxX = xs.length ? Math.max(...xs) : 200;
    const minY = ys.length ? Math.min(...ys) : 0;
    // Stored position if the user (or auto-layout) placed the panel; else beside the children.
    const boundary = (side: 'in' | 'out'): BoundaryFlowNode => ({
      id: side === 'in' ? BOUNDARY_IN : BOUNDARY_OUT,
      type: 'boundary',
      position: positions[boundaryKey(current.id, side === 'in' ? 'input' : 'output')] ?? {
        x: side === 'in' ? minX - NODE_WIDTH - 100 : maxX + NODE_WIDTH + 100,
        y: minY,
      },
      draggable: true,
      selectable: false,
      deletable: false,
      data: {
        side,
        moduleId: current.id,
        moduleName: current.name,
        ports: (side === 'in' ? current.inputs : current.outputs).map(portView),
        selectedPortId: selectedPort(current.id),
      },
    });
    nodes.unshift(boundary('in'));
    nodes.push(boundary('out'));
  }

  // A selected port lights up every connection from or to it.
  const port = selection?.kind === 'port' ? selection : null;
  const touchesPort = (ep: Endpoint) => port !== null && ep.moduleId === port.moduleId && ep.portId === port.portId;

  const edges: Edge[] = current.connections.map((c) => {
    const highlighted = touchesPort(c.from) || touchesPort(c.to);
    return {
      id: c.id,
      source: endpointNodeId(c.from, current.id, 'source'),
      sourceHandle: c.from.portId,
      target: endpointNodeId(c.to, current.id, 'target'),
      targetHandle: c.to.portId,
      selected: selection?.kind === 'connection' && selection.connectionId === c.id,
      deletable: false,
      markerEnd: { type: 'arrowclosed' as const, ...(highlighted ? { color: 'var(--accent)' } : {}) },
      ...(highlighted ? { className: 'highlighted' } : {}),
    };
  });

  return { nodes, edges };
}
