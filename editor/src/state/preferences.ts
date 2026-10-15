import { create } from 'zustand';

/**
 * Per-viewer display preferences. They are not part of the document, so
 * they live outside the Rust session and survive reloads in local storage
 * (best effort).
 */

/** `solid`: every module in the default colour. `tags`: modules take their tag colour (mixed for several tags). */
export type NodeColoring = 'solid' | 'tags';

const COLORING_KEY = 'mdg.nodeColoring';

function readColoring(): NodeColoring {
  try {
    return localStorage.getItem(COLORING_KEY) === 'tags' ? 'tags' : 'solid';
  } catch {
    return 'solid';
  }
}

interface Preferences {
  nodeColoring: NodeColoring;
  setNodeColoring: (coloring: NodeColoring) => void;
}

export const usePreferences = create<Preferences>()((set) => ({
  nodeColoring: readColoring(),
  setNodeColoring: (nodeColoring) => {
    set({ nodeColoring });
    try {
      localStorage.setItem(COLORING_KEY, nodeColoring);
    } catch {
      // Storage unavailable: the choice just won't persist.
    }
  },
}));
