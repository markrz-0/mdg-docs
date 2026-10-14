import { ask, open, save } from '@tauri-apps/plugin-dialog';
import { useMemo } from 'react';
import { commands, outcomes } from '../api';
import { getView, run, runOutcome } from '../state/store';

const MDG_FILTERS = [{ name: 'MDG document', extensions: ['mdg'] }];
const MARKDOWN_FILTERS = [{ name: 'Markdown', extensions: ['md'] }];

/** Asks before throwing away unsaved changes. */
export async function confirmDiscard(): Promise<boolean> {
  return !getView().dirty || (await ask('Discard unsaved changes?', { title: 'MDG Editor', kind: 'warning' }));
}

/**
 * New / Open / Save / Save As / Export. Native dialogs pick the paths; the
 * Rust session reads, validates and writes the files (atomically).
 */
export function useFileActions() {
  return useMemo(() => {
    const saveAs = async (): Promise<boolean> => {
      const path = await save({ defaultPath: getView().fileName, filters: MDG_FILTERS });
      if (!path) return false; // cancelled
      return runOutcome(outcomes.saveTo(path));
    };

    return {
      async newDocument() {
        if (await confirmDiscard()) await run(commands.newDocument());
      },

      async open() {
        if (!(await confirmDiscard())) return;
        const path = await open({ multiple: false, directory: false, filters: MDG_FILTERS });
        if (typeof path === 'string') await runOutcome(outcomes.openFile(path));
      },

      async save(): Promise<boolean> {
        const { filePath } = getView();
        return filePath ? runOutcome(outcomes.saveTo(filePath)) : saveAs();
      },

      saveAs,

      async exportMarkdown() {
        const name = getView().fileName.replace(/\.mdg$/i, '') + '.md';
        const path = await save({ defaultPath: name, filters: MARKDOWN_FILTERS });
        if (path) await runOutcome(outcomes.exportMarkdown(path));
      },
    };
  }, []);
}
