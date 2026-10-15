---
name: mdg-docs
description: Read, navigate, create, and update software architecture documentation stored in `.mdg` Markdown Graph files with the `mdg` CLI. Use when a project contains `.mdg` files or identifies MDG as its architecture/documentation system; do not use for ordinary Markdown files that are unrelated to MDG.
---

# MDG Documentation

Use MDG as the project's structured architecture documentation. Read it before making claims about documented components or message contracts. Update it when documentation changes are requested or repository instructions require architecture documentation to stay synchronized with implementation work.

## Understand the model

An `.mdg` file is a portable binary document, not a Markdown file to edit directly. It contains:

- recursive modules, each with a Markdown description;
- named input and output ports with Markdown contracts;
- directed connections describing possible message flow;
- a document-level list of colored tags that modules can carry (e.g. a status);
- optional visual layout metadata.

Every port has a kind:

- **event** (the default): a one-way message. An output emits it; an input listens for it. It has one contract (its description).
- **request** (request/response): an exchange where the caller expects an answer. An output *sends* the request (the caller); an input *handles* it (the handler). A single connection runs caller → handler and the response travels back over it implicitly. A request port has two contracts: the request (its description) and the response.

Both ends of a connection must be the same kind. A caller may connect to several handlers, just as an event may reach several listeners.

A module's children form its internal graph. Connections at that level may reference the module's own boundary ports or its direct children. Messages cross hierarchy levels through explicit boundary ports.

Tags are labels, typically implementation status. Every document starts with four, which users may rename, recolor, or remove, and may extend with their own:

| ID | Meaning | Color |
|---|---|---|
| `done` | done | green |
| `partially-done` | partially done | yellow |
| `not-done` | not done | red |
| `to-implement` | to implement | blue |

A module may carry any number of tags, each at most once. Tags have no effect on message flow or validity beyond referring to tags that exist. Always run `mdg tag list <file>` before relying on a tag: the list is per document, so names, colors, and the set of tags may differ from the defaults.

Treat module and port descriptions as the place for behavior, responsibilities, invariants, ordering, conditions, failures, protocols, and examples. Treat connections only as possible flow: they do not express execution order, branching conditions, or runtime behavior. Layout has no semantic meaning.

Always use the `mdg` CLI to read or change `.mdg` files. Do not inspect, patch, or generate their binary contents with generic text or binary-editing tools.

## Find the relevant document

Confirm the CLI is available with `mdg --help`, then locate `.mdg` files using the environment's normal file search. Respect project instructions that identify a canonical document. If several documents are plausible, inspect each with `mdg info <file>` and select based on its title and scope; ask the user only when the choice remains materially ambiguous.

If `mdg` is unavailable, report that clearly. Do not install software unless installation is within the user's request or separately authorized.

## Read progressively

Start with structure and retrieve detailed Markdown only for the relevant area:

```text
mdg info <file>
mdg validate <file>
mdg tree <file> --ids
mdg describe <file> <module-id> --depth 1
```

Then narrow or expand as the task requires:

```text
mdg describe <file> <module-id> --depth 0
mdg describe <file> <module-id> --depth <N>
mdg inputs <file> <module-id>
mdg outputs <file> <module-id>
mdg connections <file> <module-id>
mdg trace <file> <module-id> <port-id>
mdg trace <file> <module-id> <port-id> --reverse
mdg tag list <file>
```

Use `--json` when results will be parsed or compared programmatically. Use `mdg export <file> --format markdown` only when the task genuinely needs the whole document; prefer `tree` and focused `describe` calls for normal exploration.

`describe --depth 0` returns a module's public contract. Depth 1 also includes its immediate internal structure and each child's public contract. Higher depths recurse only through that module's subtree.

IDs are stable identifiers and are the correct way to address modules, ports, and connections. Commands accept an unambiguous ID prefix, but use the complete ID for edits when practical. Port endpoints use `<module-id>:<port-id>`.

`inputs`, `outputs`, `describe` and `export` mark request ports with a `Kind:` line (`request/response, handled here` for inputs, `sent from here` for outputs) and show their contracts under **Request** and **Response** headings. In `--json` output a request port has `"kind": "request"` and a `response` field; event ports have neither.

`tree` appends `#<tag-id>` to each tagged module, `describe` and `export` add a `- Tags:` line to its identity, and their `--json` output has a `tags` field (omitted for untagged modules). `mdg tag list --json` lists every tag with its color and the IDs of the modules carrying it, which is the quickest way to answer questions such as "what is still to implement?".

