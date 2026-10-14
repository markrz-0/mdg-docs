import { commands } from '../api';
import { run, useEditor } from '../state/store';

/** Validation summary (computed by the session) plus the last rejected operation, if any. */
export function StatusBar() {
  const validation = useEditor((s) => s.view.validation);
  const error = useEditor((s) => s.view.error);
  return (
    <footer className="statusbar">
      <span className={validation.valid ? 'ok' : 'bad'} data-testid="validation-summary">
        {validation.valid ? 'Valid' : 'Invalid'} · {validation.errors} error(s) · {validation.warnings} warning(s)
      </span>
      {error && (
        <span className="error" role="alert" data-testid="error">
          {error}
          <button className="link" onClick={() => void run(commands.setError(null))} aria-label="Dismiss">
            ✕
          </button>
        </span>
      )}
    </footer>
  );
}
