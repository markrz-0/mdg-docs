import { useEffect, useState } from 'react';
import { commands, type DocumentView } from '../api';
import { tagStyle } from '../components/TagChip';
import { getModule, resolveTags } from '../graph/buildGraph';
import { run, useEditor } from '../state/store';

interface ItemProps {
  doc: DocumentView;
  moduleId: string;
  depth: number;
  expanded: Set<string>;
  toggle: (id: string) => void;
}

function TreeItem({ doc, moduleId, depth, expanded, toggle }: ItemProps) {
  const module = getModule(doc, moduleId);
  const currentModuleId = useEditor((s) => s.view.currentModuleId);
  const selection = useEditor((s) => s.view.selection);
  const isOpen = expanded.has(moduleId);
  const isCurrent = currentModuleId === moduleId;
  const isSelected = selection?.kind === 'module' && selection.moduleId === moduleId;

  // Click: show the module on its parent's canvas and select it.
  const onClick = async () => {
    if (module.parentId === null) {
      await run(commands.navigate(moduleId));
      return;
    }
    await run(commands.navigate(module.parentId));
    await run(commands.select({ kind: 'module', moduleId }));
  };

  return (
    <li>
      <div
        className={`tree-row${isCurrent ? ' current' : ''}${isSelected ? ' selected' : ''}`}
        style={{ paddingLeft: depth * 14 + 4 }}
      >
        <button
          className="tree-toggle"
          onClick={() => toggle(moduleId)}
          aria-label={isOpen ? 'Collapse' : 'Expand'}
          data-testid={`toggle-${moduleId}`}
          style={{ visibility: module.childIds.length ? 'visible' : 'hidden' }}
        >
          {isOpen ? '▾' : '▸'}
        </button>
        <button
          className="tree-label"
          onClick={() => void onClick()}
          onDoubleClick={() => void run(commands.navigate(moduleId))}
          data-testid={`tree-${moduleId}`}
          title="Click to select, double-click to open its internal graph"
        >
          {module.name}
        </button>
        {resolveTags(doc, module).map((tag) => (
          <span key={tag.id} className="tag-dot tree-tag" style={tagStyle(tag)} title={tag.name} />
        ))}
      </div>
      {isOpen && module.childIds.length > 0 && (
        <ul>
          {module.childIds.map((id) => (
            <TreeItem key={id} doc={doc} moduleId={id} depth={depth + 1} expanded={expanded} toggle={toggle} />
          ))}
        </ul>
      )}
    </li>
  );
}

/** Module hierarchy. Expansion is local UI state; the open module's path auto-expands. */
export function HierarchyPanel() {
  const doc = useEditor((s) => s.view.document);
  const path = useEditor((s) => s.view.path);
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set([doc.rootId]));

  useEffect(() => {
    setExpanded((prev) => {
      const next = new Set(prev);
      next.add(doc.rootId);
      for (const m of path) next.add(m.id);
      return next.size === prev.size ? prev : next;
    });
  }, [doc.rootId, path]);

  const toggle = (id: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  return (
    <aside className="panel hierarchy" data-testid="hierarchy">
      <h2 className="panel-title">Hierarchy</h2>
      <ul className="tree">
        <TreeItem doc={doc} moduleId={doc.rootId} depth={0} expanded={expanded} toggle={toggle} />
      </ul>
    </aside>
  );
}
