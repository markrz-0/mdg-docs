# Development

Requirements: Rust (stable, 1.82 or newer) and, for the editor, Node.js 20
or newer with npm. On Windows, the editor needs the WebView2 runtime
(preinstalled on Windows 10/11).

```sh
cargo test                     # core, CLI (incl. golden corpus) and session
cargo clippy --all-targets
cargo build --release          # target/release/mdg(.exe)

cd editor
npm install
npm test                       # Vitest: graph projection and auto-layout
npm run typecheck
npm run tauri dev              # run the editor (Vite dev server + Rust)
npm run tauri build            # installer in target/release/bundle/nsis/
```

`cargo test` at the workspace root does not build the editor: the Tauri
crate (`editor/src-tauri`) is a workspace member but not a default member,
because it embeds the built UI (`editor/dist`). Build it with
`npm run tauri build`, or `npm run build` followed by
`cargo build -p mdg-editor`.

## Layout of tests

| Crate / package | Tests |
|---|---|
| mdg-core | `tests/model.rs` (operations, validation, transactions), `tests/serialization.rs` (round trips, UTF-8, integrity, schema messages, partial reads, atomic writes), `tests/routing.rs`, `tests/trace_docs.rs`, `tests/compat.rs` (byte-identical example file) |
| mdg-cli | `tests/golden.rs` (replays the TypeScript CLI corpus), `tests/cli.rs` (in-process, effects of edits) |
| mdg-session | `tests/session.rs` (undo, coalescing, dirtiness, layout, routing, files), `tests/fixture.rs` |
| editor | `tests/graph.test.ts` (graph projection, highlighting, auto-layout) |

The editor's frontend tests run against
`editor/tests/fixtures/authentication.view.json`, the session's view of the
example. `tests/fixture.rs` fails when that file no longer matches what the
session sends; regenerate it with
`UPDATE_FIXTURES=1 cargo test -p mdg-session --test fixture`.

## CLI golden tests

`crates/mdg-cli/tests/golden/cases.json` was recorded by running the
original TypeScript CLI in-process:

```sh
# in the TypeScript repository: npm install && npm run build
node scripts/gen-golden.mjs ../MDG
```

Each case sets up a fixture directory, runs one or more invocations, and
records exit codes, stdout, stderr and written files. Generated UUIDs and
the per-case directory are normalized. Add a case there when changing CLI
behaviour that must stay compatible.

## Conventions

- Graph edits are pure functions returning a new document, and errors are
  `MdgError`s with a stable code.
- Traversals are iterative.
- `mdg-core` has no UI or Tauri code; the editor UI has no model logic.
- Every change to the core rules needs a test, and
  `examples/authentication.mdg` must stay valid with no warnings and
  byte-identical with `create_authentication_example()`.
