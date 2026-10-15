import {
  Background,
  Controls,
  MiniMap,
  Panel,
  ReactFlow,
  useNodesState,
  useReactFlow,
  type Connection,
  type Edge,
  type IsValidConnection,
  type Node,
  type NodeMouseHandler,
  type OnConnectStart,
  type OnNodeDrag,
} from '@xyflow/react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { commands, queries, type Endpoint, type PortKind } from '../api';
import { boundaryKey } from '../state/layoutKeys';
import { usePreferences, type NodeColoring } from '../state/preferences';
import { getView, run, useEditor } from '../state/store';
import { layoutAll, layoutChildren, type Size } from './autoLayout';
import {
  boundarySideOf,
  buildGraph,
  endpointKind,
  endpointNodeId,
  kindColor,
  nodeToModuleId,
  type FlowNode,
} from './buildGraph';
import { nodeTypes } from './nodes';

function toEndpoints(connection: Connection | Edge, currentModuleId: string): { from: Endpoint; to: Endpoint } | null {
  if (!connection.sourceHandle || !connection.targetHandle) return null;
  return {
    from: { moduleId: nodeToModuleId(connection.source, currentModuleId), portId: connection.sourceHandle },
    to: { moduleId: nodeToModuleId(connection.target, currentModuleId), portId: connection.targetHandle },
  };
}

const handleKey = (nodeId: string, handleId: string) => `${nodeId}\u0000${handleId}`;

/**
 * Interactive graph of the current module. Remounted per module so React
 * Flow's node state and fit-to-view start fresh at every navigation level.
 */
export function Canvas() {
  const currentModuleId = useEditor((s) => s.view.currentModuleId);
  return <ModuleCanvas key={currentModuleId} currentModuleId={currentModuleId} />;
}

/**
 * React Flow events are translated into session commands, which apply core
 * operations in Rust; the model is never changed from here.
 */
function ModuleCanvas({ currentModuleId }: { currentModuleId: string }) {
  const document = useEditor((s) => s.view.document);
  const positions = useEditor((s) => s.view.positions);
  const selection = useEditor((s) => s.view.selection);

  const graph = useMemo(
    () => buildGraph(document, currentModuleId, positions, selection),
    [document, currentModuleId, positions, selection],
  );

  // Local node state lets React Flow track measurements and in-progress drags.
  const [nodes, setNodes, onNodesChange] = useNodesState<FlowNode>(graph.nodes);
  useEffect(() => {
    setNodes((previous) => {
      const byId = new Map(previous.map((n) => [n.id, n]));
      return graph.nodes.map((n) => {
        const old = byId.get(n.id);
        return old ? ({ ...old, ...n, measured: old.measured } as FlowNode) : n;
      });
    });
  }, [graph.nodes, setNodes]);

  // While dragging a new connection, the handles it may end at. The session
  // computes them with core's rules, so the canvas refuses exactly what an
  // edit would reject. Until the answer arrives (a very fast drag), drops are
  // allowed: `connect` validates again and reports a rejection as an error.
  const partners = useRef<{ from: 'source' | 'target'; keys: Set<string> } | null>(null);
  // The line being dragged takes the colour of the port it starts from.
  const [dragKind, setDragKind] = useState<PortKind>('event');

  const onConnectStart: OnConnectStart = useCallback(
    (_event, { nodeId, handleId, handleType }) => {
      partners.current = null;
      if (!nodeId || !handleId || !handleType) return;
      const endpoint = { moduleId: nodeToModuleId(nodeId, currentModuleId), portId: handleId };
      setDragKind(endpointKind(getView().document, endpoint));
      const otherRole = handleType === 'source' ? 'target' : 'source';
      void queries.connectionPartners(endpoint, handleType).then((list) => {
        const keys = new Set(list.map((ep) => handleKey(endpointNodeId(ep, currentModuleId, otherRole), ep.portId)));
        partners.current = { from: handleType, keys };
      });
    },
    [currentModuleId],
  );

  const isValidConnection: IsValidConnection = useCallback((connection) => {
    const allowed = partners.current;
    if (!connection.sourceHandle || !connection.targetHandle) return false;
    if (!allowed) return true;
    const key =
      allowed.from === 'source'
        ? handleKey(connection.target, connection.targetHandle)
        : handleKey(connection.source, connection.sourceHandle);
    return allowed.keys.has(key);
  }, []);

  const onConnect = useCallback(
    (connection: Connection) => {
      const endpoints = toEndpoints(connection, currentModuleId);
      if (endpoints) void run(commands.connect(endpoints.from, endpoints.to));
    },
    [currentModuleId],
  );

  const onNodeDragStop: OnNodeDrag<Node> = useCallback(
    (_event, _node, dragged) => {
      const changes = dragged.map((n) => {
        const side = boundarySideOf(n.id);
        const key = side ? boundaryKey(currentModuleId, side) : n.id;
        return { key, x: Math.round(n.position.x), y: Math.round(n.position.y) };
      });
      void run(commands.setPositions(changes));
    },
    [currentModuleId],
  );

  const onNodeClick: NodeMouseHandler<Node> = useCallback((_event, node) => {
    if (node.type === 'module') void run(commands.select({ kind: 'module', moduleId: node.id }));
  }, []);

  const onNodeDoubleClick: NodeMouseHandler<Node> = useCallback((_event, node) => {
    if (node.type === 'module') void run(commands.navigate(node.id));
  }, []);

  const onEdgeClick = useCallback((_event: React.MouseEvent, edge: Edge) => {
    void run(commands.select({ kind: 'connection', connectionId: edge.id }));
  }, []);

  const onPaneClick = useCallback(() => void run(commands.select(null)), []);

  return (
    <div className="canvas" data-testid="canvas">
      <ReactFlow
        nodes={nodes}
        edges={graph.edges}
        nodeTypes={nodeTypes}
        onNodesChange={onNodesChange}
        onNodeDragStop={onNodeDragStop}
        onConnectStart={onConnectStart}
        connectionLineStyle={{ stroke: kindColor(dragKind) }}
        onConnect={onConnect}
        isValidConnection={isValidConnection}
        onNodeClick={onNodeClick}
        onNodeDoubleClick={onNodeDoubleClick}
        onEdgeClick={onEdgeClick}
        onPaneClick={onPaneClick}
        zoomOnDoubleClick={false}
        deleteKeyCode={null}
        fitView
        fitViewOptions={{ padding: 0.2, maxZoom: 1 }}
        minZoom={0.1}
        proOptions={{ hideAttribution: true }}
        colorMode="system"
      >
        <Background gap={20} />
        <Controls showInteractive={false} />
        <MiniMap pannable zoomable />
        <ColoringPanel />
        <LayoutPanel />
      </ReactFlow>
    </div>
  );
}

