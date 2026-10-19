import type { CSSProperties, ReactNode } from 'react';
import type { Tag } from '../api';

/** CSS custom property carrying a tag's colour into the chip styles. */
export const tagStyle = (tag: Tag): CSSProperties => ({ '--tag': tag.color }) as CSSProperties;

/** A tag as a coloured pill. `children` replaces the name (e.g. for a count). */
export function TagChip({ tag, children }: { tag: Tag; children?: ReactNode }) {
  return (
    <span className="tag-chip" style={tagStyle(tag)} title={tag.name}>
      <span className="tag-dot" />
      {children ?? tag.name}
    </span>
  );
}
