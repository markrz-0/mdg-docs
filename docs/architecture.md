# Architecture

```
crates/
  mdg-core/     model, validation, operations, routing, trace, documentation,
                .mdg (de)serialization and file I/O. No UI code.
  mdg-cli/      the `mdg` executable. Thin layer over mdg-core.
  mdg-session/  editor state: undo history, layout, routing previews, files.
editor/         the desktop editor (Tauri 2)
  src-tauri/    Rust: Tauri commands wrapping an mdg-session Session
  src/          React + React Flow UI that renders the session's View
examples/       authentication.mdg (byte-identical with the TypeScript original)
scripts/        gen-golden.mjs (records the CLI golden corpus)
```

The CLI and the editor share every rule through `mdg-core`. Neither
re-implements validation or graph logic. The editor's UI does not contain
model logic at all: it renders a `View` produced by Rust and calls
commands.

## Core (`mdg-core`)

| Module | Contents |
|---|---|
| `model` | `Document`, `Module`, `Port`, `Connection`, `Layout` |
| `queries`, `trace` | `get_module`, `get_ancestors`, `walk_descendants`, LCA, ...; `trace` |
| `validate` | `validate_document`, `validate_module`, `check_connection` |
| `ops` | pure edit operations, `transaction`, `route_connection` |
| `serialization` | header/index/content encoding, `ByteSource` random-access reads, atomic file writes |
| `documentation` | `describe_module` (JSON/Markdown), `export_document_markdown`, `render_tree` |
| `js`, `jsmap` | JavaScript-compatible JSON output, string trimming and key order |
| `example` | the reference authentication architecture |

### Model

`Document.modules` is a flat map from ID to `Arc<Module>`. Each module
stores its `parent_id`, ordered `child_ids`, its ports, and the connections
of **its own internal graph**. An edit clones the map (pointers) and the
touched modules only, so snapshots share everything else. Every lookup is
O(1).

### Operations are pure

Every operation (`add_module`, `remove_port`, `add_connection`, ...) takes
`&Document` and returns a new `Document` or an `MdgError`. This gives:

- **atomicity**: a multi-step edit (`transaction`, `route_connection`) works
  on snapshots. If any step or the final validation fails, the caller still
  holds the untouched original;
- **dry runs**: compute the result and do not commit it;
- **cheap undo** in the editor: a history step is two `Arc`s.

### Errors

`MdgError` carries a stable `code`: `NOT_FOUND`, `INVALID_ARGUMENT`,
`INVALID_OPERATION`, `VALIDATION_FAILED`, `FORMAT_ERROR`,
`UNSUPPORTED_VERSION`, `IO_ERROR`. The CLI maps these to exit codes.

### Traversal

All tree walks (ancestors, descendants, cycle detection, tree rendering,
trace) are iterative with explicit stacks. A 2000-level hierarchy is covered
by tests. The CLI runs on a thread with a large stack so that serializing
very deep trees as JSON cannot overflow.

## CLI (`mdg-cli`)

`run_cli(argv, io)` is the whole CLI; `io` abstracts stdout, stderr and
stdin so tests can run in-process. Argument parsing is a small
re-implementation of Commander.js 12 (`commander.rs`) because the original
CLI's help text, error messages and parsing quirks are part of its
interface for agents. Read commands use `read_partial`: they open the file
once, read the index, decide which modules' Markdown they need, and read
only those blocks. Edit commands use one pipeline:
`read_mdg → core operation → assert_valid → write_mdg` (atomic), or stop
before writing for `--dry-run`.

## Editor

