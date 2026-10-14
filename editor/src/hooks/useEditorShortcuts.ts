import { getCurrentWindow } from '@tauri-apps/api/window';
import { useEffect } from 'react';
import { commands } from '../api';
import { getView, run, useEditor } from '../state/store';
import { confirmDiscard } from './useFileActions';

function isTyping(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName);
}

/**
 * Delete/Backspace removes the selected element (safe, non-cascading),
 * Ctrl/Cmd+Z undoes, Ctrl/Cmd+Shift+Z or Ctrl+Y redoes, Ctrl/Cmd+S saves,
 * and closing the window with unsaved changes asks first. While typing in a
 * field, the field's own text undo applies instead. Also keeps the window
 * title in sync with the file name.
 */
export function useEditorShortcuts(save: () => void): void {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (document.querySelector('[aria-modal="true"]')) return; // dialogs handle their own keys
      const mod = event.ctrlKey || event.metaKey;
      const key = event.key.toLowerCase();
      if (mod && key === 's') {
        event.preventDefault();
        save();
        return;
      }
      if (mod && (key === 'z' || key === 'y') && !isTyping(event.target)) {
        event.preventDefault();
        void run(key === 'y' || event.shiftKey ? commands.redo() : commands.undo());
        return;
      }
      if ((event.key !== 'Delete' && event.key !== 'Backspace') || isTyping(event.target)) return;
      if (!getView().selection) return;
      event.preventDefault();
      void run(commands.deleteSelection());
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [save]);

  useEffect(() => {
    const unlisten = getCurrentWindow().onCloseRequested(async (event) => {
      if (!(await confirmDiscard())) event.preventDefault();
    });
    return () => void unlisten.then((stop) => stop());
  }, []);

  const fileName = useEditor((s) => s.view.fileName);
  const dirty = useEditor((s) => s.view.dirty);
  useEffect(() => {
    void getCurrentWindow().setTitle(`${dirty ? '● ' : ''}${fileName} — MDG Editor`);
  }, [fileName, dirty]);
}
