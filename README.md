# MDG — Markdown Graph

MDG is a local-first tool for documenting system architecture as **recursive
message-flow graphs**. Each module has a Markdown description and named
input and output ports with natural-language contracts. Modules contain child
modules, and directed connections document how messages may flow between
ports at the same hierarchy level. Messages cross levels only through
explicit boundary ports.

A whole document is one portable binary `.mdg` file. It has two interfaces:

- **`mdg` CLI**: for AI agents and scripts. Supports progressive disclosure,
  JSON everywhere, dry runs, validation and cross-level routing.
- **MDG Editor**: a desktop app (Tauri) for visual editing.

This is the Rust implementation. `.mdg` files are byte-compatible with the
original TypeScript implementation, and the CLI is a drop-in replacement
(same commands, help, messages, JSON and exit codes).

```sh
cargo build --release
target/release/mdg tree examples/authentication.mdg --ids
target/release/mdg describe examples/authentication.mdg api-service --depth 1
target/release/mdg trace examples/authentication.mdg protected-endpoint token-validation-request

cd editor && npm install && npm run tauri dev   # then Open… examples/authentication.mdg
```

| Path | Contents |
|---|---|
| `crates/mdg-core` | model, validation, operations, routing, trace, documentation, `.mdg` format |
| `crates/mdg-cli` | the `mdg` command-line tool |
| `crates/mdg-session` | editor state: undo history, layout, routing previews, files |
| `editor/` | the Tauri desktop editor (`src-tauri/` Rust, `src/` React UI) |

MDG is not an execution engine. Connections describe possible message flow;
ordering, conditions and error handling belong in the Markdown descriptions.

Documentation: [format](docs/format.md) · [architecture & decisions](docs/architecture.md) ·
[CLI](docs/cli.md) · [development](docs/development.md)