| Concern | Where |
|---|---|
| canonical document | `Session.document` (changed only via core operations) |
| navigation | `Session.current_module_id` |
| selection | `Session.selection` (module / port / connection) |
| layout (positions) | `Session.positions`, kept out of `document`; merged into the file's `layout` on save |
| unsaved changes | `dirty`, which compares the current snapshot with `saved` by identity |
| undo / redo | `past` / `future`: stacks of `(Arc<Document>, Arc<Positions>)` snapshots |
| what the UI shows | `Session::view()` → `View` (document, positions, validation, breadcrumbs, details) |
| Tauri commands | `editor/src-tauri/src/lib.rs` |
| React Flow projection | `editor/src/graph/buildGraph.ts` (pure) |
| auto-layout | `editor/src/graph/autoLayout.ts` (dagre); committed through the session |
| cross-level routing UI | `editor/src/panels/RouteDialog.tsx` (previews `Session::preview_route`) |
| file dialogs | `editor/src/hooks/useFileActions.ts`; reading and writing happen in Rust |

Every UI event becomes a Tauri command. The command runs a session method,
which runs a core operation, and returns the new `View`, which replaces the
UI's copy. A rejected edit leaves the document unchanged and comes back as
`view.error`. While a connection is being dragged, the canvas asks the
session for the handles it may end at (`connection_partners`), so the
canvas refuses exactly what `connect` would reject. Inside a non-root
module, its own ports are drawn as two boundary nodes: inputs on the left
(sources) and outputs on the right (destinations). The module is never
drawn as its own child.

## Architectural decisions

Ambiguities in the specification were resolved as follows. Each choice is
the simplest one consistent with the core principles. Decisions 1–20 were
made for the original TypeScript implementation and carry over unless
noted; 21–25 are about the Rust port.

1. **Build tooling: a Cargo workspace.** The editor UI additionally uses npm
   (Vite, React). `cargo test` covers core, CLI and session without
   building the UI.
2. **ID syntax.** `^[A-Za-z0-9][A-Za-z0-9_.-]{0,127}$`. Excluding `:` and `/`
   makes the CLI's `module:port` notation and the content IDs
   (`port/<module>/<port>`) unambiguous. It also rules out `__proto__`.
   Generated IDs are `mod-<uuid>`, `port-<uuid>`, `conn-<uuid>`. Callers may
   supply readable IDs.
3. **Connection IDs are unique across the whole document**, not just per
   module, so `mdg disconnect <id>` works without naming a container.
4. **Duplicate connections** (same `from` and `to` in one module) are
   rejected by `add_connection` and reported as a *warning* by validation.
5. **Empty names** are rejected by operations. Names are trimmed.
6. **Unconnected ports** are warnings. A port counts as connected if any
   connection anywhere references it.
7. **Removal is safe by default.** `remove_module` fails if the module has
   children or is connected in its parent. `remove_port` fails if any
   connection uses the port. `cascade` removes descendants and the
   referencing connections explicitly.
8. **Moving** rejects the root, parent cycles, and any move while the module
   is connected in its old parent. Connections are never rewritten. A move
   drops the module's stored canvas position, since it means nothing under a
   different parent.
9. **Boundary routing reuse rule.** At each level, an existing boundary port
   is reused only when named explicitly (`via`), or when exactly one
   existing connection already carries the current source to a boundary
   port of that module. Names are never compared. Generated ports take the
   source port's name on the way up and the destination port's name on the
   way down. Their description says they were generated by routing.
10. **Trace semantics.** Forward tracing follows connections whose `from` is
    the current port; backward follows `to`. A route ends at a port with no
    further connections (`terminal`), at a repeated port (`cycle`), or at
    `max_depth`. Results are capped by `max_routes`, and capped output is
    flagged as truncated.
11. **Layout** lives in the optional index field `layout`, in the same
    single file but separate from the logical structure. `positions` maps a
    module ID to `{x, y}` inside its parent's canvas. `boundaries` maps a
    module ID to the positions of its own boundary panels inside its canvas.
    Panels without a stored position are placed beside the children.
    Removing a module drops its layout entries. Moving a module drops its
    position in the old parent but keeps its internal layout. The session
    keeps all positions in one flat map for undo (boundary keys are
    `<moduleId>\0input` / `<moduleId>\0output`) and splits it on save.
12. **Unknown data.** Unknown top-level index keys and metadata keys are
    preserved. Unknown fields in module/port/connection records are
    rejected rather than silently dropped.
