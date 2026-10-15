import { invoke } from '@tauri-apps/api/core';

/**
 * Types and commands of the Rust session (editor/src-tauri). The session owns
 * the document, undo history and files; every command returns the new
 * {@link View}. The UI never changes the model itself.
 */

export type PortDirection = 'input' | 'output';

/**
 * `event`: one-way message (outputs emit, inputs listen).
 * `request`: a request answered over the same connection (outputs send it,
 * inputs handle it).
 */
export type PortKind = 'event' | 'request';

export interface Port {
  id: string;
  name: string;
  /** Omitted for event ports. */
  kind?: PortKind;
  /** For a request port: the request contract. */
  description: string;
  /** Request ports only; omitted when empty. */
  response?: string;
}

export const portKind = (port: Port): PortKind => port.kind ?? 'event';

export interface Endpoint {
  moduleId: string;
  portId: string;
}

export interface Connection {
  id: string;
  from: Endpoint;
  to: Endpoint;
}

/** A document-level label modules can carry. */
export interface Tag {
  id: string;
  name: string;
  /** `#rrggbb`. */
  color: string;
}

export interface Module {
  id: string;
  parentId: string | null;
  name: string;
  description: string;
  inputs: Port[];
  outputs: Port[];
  childIds: string[];
  connections: Connection[];
  /** IDs of the document tags the module carries; omitted when empty. */
  tags?: string[];
}

export const moduleTags = (module: Module): string[] => module.tags ?? [];

export interface Position {
  x: number;
  y: number;
}

export interface DocumentView {
  rootId: string;
  title: string;
  modules: Record<string, Module>;
  tags: Tag[];
}

export type Selection =
  | { kind: 'module'; moduleId: string }
  | { kind: 'port'; moduleId: string; portId: string }
  | { kind: 'connection'; connectionId: string }
  | null;

export interface View {
  document: DocumentView;
  /** Flat layout: module ID, or `<moduleId>\0input|output` for boundary panels. */
  positions: Record<string, Position>;
  fileName: string;
  filePath: string | null;
  dirty: boolean;
  currentModuleId: string;
  selection: Selection;
  error: string | null;
  canUndo: boolean;
  canRedo: boolean;
  /** Incremented by auto-layout so the canvas can re-fit the view. */
  layoutVersion: number;
  validation: { valid: boolean; errors: number; warnings: number };
  /** Root-to-current module path. */
  path: Array<{ id: string; name: string }>;
  details: {
    portConnections?: number;
    connection?: { id: string; from: string; to: string };
  };
}

export interface Outcome {
  ok: boolean;
  view: View;
}

export type RouteMode = 'send' | 'receive';

export interface RouteCandidate {
  endpoint: Endpoint;
  label: string;
  search: string;
}

export interface RouteChoices {
  sourceLabel: string;
  sourceDirection: PortDirection;
  candidates: RouteCandidate[];
}

export type RoutePreview =
  | { status: 'ok'; token: number; joinName: string; steps: string[]; alreadyRouted: boolean }
  | { status: 'error'; message: string };

export interface RouteRequest {
  source: Endpoint;
  target: Endpoint;
  mode: RouteMode;
  reuse: boolean;
  outputName: string;
  inputName: string;
}

export interface PositionChange {
  key: string;
  x: number;
  y: number;
}

/** Commands that return the new view. */
export const commands = {
  getView: () => invoke<View>('get_view'),
  newDocument: () => invoke<View>('new_document'),
  navigate: (moduleId: string) => invoke<View>('navigate', { moduleId }),
  select: (selection: Selection) => invoke<View>('select', { selection }),
  setError: (message: string | null) => invoke<View>('set_error', { message }),
  undo: () => invoke<View>('undo'),
  redo: () => invoke<View>('redo'),
  setTitle: (title: string) => invoke<View>('set_title', { title }),
  renameModule: (moduleId: string, name: string) => invoke<View>('rename_module_cmd', { moduleId, name }),
  setModuleDescription: (moduleId: string, text: string) => invoke<View>('set_module_description', { moduleId, text }),
  renamePort: (moduleId: string, portId: string, name: string) =>
    invoke<View>('rename_port_cmd', { moduleId, portId, name }),
  setPortDescription: (moduleId: string, portId: string, text: string) =>
    invoke<View>('set_port_description', { moduleId, portId, text }),
  setPortResponse: (moduleId: string, portId: string, text: string) =>
    invoke<View>('set_port_response', { moduleId, portId, text }),
  setPortKind: (moduleId: string, portId: string, kind: PortKind) =>
    invoke<View>('set_port_kind_cmd', { moduleId, portId, kind }),
  setModuleTags: (moduleId: string, tags: string[]) => invoke<View>('set_module_tags_cmd', { moduleId, tags }),
  addTag: (name: string, color: string) => invoke<View>('add_tag_cmd', { name, color }),
  updateTag: (tagId: string, change: { name?: string; color?: string }) =>
    invoke<View>('update_tag_cmd', { tagId, name: change.name ?? null, color: change.color ?? null }),
  moveTag: (tagId: string, index: number) => invoke<View>('move_tag_cmd', { tagId, index }),
  removeTag: (tagId: string) => invoke<View>('remove_tag_cmd', { tagId }),
  addChildModule: (at?: Position) => invoke<View>('add_child_module', { at: at ?? null }),
  addPort: (moduleId: string, direction: PortDirection, kind: PortKind) =>
    invoke<View>('add_port', { moduleId, direction, kind }),
  connect: (from: Endpoint, to: Endpoint) => invoke<View>('connect', { from, to }),
  deleteModule: (moduleId: string, cascade: boolean) => invoke<View>('delete_module', { moduleId, cascade }),
  deletePort: (moduleId: string, portId: string, cascade: boolean) =>
    invoke<View>('delete_port', { moduleId, portId, cascade }),
  deleteConnection: (connectionId: string) => invoke<View>('delete_connection', { connectionId }),
  deleteSelection: () => invoke<View>('delete_selection'),
  setPositions: (changes: PositionChange[]) => invoke<View>('set_positions', { changes }),
  applyLayout: (positions: PositionChange[]) => invoke<View>('apply_layout', { positions }),
};

/** Commands that report success alongside the new view. */
export const outcomes = {
  openFile: (path: string) => invoke<Outcome>('open_file', { path }),
  saveTo: (path: string) => invoke<Outcome>('save_to', { path }),
  exportMarkdown: (path: string) => invoke<Outcome>('export_markdown', { path }),
  commitRoute: (token: number) => invoke<Outcome>('commit_route', { token }),
};

/** Queries that do not change the session. */
export const queries = {
  connectionPartners: (endpoint: Endpoint, role: 'source' | 'target') =>
    invoke<Endpoint[]>('connection_partners', { endpoint, role }),
  routeChoices: (source: Endpoint, mode: RouteMode | null) => invoke<RouteChoices>('route_choices', { source, mode }),
  previewRoute: (request: RouteRequest) => invoke<RoutePreview>('preview_route', { request }),
};
