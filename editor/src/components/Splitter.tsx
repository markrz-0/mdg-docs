import { useCallback, useState, type PointerEvent } from 'react';

const MIN_WIDTH = 160;
/** Space always left to the canvas, whatever the side panels are dragged to. */
const MIN_CANVAS = 240;

function readWidth(key: string, fallback: number): number {
  try {
    const stored = Number(localStorage.getItem(key));
    return stored > 0 ? stored : fallback;
  } catch {
    return fallback;
  }
}

/** A side panel width that survives reloads (per viewer, best effort). */
export function usePanelWidth(key: string, fallback: number) {
  const [width, setWidth] = useState(() => readWidth(key, fallback));
  const update = useCallback(
    (next: number) => {
      setWidth(next);
      try {
        localStorage.setItem(key, String(Math.round(next)));
      } catch {
        // Storage unavailable: the width just won't persist.
      }
    },
    [key],
  );
  return [width, update] as const;
}

interface SplitterProps {
  /** Which side of the splitter the resized panel is on. */
  side: 'left' | 'right';
  width: number;
  /** Width of the panel on the other side of the canvas, so the canvas keeps room. */
  otherWidth: number;
  onResize: (width: number) => void;
  label: string;
}

/** A vertical drag handle between a side panel and the canvas. */
export function Splitter({ side, width, otherWidth, onResize, label }: SplitterProps) {
  const [dragging, setDragging] = useState(false);

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const handle = e.currentTarget;
    handle.setPointerCapture(e.pointerId);
    setDragging(true);
    const startX = e.clientX;
    const startWidth = width;
    const max = Math.max(MIN_WIDTH, window.innerWidth - otherWidth - MIN_CANVAS);

    const move = (ev: globalThis.PointerEvent) => {
      const delta = side === 'left' ? ev.clientX - startX : startX - ev.clientX;
      onResize(Math.min(max, Math.max(MIN_WIDTH, startWidth + delta)));
    };
    const up = (ev: globalThis.PointerEvent) => {
      handle.releasePointerCapture(ev.pointerId);
      handle.removeEventListener('pointermove', move);
      handle.removeEventListener('pointerup', up);
      handle.removeEventListener('pointercancel', up);
      setDragging(false);
    };
    handle.addEventListener('pointermove', move);
    handle.addEventListener('pointerup', up);
    handle.addEventListener('pointercancel', up);
  };

  return (
    <div
      className={`splitter${dragging ? ' dragging' : ''}`}
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuenow={Math.round(width)}
      onPointerDown={onPointerDown}
      data-testid={`splitter-${side}`}
    />
  );
}
