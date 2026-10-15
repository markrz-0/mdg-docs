import { Splitter, usePanelWidth } from './components/Splitter';
import { StatusBar } from './components/StatusBar';
import { TopBar } from './components/TopBar';
import { Canvas } from './graph/Canvas';
import { useEditorShortcuts } from './hooks/useEditorShortcuts';
import { useFileActions } from './hooks/useFileActions';
import { DetailsPanel } from './panels/DetailsPanel';
import { HierarchyPanel } from './panels/HierarchyPanel';

export function App() {
  const files = useFileActions();
  useEditorShortcuts(() => void files.save());
  const [hierarchyWidth, setHierarchyWidth] = usePanelWidth('mdg.hierarchyWidth', 240);
  const [detailsWidth, setDetailsWidth] = usePanelWidth('mdg.detailsWidth', 340);
  return (
    <div className="app">
      <TopBar files={files} />
      <main
        className="workspace"
        style={{ gridTemplateColumns: `${hierarchyWidth}px auto minmax(0, 1fr) auto ${detailsWidth}px` }}
      >
        <HierarchyPanel />
        <Splitter
          side="left"
          width={hierarchyWidth}
          otherWidth={detailsWidth}
          onResize={setHierarchyWidth}
          label="Resize hierarchy panel"
        />
        <Canvas />
        <Splitter
          side="right"
          width={detailsWidth}
          otherWidth={hierarchyWidth}
          onResize={setDetailsWidth}
          label="Resize module panel"
        />
        <DetailsPanel />
      </main>
      <StatusBar />
    </div>
  );
}
