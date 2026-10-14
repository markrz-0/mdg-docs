import dagre from '@dagrejs/dagre';
import type { DocumentView, Module, Position } from '../api';
import { boundaryKey } from '../state/layoutKeys';
import { getModule } from './buildGraph';

/**
 * Automatic layered layout (dagre, left to right). Purely presentational:
 * it only produces node positions, never touches the logical graph, and
 * implies nothing about execution order. The session commits the result as
 * one undo step.
 *
 * Inside a non-root module, its boundary input and output panels take part
 * as two nodes so that message flow reads from the input side to the output
 * side. Their positions are returned under `boundaryKey(moduleId, side)`.
 */

export interface Size {
  width: number;
  height: number;
}

const BOUNDARY_IN = '\u0000in';
const BOUNDARY_OUT = '\u0000out';
/** Horizontal gap between the boundary panels and the children. */
const BOUNDARY_GAP = 120;

/** Approximates a module node's rendered size (used for unmeasured levels). */
export function estimateSize(module: Module): Size {
  const longest = (ports: { name: string }[]) => ports.reduce((max, p) => Math.max(max, p.name.length), 0);
  const portsWidth = (longest(module.inputs) + longest(module.outputs)) * 6.2 + 56;
  const titleWidth = module.name.length * 7.5 + 60;
  const rows = Math.max(module.inputs.length, module.outputs.length, 1);
  return { width: Math.min(440, Math.max(220, portsWidth, titleWidth)), height: 40 + rows * 21 };
}

/** Approximates a boundary panel's rendered size. */
export function estimateBoundarySize(portCount: number): Size {
  return { width: 220, height: 32 + Math.max(portCount, 1) * 21 };
}

/**
 * Positions for the direct children of `moduleId` and, for non-root modules,
 * its two boundary panels. `sizes` may supply measured sizes (keyed like the
 * result); anything missing is estimated.
 */
export function layoutChildren(
  doc: DocumentView,
  moduleId: string,
  sizes: Record<string, Size> = {},
): Record<string, Position> {
  const module = getModule(doc, moduleId);
  if (module.childIds.length === 0) return {};

  const graph = new dagre.graphlib.Graph({ multigraph: true });
  graph.setGraph({ rankdir: 'LR', nodesep: 50, ranksep: 140, marginx: 0, marginy: 0 });
  graph.setDefaultEdgeLabel(() => ({}));

  for (const childId of module.childIds) {
    graph.setNode(childId, sizes[childId] ?? estimateSize(getModule(doc, childId)));
  }
  const hasBoundary = module.parentId !== null;
  if (hasBoundary) {
    graph.setNode(BOUNDARY_IN, sizes[boundaryKey(moduleId, 'input')] ?? estimateBoundarySize(module.inputs.length));
    graph.setNode(BOUNDARY_OUT, sizes[boundaryKey(moduleId, 'output')] ?? estimateBoundarySize(module.outputs.length));
  }
  for (const c of module.connections) {
    const source = c.from.moduleId === moduleId ? BOUNDARY_IN : c.from.moduleId;
    const target = c.to.moduleId === moduleId ? BOUNDARY_OUT : c.to.moduleId;
    if (source === BOUNDARY_IN && target === BOUNDARY_OUT) continue; // pass-through: no child involved
    if (!graph.hasNode(source) || !graph.hasNode(target)) continue;
    graph.setEdge(source, target, {}, c.id);
  }

  dagre.layout(graph);

  const topLeft = (id: string) => {
    const node = graph.node(id);
    return { x: node.x - node.width / 2, y: node.y - node.height / 2, width: node.width };
  };
  const raw = module.childIds.map((id) => ({ key: id, ...topLeft(id) }));
  if (hasBoundary) {
    // Keep dagre's vertical placement but pin the panels outside the children,
    // inputs on the left and outputs on the right, whatever their connectivity.
    const left = Math.min(...raw.map((r) => r.x));
    const right = Math.max(...raw.map((r) => r.x + r.width));
    const input = topLeft(BOUNDARY_IN);
    const output = topLeft(BOUNDARY_OUT);
    raw.push({ ...input, key: boundaryKey(moduleId, 'input'), x: left - BOUNDARY_GAP - input.width });
    raw.push({ ...output, key: boundaryKey(moduleId, 'output'), x: right + BOUNDARY_GAP });
  }
  const minX = Math.min(...raw.map((r) => r.x));
  const minY = Math.min(...raw.map((r) => r.y));
  return Object.fromEntries(
    raw.map((r) => [r.key, { x: Math.round(r.x - minX), y: Math.round(r.y - minY) }] as const),
  );
}

/** Pre-order walk of the module tree (iterative). */
function walk(doc: DocumentView): Module[] {
  const result: Module[] = [];
  const stack = [doc.rootId];
  while (stack.length > 0) {
    const module = getModule(doc, stack.pop()!);
    result.push(module);
    for (let i = module.childIds.length - 1; i >= 0; i -= 1) stack.push(module.childIds[i]!);
  }
  return result;
}

/** Positions for every level of the hierarchy. */
export function layoutAll(doc: DocumentView, sizes: Record<string, Size> = {}): Record<string, Position> {
  const positions: Record<string, Position> = {};
  for (const module of walk(doc)) {
    if (module.childIds.length > 0) Object.assign(positions, layoutChildren(doc, module.id, sizes));
  }
  return positions;
}
