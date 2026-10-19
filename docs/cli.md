# `mdg` CLI

The CLI is the primary interface for AI agents and scripts. It never
prompts, writes errors to stderr, supports `--json` on every read command and
every edit, and returns stable exit codes.

```sh
cargo build --release
target/release/mdg --help            # or: cargo run -p mdg-cli -- --help
```

It is a drop-in replacement for the original TypeScript `mdg`: commands,
options, help text, messages, JSON output and exit codes are identical
(checked by `crates/mdg-cli/tests/golden.rs`, see
[development](development.md#cli-golden-tests)). Request/response ports and
module tags are additions the original never had.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | success (warnings never fail unless `validate --strict`) |
| 1 | validation errors, or an edit rejected by structural rules |
| 2 | usage error: bad arguments, malformed or ambiguous ID |
| 3 | file missing/unreadable, corrupted, or unsupported version |
| 4 | module, port or connection not found |

With `--json`, errors are printed to stderr as
`{"error": {"code": "...", "message": "...", "issues": [...]}}`.

## IDs

Modules, ports and connections are addressed by their stable IDs. A unique
prefix of an ID is also accepted, which helps with generated UUID IDs.
Endpoints are written `<module-id>:<port-id>`. List IDs with
`mdg tree <file> --ids`, `mdg inputs`, `mdg outputs` and `mdg connections`.

## Reading

| Command | Purpose |
|---|---|
| `mdg info <file>` | title, versions, root ID, module/port/connection counts, file size |
| `mdg tree <file> [--ids] [--depth N] [--json]` | hierarchy |
| `mdg describe <file> <module> [--depth N] [--json] [--output F]` | module documentation |
| `mdg inputs <file> <module> [--json]` | input contracts |
| `mdg outputs <file> <module> [--json]` | output contracts |
| `mdg connections <file> <module> [--json]` | the module's internal connections |
| `mdg trace <file> <module> <port> [--reverse] [--max-depth N] [--max-routes N] [--json]` | structural message routes |
| `mdg validate <file> [--json] [--strict] [--no-warnings]` | structural validation |
| `mdg export <file> --format markdown [--output F \| --split DIR]` | full documentation |
| `mdg tag list <file> [--json]` | the document's tags, colours and which modules carry them |

`describe --depth`:

- `0` (default): public contract only (identity, description, inputs,
  outputs);
- `1`: adds submodules and internal connections, plus each child's public
  contract;
- `N`: recurses N levels. Siblings and unrelated modules are never included.

`describe`, `inputs` and `outputs` read only the Markdown blocks they print.
`tree`, `info`, `connections` and `trace` read only the index.

`trace` follows connections across parent boundaries. Several routes are
reported when they exist. Cycles end a route safely and are flagged. Routes
describe possible message flow, **not** execution order.

## Editing

Every edit reads the file, applies one core operation, validates the whole
result and only then writes the file atomically. A rejected or invalid edit
leaves the file untouched. `--dry-run` reports what would change without
writing. `--json` returns `{"ok": true, "dryRun": false, ...}`.

```sh
mdg new arch.mdg --title "Shop" --root-id shop
mdg module add arch.mdg --parent shop --name Cart --id cart --description "Holds items."
mdg module rename arch.mdg cart "Shopping Cart"
mdg module move arch.mdg cart --to checkout
mdg module set-description arch.mdg cart --description-file cart.md   # or `-` for stdin
mdg module remove arch.mdg cart [--cascade]

mdg port add arch.mdg cart --direction output --name "Cart Ready" --id ready --description "Emits the final cart."
mdg port rename arch.mdg cart ready "Cart Finalized"
mdg port set-description arch.mdg cart ready --description "..."
mdg port remove arch.mdg cart ready [--cascade]

mdg connect arch.mdg cart:ready checkout:start [--id c1]
mdg disconnect arch.mdg c1
```

### Event and request/response ports

A port is an **event** port (the default) or a **request/response** port.
An event output emits a one-way message and an event input listens for it.
A request output *sends* a request and an input *handles* it. The single
connection runs from caller to handler, and the response travels back over
it, so one exchange needs one connection, not two. A request port has two
contracts: the request (its description) and the response.

```sh
mdg port add arch.mdg web --direction output --kind request --name "Get Cart" --id get-cart \
  --description "GET /cart with the session cookie." --response "The cart, or 404."
mdg port add arch.mdg cart --direction input --kind request --name "Get Cart" --id get-cart \
  --description "GET /cart." --response-file cart-response.md
mdg connect arch.mdg web:get-cart cart:get-cart
mdg port set-response arch.mdg cart get-cart --response "..."
mdg port set-kind arch.mdg cart ready request     # the port must not be connected
```

Both ends of a connection must be the same kind, and routing (`--route`)
creates boundary ports of the routed kind. A caller may connect to several
handlers, just as an event may reach several listeners. `inputs`, `outputs`,
`describe` and `export` show a request port's contracts under **Request**
and **Response** headings. In `--json` output, a request port has
`"kind": "request"` and a `response` field.

### Tags

Every document has a tag list, and any module can carry any of its tags
(for example to track status). A new document starts with four tags, which
can be renamed, recoloured or removed like any other:

| ID | Name | Colour |
|---|---|---|
| `done` | done | `#2a8a4a` (green) |
| `partially-done` | partially done | `#d4a017` (yellow) |
| `not-done` | not done | `#c43b3b` (red) |
| `to-implement` | to implement | `#3b6fd8` (blue) |

```sh
mdg tag list arch.mdg
mdg tag add arch.mdg --name "needs review" --color "#8e44ad" --id needs-review
mdg tag update arch.mdg to-implement --name planned --color "#5b8def"
mdg tag remove arch.mdg needs-review          # also untags every module

mdg module tag arch.mdg cart done needs-review
mdg module untag arch.mdg cart needs-review
```

Colours are `#rrggbb` and stored in lowercase. A module carries each tag at
most once. `tree` appends `#<tag-id>` to tagged modules, `describe` and
`export` add a `- Tags:` line to the module's identity, and their `--json`
output has a `tags` field. Untagged modules show nothing extra.

### Cross-level connections

Stored connections are always local. To connect ports at different depths,
pass `--route`. MDG then creates boundary ports and local connections at every
intermediate level:

```sh
mdg connect arch.mdg protected-endpoint:audit-event audit-log:record --route --dry-run
[dry run] Routed protected-endpoint:audit-event -> audit-log:record via application:
  + output user-routes:port-… "Audit Event"
  + conn-… in user-routes: protected-endpoint:audit-event -> user-routes:port-…
  + output api-service:port-… "Audit Event"
  + conn-… in api-service: user-routes:port-… -> api-service:port-…
  + conn-… in application: api-service:port-… -> audit-log:record
```

- `--via module:port` (repeatable) uses a specific existing boundary port at
  that level.
- An existing boundary port is reused automatically only when exactly one
  existing connection already carries the same source to it. `--no-reuse`
  turns this off. Ports are never matched by name.
- `--output-name` / `--input-name` name the generated ports.
