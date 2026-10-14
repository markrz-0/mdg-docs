import { useState } from 'react';
import { commands, type Module, type Port, type PortDirection } from '../api';
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

function PortList({ module, direction }: { module: Module; direction: PortDirection }) {
  const ports = direction === 'input' ? module.inputs : module.outputs;
  return (
    <section className="field">
      <div className="field-header">
        <span className="field-label">{direction === 'input' ? 'Inputs' : 'Outputs'}</span>
        <button
          className="small"
          onClick={() => void run(commands.addPort(module.id, direction))}
          data-testid={`add-${direction}`}
        >
          + {direction === 'input' ? 'Input' : 'Output'}
        </button>
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
  return (
    <div data-testid="port-details">
      <h2 className="panel-title">{direction === 'input' ? 'Input port' : 'Output port'}</h2>
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
        <span className="field-label">Contract (Markdown)</span>
        <MarkdownField
          value={port.description}
          onChange={(text) => void run(commands.setPortDescription(module.id, port.id, text))}
          testId="port-description"
        />
      </div>
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
