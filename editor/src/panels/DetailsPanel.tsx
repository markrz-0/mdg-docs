import { useState } from 'react';
import { commands, portKind, type Module, type Port, type PortDirection, type PortKind } from '../api';
import { CommitInput } from '../components/CommitInput';
import { run, useEditor } from '../state/store';
import { MarkdownField } from './MarkdownField';
import { RouteDialog } from './RouteDialog';

function findPort(module: Module, portId: string): { port: Port; direction: PortDirection } | null {
  const input = module.inputs.find((p) => p.id === portId);
  if (input) return { port: input, direction: 'input' };
  const output = module.outputs.find((p) => p.id === portId);
  return output ? { port: output, direction: 'output' } : null;
}

/** What a request port means on this side of the exchange. */
const requestRole = (direction: PortDirection) => (direction === 'input' ? 'handles requests' : 'sends requests');

function PortList({ module, direction }: { module: Module; direction: PortDirection }) {
  const ports = direction === 'input' ? module.inputs : module.outputs;
  const add = (kind: PortKind) => void run(commands.addPort(module.id, direction, kind));
  return (
    <section className="field">
      <div className="field-header">
        <span className="field-label">{direction === 'input' ? 'Inputs' : 'Outputs'}</span>
        <span className="actions-inline">
          <button
            className="small"
            onClick={() => add('event')}
            data-testid={`add-${direction}`}
            title={direction === 'input' ? 'Listens for an event' : 'Emits an event'}
          >
            + Event
          </button>
          <button
            className="small request"
            onClick={() => add('request')}
            data-testid={`add-${direction}-request`}
            title={direction === 'input' ? 'Handles a request and returns a response' : 'Sends a request and receives a response'}
          >
            + Request
          </button>
        </span>
      </div>
      {ports.length === 0 && <p className="muted">None.</p>}
      <ul className="port-list">
        {ports.map((p) => (
          <li key={p.id}>
            <button
              className="link"
              onClick={() => void run(commands.select({ kind: 'port', moduleId: module.id, portId: p.id }))}
            >
              {p.name}
            </button>{' '}
            <code>{p.id}</code>
            {portKind(p) === 'request' ? (
              <span className="kind-badge">⇄ {requestRole(direction)}</span>
            ) : (
              <span className="kind-badge event">→ event</span>
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}

function ModuleDetails({ module }: { module: Module }) {
  const title = useEditor((s) => s.view.document.title);
  const currentModuleId = useEditor((s) => s.view.currentModuleId);
  const [cascade, setCascade] = useState(false);
  const isRoot = module.parentId === null;

  return (
    <div data-testid="module-details">
      <h2 className="panel-title">{isRoot ? 'Root module' : 'Module'}</h2>
      {isRoot && (
        <label className="field">
          <span className="field-label">Document title</span>
          <CommitInput label="Document title" value={title} onCommit={(t) => void run(commands.setTitle(t))} />
        </label>
      )}
      <label className="field">
        <span className="field-label">Name</span>
        <CommitInput
          label="Module name"
          value={module.name}
          onCommit={(name) => void run(commands.renameModule(module.id, name))}
          testId="module-name"
        />
      </label>
      <div className="field">
        <span className="field-label">ID</span>
        <code data-testid="module-id">{module.id}</code>
      </div>
      <div className="field">
        <span className="field-label">Description</span>
        <MarkdownField
          value={module.description}
          onChange={(text) => void run(commands.setModuleDescription(module.id, text))}
          testId="module-description"
        />
      </div>
      {!isRoot && <PortList module={module} direction="input" />}
      {!isRoot && <PortList module={module} direction="output" />}
      <div className="actions">
        {module.id !== currentModuleId && (
          <button onClick={() => void run(commands.navigate(module.id))} data-testid="open-module">
            Open internal graph
          </button>
        )}
        {!isRoot && (
          <>
            <button
              className="danger"
              onClick={() => void run(commands.deleteModule(module.id, cascade))}
              data-testid="delete-module"
            >
              Delete module
            </button>
            <label className="checkbox">
              <input type="checkbox" checked={cascade} onChange={(e) => setCascade(e.target.checked)} />
              include submodules &amp; connections
            </label>
          </>
        )}
      </div>
    </div>
  );
}

function PortDetails({ module, port, direction }: { module: Module; port: Port; direction: PortDirection }) {
  const connections = useEditor((s) => s.view.details.portConnections ?? 0);
  const [cascade, setCascade] = useState(false);
  const [routing, setRouting] = useState(false);
  const kind = portKind(port);
  const title =
    kind === 'request'
      ? direction === 'input'
        ? 'Request port (handled here)'
        : 'Request port (sent from here)'
      : direction === 'input'
        ? 'Input port'
        : 'Output port';
  const setKind = (next: PortKind) => {
    if (next !== kind) void run(commands.setPortKind(module.id, port.id, next));
  };
  return (
    <div data-testid="port-details">
      <h2 className="panel-title">{title}</h2>
      <div className="field">
        <span className="field-label">Module</span>
        <button className="link" onClick={() => void run(commands.select({ kind: 'module', moduleId: module.id }))}>
          {module.name}
        </button>
      </div>
      <label className="field">
        <span className="field-label">Name</span>
        <CommitInput
          label="Port name"
          value={port.name}
          onCommit={(name) => void run(commands.renamePort(module.id, port.id, name))}
          testId="port-name"
        />
      </label>
      <div className="field">
        <span className="field-label">ID</span>
        <code>{port.id}</code>
      </div>
      <div className="field">
        <span className="field-label">Kind</span>
        <div
          className="segmented kind-toggle"
          title={connections > 0 ? 'Disconnect the port to change its kind: both ends of a connection must match.' : undefined}
        >
          <button
            className={kind === 'event' ? 'active' : ''}
            disabled={connections > 0 && kind !== 'event'}
            onClick={() => setKind('event')}
            data-testid="port-kind-event"
          >
            Event
          </button>
          <button
            className={`request${kind === 'request' ? ' active' : ''}`}
            disabled={connections > 0 && kind !== 'request'}
            onClick={() => setKind('request')}
            data-testid="port-kind-request"
          >
            ⇄ Request / response
          </button>
        </div>
      </div>
      <div className="field">
        <span className="field-label">{kind === 'request' ? 'Request (Markdown)' : 'Contract (Markdown)'}</span>
        <MarkdownField
          value={port.description}
          onChange={(text) => void run(commands.setPortDescription(module.id, port.id, text))}
          testId="port-description"
        />
      </div>
      {kind === 'request' && (
        <div className="field">
          <span className="field-label">Response (Markdown)</span>
          <MarkdownField
            value={port.response ?? ''}
            onChange={(text) => void run(commands.setPortResponse(module.id, port.id, text))}
            testId="port-response"
          />
        </div>
      )}
      <p className="muted">Used by {connections} connection(s).</p>
      <div className="actions">
        <button onClick={() => setRouting(true)} data-testid="route-port" title="Connect to a port on another hierarchy level">
          Route to…
        </button>
      </div>
      {routing && <RouteDialog moduleId={module.id} portId={port.id} onClose={() => setRouting(false)} />}
      <div className="actions">
        <button
          className="danger"
          onClick={() => void run(commands.deletePort(module.id, port.id, cascade))}
          data-testid="delete-port"
        >
          Delete port
        </button>
        <label className="checkbox">
          <input type="checkbox" checked={cascade} onChange={(e) => setCascade(e.target.checked)} />
          also remove its connections
        </label>
      </div>
    </div>
  );
}

function ConnectionDetails() {
  const connection = useEditor((s) => s.view.details.connection);
  if (!connection) return null;
  return (
    <div data-testid="connection-details">
      <h2 className="panel-title">Connection</h2>
      <div className="field">
        <span className="field-label">ID</span>
        <code>{connection.id}</code>
      </div>
      <div className="field">
        <span className="field-label">From</span>
        <span>{connection.from}</span>
      </div>
      <div className="field">
        <span className="field-label">To</span>
        <span>{connection.to}</span>
      </div>
      <p className="muted">
        A connection documents a possible message flow. It implies no ordering or delivery guarantee.
      </p>
      <div className="actions">
        <button
          className="danger"
          onClick={() => void run(commands.deleteConnection(connection.id))}
          data-testid="delete-connection"
        >
          Delete connection
        </button>
      </div>
    </div>
  );
}

/** Details for the selection, or for the module whose graph is open. */
export function DetailsPanel() {
  const modules = useEditor((s) => s.view.document.modules);
  const selection = useEditor((s) => s.view.selection);
  const currentModuleId = useEditor((s) => s.view.currentModuleId);
  const find = (id: string) => (Object.hasOwn(modules, id) ? modules[id] : undefined);

  let content: JSX.Element | null = null;
  if (selection?.kind === 'connection') {
    content = <ConnectionDetails />;
  } else if (selection?.kind === 'port') {
    const module = find(selection.moduleId);
    const lookup = module ? findPort(module, selection.portId) : null;
    if (module && lookup) {
      content = <PortDetails key={`${module.id}:${lookup.port.id}`} module={module} port={lookup.port} direction={lookup.direction} />;
    }
  }
  if (!content) {
    const module = find(selection?.kind === 'module' ? selection.moduleId : currentModuleId);
    if (module) content = <ModuleDetails key={module.id} module={module} />;
  }
  return (
    <aside className="panel details" data-testid="details">
      {content}
    </aside>
  );
}
