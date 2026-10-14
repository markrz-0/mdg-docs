# MDG — Markdown Graph (Rust)

A Rust reimplementation of MDG: a local-first tool for documenting system
architecture as **recursive message-flow graphs**.

- `crates/mdg-core` — model, validation, operations, routing, trace,
  documentation and the `.mdg` file format.

Files are byte-compatible with the original TypeScript implementation.

```sh
cargo test
```