`trace` follows connections in their stored direction. For a request port that is caller → handler; the response returns along the same route in reverse, so do not look for separate response connections.

`trace` may return multiple routes and marks cycles or limits. Interpret these as structurally possible routes, never as proof of temporal order or runtime execution.

## Write safely

Before editing, validate the document and inspect the affected module, its ports, and its local connections. Base changes on the requested or observed architecture; do not invent undocumented components or contracts merely to make the graph look complete.

Use the dedicated operations:

```text
mdg new <file> --title <title> --root-id <root-id>

mdg module add <file> --parent <parent-id> --name <name> --id <id> --description <text>
mdg module rename <file> <module-id> <new-name>
mdg module move <file> <module-id> --to <new-parent-id>
mdg module set-description <file> <module-id> --description <text>
mdg module set-description <file> <module-id> --description-file <markdown-file-or-dash>
mdg module remove <file> <module-id> [--cascade]

mdg port add <file> <module-id> --direction <input|output> --name <name> --id <id> --description <text>
mdg port add <file> <module-id> --direction <input|output> --kind request --name <name> --id <id> --description <request-text> --response <response-text>
mdg port rename <file> <module-id> <port-id> <new-name>
mdg port set-description <file> <module-id> <port-id> --description <text>
mdg port set-description <file> <module-id> <port-id> --description-file <markdown-file-or-dash>
mdg port set-response <file> <module-id> <port-id> --response <text>
mdg port set-response <file> <module-id> <port-id> --response-file <markdown-file-or-dash>
mdg port set-kind <file> <module-id> <port-id> <event|request>
mdg port remove <file> <module-id> <port-id> [--cascade]

mdg connect <file> <from-module>:<from-port> <to-module>:<to-port> [--id <connection-id>]
mdg disconnect <file> <connection-id>

mdg module tag <file> <module-id> <tag-id>...
mdg module untag <file> <module-id> <tag-id>...
mdg tag add <file> --name <name> --color <#rrggbb> --id <tag-id>
mdg tag update <file> <tag-id> [--name <name>] [--color <#rrggbb>]
mdg tag remove <file> <tag-id>
```

Use `--description-file` (and `--response-file`) for substantial Markdown so shell quoting does not damage formatting. A value of `-` reads the text from standard input.

Choose the port kind from the interaction, not from habit:

- Use a **request** port pair when a caller waits for an answer (an HTTP call, an RPC, a query, a validation). Do not model it as an output event plus a separate "result" input event with two connections.
- Use **event** ports for fire-and-forget notifications, streams, and messages nobody waits for.
- `--response` / `--response-file` are only valid with `--kind request`; `set-response` fails on event ports.
- `set-kind` refuses a connected port (both ends must stay the same kind): disconnect, change the kind on both ends, then reconnect. Switching to `event` discards the response contract.

Documents with request ports are written as schema `1.1.0`; event-only documents stay `1.0.0`. Documents that tag a module or change the default tag list are written as schema `1.2.0`.

Tag modules to reflect the truth, not intent:

- When you add or substantially change a documented module, keep its status tag accurate if the document uses status tags (e.g. `to-implement` for a designed but unbuilt module, `partially-done` when only part of its documented contracts is implemented, `not-done` when its implementation is missing or does not work, `done` when the implementation matches the documentation). Remove a status tag that no longer holds, so a module never carries contradictory statuses such as `done` and `not-done`.
- Only tag a module `done` when you have verified that the implementation exists and matches its documented contracts.
- Do not add, rename, recolor, or remove document tags (`mdg tag add/update/remove`) unless the user asks; the tag list is a project convention. `tag remove` also untags every module carrying it.

Prefer meaningful, stable, lowercase kebab-case IDs when creating them, unless the project already uses another convention. Names are human-facing and may change; IDs should remain stable across renames.

For a connection whose endpoints are at different hierarchy levels, use routing rather than manually guessing boundary ports:

```text
mdg connect <file> <source-module>:<output-port> <target-module>:<input-port> --route --dry-run
```

For request ports the source is the caller and the target is the handler; both must be the same kind. Review the plan, then repeat without `--dry-run` to apply it. MDG creates the required boundary ports (of the routed kind, with generated request and response contracts) and local connections. Use `--via <module>:<port>` only when an existing boundary port is intentionally part of the contract. Ports are not matched by name.

