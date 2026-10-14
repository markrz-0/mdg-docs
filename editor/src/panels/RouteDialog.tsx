import { useEffect, useMemo, useState } from 'react';
import { outcomes, queries, type Endpoint, type RouteChoices, type RouteMode, type RoutePreview } from '../api';
import { runOutcome, useEditor } from '../state/store';

/**
 * Cross-level routing: connect a port to any port in the hierarchy. The
 * session plans the boundary ports and local connections with core's
 * routing; the dialog previews that plan and commits exactly the previewed
 * result as a single undoable step.
 */

const MAX_SHOWN = 200;

export function RouteDialog(props: { moduleId: string; portId: string; onClose: () => void }) {
  const document = useEditor((s) => s.view.document);
  const source: Endpoint = useMemo(() => ({ moduleId: props.moduleId, portId: props.portId }), [props.moduleId, props.portId]);

  const [mode, setMode] = useState<RouteMode | null>(null);
  const [choices, setChoices] = useState<RouteChoices | null>(null);
  const [query, setQuery] = useState('');
  const [target, setTarget] = useState<Endpoint | null>(null);
  const [reuse, setReuse] = useState(true);
  const [outputName, setOutputName] = useState('');
  const [inputName, setInputName] = useState('');
  const [preview, setPreview] = useState<RoutePreview | null>(null);

  const { onClose } = props;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  // Sending reaches inputs; receiving comes from outputs. Without a chosen
  // mode, an output port sends and an input port receives.
  const effectiveMode: RouteMode | null = mode ?? (choices ? (choices.sourceDirection === 'output' ? 'send' : 'receive') : null);
  const wanted = effectiveMode === 'receive' ? 'output' : 'input';

  useEffect(() => {
    let live = true;
    const requested = effectiveMode ?? null;
    void queries
      .routeChoices(source, requested)
      .then((c) => live && setChoices(c))
      .catch(() => live && setChoices(null));
    return () => {
      live = false;
    };
  }, [source, effectiveMode, document]);

  const filtered = useMemo(() => {
    const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
    return (choices?.candidates ?? []).filter((c) => terms.every((t) => c.search.includes(t)));
  }, [choices, query]);

  useEffect(() => {
    if (!target || !effectiveMode) {
      setPreview(null);
      return;
    }
    let live = true;
    void queries
      .previewRoute({ source, target, mode: effectiveMode, reuse, outputName, inputName })
      .then((p) => live && setPreview(p))
      .catch((error: unknown) => live && setPreview({ status: 'error', message: String(error) }));
    return () => {
      live = false;
    };
  }, [source, target, effectiveMode, reuse, outputName, inputName, document]);

  const plan = preview?.status === 'ok' ? preview : null;
  // Every step reused: the route already exists, so there is nothing to commit.
  const alreadyRouted = plan?.alreadyRouted ?? false;

  const confirm = async () => {
    if (plan && !alreadyRouted && (await runOutcome(outcomes.commitRoute(plan.token)))) props.onClose();
  };

  const chooseMode = (next: RouteMode) => {
    setMode(next);
    setTarget(null);
  };

  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}>
      <div className="modal" role="dialog" aria-modal="true" aria-labelledby="route-title" data-testid="route-dialog">
        <h2 id="route-title" className="modal-title">
          Route across levels
        </h2>
        <p className="muted">
          Creates the boundary ports and local connections needed at every level in between. Stored
          connections stay local; nothing is connected across levels directly.
        </p>

        <div className="segmented" role="radiogroup" aria-label="Direction">
          <button
            role="radio"
            aria-checked={effectiveMode === 'send'}
            className={effectiveMode === 'send' ? 'active' : ''}
            onClick={() => chooseMode('send')}
            data-testid="route-mode-send"
          >
            Send from this port
          </button>
          <button
            role="radio"
            aria-checked={effectiveMode === 'receive'}
            className={effectiveMode === 'receive' ? 'active' : ''}
            onClick={() => chooseMode('receive')}
            data-testid="route-mode-receive"
          >
            Receive into this port
          </button>
        </div>

        <div className="route-ends">
          <span className="field-label">{effectiveMode === 'receive' ? 'To' : 'From'}</span>
          <span>{choices?.sourceLabel ?? ''}</span>
        </div>

        <label className="field">
          <span className="field-label">{effectiveMode === 'receive' ? 'From (an output port)' : 'To (an input port)'}</span>
          <input
            className="text-input"
            autoFocus
            placeholder="Search modules and ports…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            data-testid="route-search"
          />
        </label>
        <ul className="candidate-list" role="listbox" aria-label="Ports" data-testid="route-candidates">
          {filtered.slice(0, MAX_SHOWN).map((c) => {
            const selected = target?.moduleId === c.endpoint.moduleId && target.portId === c.endpoint.portId;
            return (
              <li key={`${c.endpoint.moduleId}:${c.endpoint.portId}`}>
                <button
                  role="option"
                  aria-selected={selected}
                  className={selected ? 'selected' : ''}
                  onClick={() => setTarget(c.endpoint)}
                  data-testid={`route-candidate-${c.endpoint.moduleId}-${c.endpoint.portId}`}
                >
                  {c.label}
                </button>
              </li>
            );
          })}
          {choices && filtered.length === 0 && <li className="muted">No matching {wanted} ports.</li>}
          {filtered.length > MAX_SHOWN && <li className="muted">{filtered.length - MAX_SHOWN} more — refine the search.</li>}
        </ul>

        <details className="route-options">
          <summary>Options</summary>
          <label className="checkbox">
            <input type="checkbox" checked={reuse} onChange={(e) => setReuse(e.target.checked)} />
            reuse an existing boundary port when unambiguous (never matched by name)
          </label>
          <label className="field">
            <span className="field-label">Name for generated output ports</span>
            <input className="text-input" value={outputName} onChange={(e) => setOutputName(e.target.value)} placeholder="source port name" />
          </label>
          <label className="field">
            <span className="field-label">Name for generated input ports</span>
            <input className="text-input" value={inputName} onChange={(e) => setInputName(e.target.value)} placeholder="destination port name" />
          </label>
        </details>

        <div className="route-preview" data-testid="route-preview">
          {!preview && <p className="muted">Choose a port to preview the route.</p>}
          {preview?.status === 'error' && (
            <p className="error-text" role="alert">
              {preview.message}
            </p>
          )}
          {plan && (
            <>
              <span className="field-label">Plan (joins in {plan.joinName})</span>
              <ol>
                {plan.steps.map((step, i) => (
                  <li key={i}>{step}</li>
                ))}
              </ol>
            </>
          )}
        </div>

        <div className="actions modal-actions">
          <button onClick={props.onClose}>Cancel</button>
          <button className="primary" disabled={plan === null || alreadyRouted} onClick={() => void confirm()} data-testid="route-confirm">
            {alreadyRouted ? 'Already routed' : 'Create route'}
          </button>
        </div>
      </div>
    </div>
  );
}
