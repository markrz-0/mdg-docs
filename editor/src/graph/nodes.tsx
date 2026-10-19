import { Handle, Position, type NodeProps } from '@xyflow/react';
import type { CSSProperties, MouseEvent } from 'react';
import { commands } from '../api';
import { TagChip } from '../components/TagChip';
import { usePreferences } from '../state/preferences';
import { run } from '../state/store';
import type { BoundaryFlowNode, ModuleFlowNode, PortView } from './buildGraph';

/** Handle colour follows the port kind: blue for events, green for requests. */
const handleClass = (port: PortView) => (port.kind === 'request' ? 'mdg-handle-request' : undefined);

function PortLabel({ moduleId, port, selected }: { moduleId: string; port: PortView; selected: boolean }) {
  const onClick = (event: MouseEvent) => {
    event.stopPropagation();
    void run(commands.select({ kind: 'port', moduleId, portId: port.id }));
  };
  const request = port.kind === 'request';
  return (
    <span
      className={`port-label${request ? ' request' : ''}${selected ? ' selected' : ''}`}
      onClick={onClick}
      title={`${port.name} (${port.id}) · ${request ? 'request/response' : 'event'}`}
      data-testid={`port-${moduleId}-${port.id}`}
    >
      <span className="port-kind" aria-label={request ? 'request/response' : 'event'}>
        {request ? '⇄' : '→'}
      </span>
      {port.name}
    </span>
  );
}

/** A child module: name, inputs (left handles) and outputs (right handles). */
export function ModuleNode({ data, selected }: NodeProps<ModuleFlowNode>) {
  const coloring = usePreferences((s) => s.nodeColoring);
  const color = coloring === 'tags' ? data.color : null;
  return (
    <div
      className={`mdg-node${color ? ' tagged' : ''}${selected ? ' selected' : ''}`}
      style={color ? ({ '--node-tag': color } as CSSProperties) : undefined}
      data-testid={`node-${data.moduleId}`}
    >
      <div className="mdg-node-title">
        <span className="mdg-node-name">{data.name}</span>
        {data.childCount > 0 && (
          <span className="mdg-node-badge" title="Submodules — double-click to open">
            {data.childCount}
          </span>
        )}
      </div>
      {data.tags.length > 0 && (
        <div className="mdg-node-tags">
          {data.tags.map((tag) => (
            <TagChip key={tag.id} tag={tag} />
          ))}
        </div>
      )}
      <div className="mdg-node-ports">
        <div className="mdg-port-column">
          {data.inputs.map((port) => (
            <div key={port.id} className="mdg-port mdg-port-in">
              <Handle type="target" position={Position.Left} id={port.id} className={handleClass(port)} />
              <PortLabel moduleId={data.moduleId} port={port} selected={data.selectedPortId === port.id} />
            </div>
          ))}
        </div>
        <div className="mdg-port-column right">
          {data.outputs.map((port) => (
            <div key={port.id} className="mdg-port mdg-port-out">
              <PortLabel moduleId={data.moduleId} port={port} selected={data.selectedPortId === port.id} />
              <Handle type="source" position={Position.Right} id={port.id} className={handleClass(port)} />
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

/**
 * The current module's own boundary. Its inputs act as sources inside the
 * graph (handles on the right); its outputs act as destinations (handles on
 * the left).
 */
export function BoundaryNode({ data }: NodeProps<BoundaryFlowNode>) {
  const isIn = data.side === 'in';
  return (
    <div className={`mdg-boundary mdg-boundary-${data.side}`} data-testid={`boundary-${data.side}`}>
      <div className="mdg-boundary-title">{isIn ? 'Inputs' : 'Outputs'} of {data.moduleName}</div>
      {data.ports.length === 0 && <div className="mdg-boundary-empty">none</div>}
      {data.ports.map((port) => (
        <div key={port.id} className={`mdg-port ${isIn ? 'mdg-port-out' : 'mdg-port-in'}`}>
          {!isIn && <Handle type="target" position={Position.Left} id={port.id} className={handleClass(port)} />}
          <PortLabel moduleId={data.moduleId} port={port} selected={data.selectedPortId === port.id} />
          {isIn && <Handle type="source" position={Position.Right} id={port.id} className={handleClass(port)} />}
        </div>
      ))}
    </div>
  );
}

export const nodeTypes = { module: ModuleNode, boundary: BoundaryNode };
