import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import '@xyflow/react/dist/style.css';
import './styles.css';
import { commands } from './api';
import { App } from './App';
import { useEditor } from './state/store';

// The session (Rust) owns the state; render once its first view is in.
const view = await commands.getView();
useEditor.setState({ view });

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