Use `--dry-run` before structural, routed, cascading, or otherwise broad edits. Removal is deliberately safe by default: only use `--cascade` when removing descendants or referencing connections is explicitly intended and the impact has been inspected.

## Specify the payload schema of every contract

Every port contract MUST state the exact shape of what travels through it, in the format the system actually uses. This applies to both kinds:

- **event port:** the schema of the event message (its description);
- **request port:** the schema of the request (its description) **and** the schema of the response (its response contract). Both are required; a response contract that only says "the result" is incomplete.

Each schema must be specific enough that someone could implement either side from it alone:

- **Format and transport framing:** name it explicitly, e.g. JSON body, `application/x-www-form-urlencoded`, HTML page, Protobuf message, CSV row, plain text, binary blob, CLI arguments. For HTTP include method, path, path/query parameters, and relevant headers; for queues/topics include the topic or channel name.
- **Every field:** name, type, **required or optional**, allowed values or format (enum, pattern, units, ranges, encoding such as ISO 8601 or UUID), nullability, and default when omitted. Nest structures fully; do not write "user object" without defining it.
- **Every outcome (responses):** each success and failure variant with its own shape and how the caller tells them apart (HTTP status, `type` discriminator, error code list).
- **Constraints that matter on the wire:** size limits, ordering of collections, idempotency keys, versioning.

Prefer the project's real definitions: if a JSON Schema, OpenAPI spec, `.proto`, TypeScript type, or DTO class exists, reproduce the relevant part (or a faithful excerpt) and name its source path. Otherwise write a schema block yourself. Use a fenced block whose language matches the format (`json`, `jsonc` with comments marking optional fields, `typescript`, `proto`, `html`, …), followed by a field table when a block alone cannot express required/optional or constraints:

````markdown
`POST /login`, `Content-Type: application/json`

```jsonc
{
  "username": "string",   // required, 3–64 chars, case-insensitive
  "password": "string",   // required, 8–128 chars
  "remember": true        // optional boolean, default false
}
```
````

and the matching response contract:

````markdown
`200 OK`, `application/json`:

```jsonc
{
  "token": "string",      // required, signed JWT (RS256)
  "expiresAt": "string"   // required, ISO 8601 UTC timestamp
}
```

`401 Unauthorized`, `application/json`:

```jsonc
{ "error": "invalid_credentials" | "account_locked" }   // required
```
````

For non-JSON payloads be equally exact, e.g. an HTML response names the document type, the elements and attributes consumers depend on, and the encoding; a plain-text event names the line format and the encoding.

If the schema genuinely cannot be determined from the code, specs, or the user, do not invent fields. Write what is known, and add an explicit `**Schema: TODO** — <what is missing and where to find it>` line. Then tell the user which contracts are incomplete. When editing an existing port whose contract lacks a schema, add one if the information is available.

Because schemas are long, write them with `--description-file` / `--response-file` rather than inline flags.

## Verify every change

After edits:

1. Run `mdg validate <file>`; use `--strict` when project policy treats warnings as failures.
2. Re-read the affected module with `mdg describe`, plus `inputs`, `outputs`, or `connections` as relevant.
3. Run `mdg trace` for changed routes in both directions when end-to-end connectivity matters.
   If you changed tags, confirm them with `mdg tag list <file>` or `mdg tree <file> --ids`.
4. Re-read every port you added or changed (`mdg inputs` / `mdg outputs`) and confirm each event contract, and each request *and* response contract, has a complete schema or an explicit `Schema: TODO`.
5. Review the repository diff or changed artifact status so only the intended document changed.

MDG edits are validated before being written and are saved atomically, but this does not replace semantic review. A structurally valid document can still describe the wrong system.

## Keep documentation useful

- Describe a module's responsibility and boundaries, not its source code line by line.
- Make each port contract state what the message represents, its full payload schema (see above), and any important guarantees or failure meaning. For request ports, put what the caller sends in the request contract and every possible answer (including errors) in the response contract.
- Keep the schemas on both ends of a connection consistent: a caller's request and response contracts must match the handler's, and an emitted event must match what its listeners expect. When they intentionally differ (e.g. a boundary port that adds or strips fields), say so.
- Put ordering and conditional behavior in Markdown descriptions rather than inferring it from graph layout.
- Reuse existing concepts and terminology from the document and codebase.
- Preserve unrelated descriptions, IDs, extensions, and layout.
- Prefer the smallest edit that makes the documented architecture accurate.

For exact flags supported by the installed version, use `mdg <command> --help` or `mdg <command> <subcommand> --help` rather than guessing.
