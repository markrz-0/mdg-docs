import { useState } from 'react';
import { commands } from '../api';
import type { useFileActions } from '../hooks/useFileActions';
import { TagsDialog } from '../panels/TagsDialog';
import { run, useEditor } from '../state/store';

function Breadcrumbs() {
  const path = useEditor((s) => s.view.path);
  return (
    <nav className="breadcrumbs" aria-label="Module path" data-testid="breadcrumbs">
      {path.map((m, i) => (
        <span key={m.id} className="crumb">
          {i > 0 && <span className="crumb-sep">›</span>}
          {i === path.length - 1 ? (
            <span className="crumb-current" data-testid="crumb-current">
              {m.name}
            </span>
          ) : (
            <button className="link" onClick={() => void run(commands.navigate(m.id))} data-testid={`crumb-${m.id}`}>
              {m.name}
            </button>
          )}
        </span>
      ))}
    </nav>
  );
}

export function TopBar({ files }: { files: ReturnType<typeof useFileActions> }) {
  const fileName = useEditor((s) => s.view.fileName);
  const dirty = useEditor((s) => s.view.dirty);
  const canUndo = useEditor((s) => s.view.canUndo);
  const canRedo = useEditor((s) => s.view.canRedo);
  const [editingTags, setEditingTags] = useState(false);
  return (
    <header className="topbar">
      <div className="topbar-row">
        <span className="brand">MDG</span>
        <span className="file-name" data-testid="file-name">
          {fileName}
          {dirty && (
            <span className="dirty" title="Unsaved changes" data-testid="dirty">
              ●
            </span>
          )}
        </span>
        <div className="toolbar">
          <button onClick={() => void files.newDocument()}>New</button>
          <button onClick={() => void files.open()} data-testid="open">
            Open…
          </button>
          <button onClick={() => void files.save()} data-testid="save">
            Save
          </button>
          <button onClick={() => void files.saveAs()}>Save As…</button>
          <button onClick={() => void files.exportMarkdown()} data-testid="export-md">
            Export Markdown
          </button>
          <span className="toolbar-sep" />
          <button onClick={() => void run(commands.undo())} disabled={!canUndo} title="Undo (Ctrl+Z)" data-testid="undo">
            Undo
          </button>
          <button
            onClick={() => void run(commands.redo())}
            disabled={!canRedo}
            title="Redo (Ctrl+Shift+Z)"
            data-testid="redo"
          >
            Redo
          </button>
          <span className="toolbar-sep" />
          <button onClick={() => setEditingTags(true)} title="Edit the document's tags" data-testid="tags">
            Tags…
          </button>
          <button className="primary" onClick={() => void run(commands.addChildModule())} data-testid="add-module">
            + Module
          </button>
        </div>
      </div>
      <Breadcrumbs />
      {editingTags && <TagsDialog onClose={() => setEditingTags(false)} />}
    </header>
  );
}
