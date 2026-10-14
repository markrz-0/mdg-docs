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
  return (
    <div className="app">
      <TopBar files={files} />
      <main className="workspace">
        <HierarchyPanel />
        <Canvas />
        <DetailsPanel />
      </main>
      <StatusBar />
    </div>
  );
}