/** How module nodes are coloured: one solid colour, or by their tags. */
function ColoringPanel() {
  const coloring = usePreferences((s) => s.nodeColoring);
  const setColoring = usePreferences((s) => s.setNodeColoring);
  return (
    <Panel position="top-left" className="coloring-panel">
      <label>
        <span className="coloring-label">Module colour</span>
        <select
          value={coloring}
          onChange={(e) => setColoring(e.target.value as NodeColoring)}
          data-testid="node-coloring"
        >
          <option value="solid">Solid colour</option>
          <option value="tags">Tag colour</option>
        </select>
      </label>
    </Panel>
  );
}

/**
 * Auto-layout controls. Lives inside React Flow so it can pass measured node
 * sizes for the open level, and re-fits the view after each layout.
 */
function LayoutPanel() {
  const flow = useReactFlow();
  const layoutVersion = useEditor((s) => s.view.layoutVersion);
  // Compare versions rather than tracking "first render": StrictMode runs
  // effects twice on mount, which would otherwise trigger a spurious fit.
  const seenVersion = useRef(layoutVersion);

  useEffect(() => {
    if (seenVersion.current === layoutVersion) return;
    seenVersion.current = layoutVersion;
    const frame = requestAnimationFrame(() => void flow.fitView({ padding: 0.2, maxZoom: 1, duration: 200 }));
    return () => cancelAnimationFrame(frame);
  }, [layoutVersion, flow]);

  const layout = (scope: 'current' | 'all') => {
    const { document, currentModuleId } = getView();
    const measured: Record<string, Size> = {};
    for (const node of flow.getNodes()) {
      if (!node.measured?.width || !node.measured.height) continue;
      const side = boundarySideOf(node.id);
      const key = side ? boundaryKey(currentModuleId, side) : node.id;
      measured[key] = { width: node.measured.width, height: node.measured.height };
    }
    try {
      const computed =
        scope === 'all' ? layoutAll(document, measured) : layoutChildren(document, currentModuleId, measured);
      const positions = Object.entries(computed).map(([key, p]) => ({ key, x: p.x, y: p.y }));
      if (positions.length > 0) void run(commands.applyLayout(positions));
    } catch (error) {
      void run(commands.setError(error instanceof Error ? error.message : String(error)));
    }
  };

  return (
    <Panel position="top-right" className="layout-panel">
      <button onClick={() => layout('current')} data-testid="auto-layout" title="Arrange this level left to right">
        Auto-layout
      </button>
      <button onClick={() => layout('all')} data-testid="auto-layout-all" title="Arrange every level of the hierarchy">
        All levels
      </button>
    </Panel>
  );
}
