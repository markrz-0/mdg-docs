# MDG — Markdown Graph (Rust)

A Rust reimplementation of MDG: a local-first tool for documenting system
architecture as **recursive message-flow graphs**.

- `crates/mdg-core` — model, validation, operations, routing, trace,
  documentation and the `.mdg` file format.
- `crates/mdg-cli` — the `mdg` command-line tool.

Files are byte-compatible with the original TypeScript implementation, and
the CLI is a drop-in replacement (same commands, output, JSON and exit codes).

```sh
cargo build --release
target/release/mdg tree examples/authentication.mdg --ids
target/release/mdg describe examples/authentication.mdg api-service --depth 1
cargo test
```