13. **Reading invalid documents.** Format corruption is rejected on read.
    Structural invalidity is not, so `mdg validate` can report it. The
    editor refuses to *open* structurally invalid files and points to
    `mdg validate`. All writers refuse to *save* invalid documents.
14. **Markdown safety.** The editor renders with `react-markdown` and
    `skipHtml`. Raw HTML is dropped, and the default URL transform strips
    `javascript:` and similar protocols. Nothing is executed. Links never
    navigate the editor window; `http(s)` and `mailto` links open in the
    system's default application. The window's content security policy only
    allows the app's own scripts.
15. **CLI ID resolution.** Exact IDs always win. Otherwise a unique ID prefix
    is accepted, and ambiguous prefixes fail with the candidates listed.
    Names are never used to resolve IDs.
16. **Description editing commands.** `mdg module set-description` and
    `mdg port set-description` exist so agents can modify descriptions.
17. **`mdg new`** creates a document from the CLI, so an agent can work
    end-to-end without the editor.
18. **Undo history stores whole snapshots.** Documents are immutable and
    share unchanged modules, so each step is two `Arc`s (`document`,
    `positions`). Every session mutation goes through one `commit`. Model
    edits, node moves (a multi-node drag is one step) and auto-layout are
    undoable; navigation and selection are not. Edits that share a coalesce
    key within 1.5 s (typing in a description) merge into one step. The
    history holds at most 200 steps and is cleared on load/new. In text
    fields, Ctrl+Z is left to the field's own text undo.
19. **Auto-layout lives in the editor UI, not in core.** It only produces
    presentation positions, which the session commits as one undo step.
    dagre lays out left to right, and the open module's boundary inputs and
    outputs act as virtual first and last ranks. Their vertical position
    comes from dagre; horizontally they are pinned left and right of the
    children. Like modules, they stay freely draggable afterwards. The open
    level uses measured node sizes; other levels use estimates.
20. **Editor routing** goes through a "Route to…" dialog on a port. Sending
    offers every input port in the document as a target; receiving offers
    every output port. The dialog shows the session's plan (created and
    reused ports and connections) under a token, and committing the token
    installs *exactly* that previewed document as one undo step. If the
    document changed in the meantime, the commit is refused. A plan that
    only reuses existing ports and connections has nothing to commit.
    Explicit `via` ports remain CLI-only.
21. **The editor's logic is in Rust.** Tauri renders the UI in a webview; the
    UI is React + React Flow, but it only renders `View`s and sends
    commands. State, history, validation, routing and file I/O live in
    `mdg-session` and `mdg-core`, so they are unit-tested in Rust and
    shared with the CLI. There is no browser build.
22. **Byte compatibility with the TypeScript implementation.** `.mdg` files
    are written exactly as before: the index is serialized like
    `JSON.stringify` (`js.rs`), maps enumerate keys in JavaScript object
    order (`jsmap.rs`), keys are sorted by UTF-16 code units, and strings
    are trimmed with JavaScript's whitespace set. Schema errors in the index
    use the original (zod) wording. One known difference: when the index is
    not valid JSON at all, the parser's message differs.
23. **CLI parity through a recorded corpus.** Rather than a modern argument
    parser, the CLI re-implements the parts of Commander.js 12 it used, and
    `scripts/gen-golden.mjs` records 233 invocations of the original CLI
    (help, usage errors, every command, edit sequences) that the Rust
    binary must reproduce byte for byte. File-system error messages mimic
    Node.js (`ENOENT: no such file or directory, open '…'`).
24. **Opening files in the editor.** Native dialogs (Tauri dialog plugin)
    choose paths; the session reads, validates and writes. Saving goes
    through the same atomic `write_file_atomic` as the CLI. A `.mdg` path
    passed on the command line (or by the Windows file association
    installed with the bundle) is opened at startup. Closing the window with
    unsaved changes asks first.
25. **Connection checks while dragging.** React Flow needs a synchronous
    `isValidConnection`, but the rules live in Rust. When a drag starts, the
    canvas fetches the valid partner handles once and checks against that
    set; until the answer arrives, nothing is accepted.
