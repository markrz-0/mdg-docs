import { create } from 'zustand';
import type { Outcome, View } from '../api';

/**
 * The UI's copy of the session view. It is replaced wholesale by the result
 * of every command; the Rust session is the single source of truth.
 */
interface EditorState {
  view: View;
}

export const useEditor = create<EditorState>()(() => ({ view: null as unknown as View }));

export const getView = (): View => useEditor.getState().view;

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Shows a failure that happened outside the session (e.g. IPC). */
function showError(error: unknown): void {
  useEditor.setState((s) => ({ view: { ...s.view, error: message(error) } }));
}

/** Runs a session command and installs the view it returns. */
export async function run(command: Promise<View>): Promise<View | null> {
  try {
    const view = await command;
    useEditor.setState({ view });
    return view;
  } catch (error) {
    showError(error);
    return null;
  }
}

/** Like {@link run} for commands that also report success. */
export async function runOutcome(command: Promise<Outcome>): Promise<boolean> {
  try {
    const { ok, view } = await command;
    useEditor.setState({ view });
    return ok;
  } catch (error) {
    showError(error);
    return false;
  }
}
