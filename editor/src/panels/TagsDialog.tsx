import { useEffect, useMemo } from 'react';
import { commands, moduleTags } from '../api';
import { CommitInput } from '../components/CommitInput';
import { run, useEditor } from '../state/store';

/** Colour of a tag created from the editor; changed right away in the list. */
const NEW_TAG_COLOR = '#6b7385';

/**
 * The document's tag list: add, rename, recolour, reorder and remove tags.
 * Every change is a session edit (undoable, saved with the file). Removing a
 * tag also removes it from every module that carries it.
 */
export function TagsDialog({ onClose }: { onClose: () => void }) {
  const tags = useEditor((s) => s.view.document.tags);
  const modules = useEditor((s) => s.view.document.modules);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  const usage = useMemo(() => {
    const counts = new Map<string, number>();
    for (const module of Object.values(modules)) {
      for (const id of moduleTags(module)) counts.set(id, (counts.get(id) ?? 0) + 1);
    }
    return counts;
  }, [modules]);

  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal" role="dialog" aria-modal="true" aria-labelledby="tags-title" data-testid="tags-dialog">
        <h2 id="tags-title" className="modal-title">
          Document tags
        </h2>
        <p className="muted">Tags are stored in this file. Any module can carry any of them.</p>
        {tags.length === 0 && <p className="muted">No tags.</p>}
        <ul className="tag-editor">
          {tags.map((tag, index) => {
            const used = usage.get(tag.id) ?? 0;
            return (
              <li key={tag.id} title={`ID: ${tag.id}`} data-testid={`tag-row-${tag.id}`}>
                <input
                  type="color"
                  className="tag-color"
                  aria-label={`Colour of ${tag.name}`}
                  value={tag.color}
                  onChange={(e) => void run(commands.updateTag(tag.id, { color: e.target.value }))}
                />
                <CommitInput
                  label={`Name of ${tag.name}`}
                  value={tag.name}
                  onCommit={(name) => void run(commands.updateTag(tag.id, { name }))}
                />
                <span className="muted tag-usage">{used} module(s)</span>
                <span className="actions-inline">
                  <button
                    className="small"
                    disabled={index === 0}
                    onClick={() => void run(commands.moveTag(tag.id, index - 1))}
                    aria-label={`Move ${tag.name} up`}
                  >
                    ↑
                  </button>
                  <button
                    className="small"
                    disabled={index === tags.length - 1}
                    onClick={() => void run(commands.moveTag(tag.id, index + 1))}
                    aria-label={`Move ${tag.name} down`}
                  >
                    ↓
                  </button>
                  <button
                    className="small danger"
                    onClick={() => void run(commands.removeTag(tag.id))}
                    title={used > 0 ? `Also removes it from ${used} module(s)` : undefined}
                    data-testid={`remove-tag-${tag.id}`}
                  >
                    Remove
                  </button>
                </span>
              </li>
            );
          })}
        </ul>
        <div className="actions modal-actions">
          <button onClick={() => void run(commands.addTag('new tag', NEW_TAG_COLOR))} data-testid="add-tag">
            + Tag
          </button>
          <button className="primary" onClick={onClose}>
            Done
          </button>
        </div>
      </div>
    </div>
  );
}
