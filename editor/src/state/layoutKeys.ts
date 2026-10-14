/**
 * Position keys of the flat layout map kept by the session: child modules
 * use their module ID; a module's own boundary panels use
 * `<moduleId>\0input` / `<moduleId>\0output` (module IDs never contain NUL).
 */

export type BoundarySide = 'input' | 'output';

export function boundaryKey(moduleId: string, side: BoundarySide): string {
  return `${moduleId}\u0000${side}`;
}
