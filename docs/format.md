# The `.mdg` file format

An `.mdg` file is a single binary container holding one MDG document: a
fixed-size header, a UTF-8 JSON index describing the graph structure, and
UTF-8 Markdown content blocks holding every description.

```
[32-byte header][JSON index][Markdown block][Markdown block]...
```

Two versions are tracked independently:

| Version | Where | Controls | Current |
|---|---|---|---|
| Container version | header bytes 4–7 | binary layout | `1.0` |
| Schema version | index `schemaVersion` | data model | `1.0.0` |

Readers reject an unknown **container major** and an unknown **schema major**
with an explicit "unsupported version" error. Nothing is reinterpreted
silently.

## Header (32 bytes)

All integers are **little-endian**. Offsets and lengths are unsigned 64-bit
integers. Values above `2^53 − 1` (JavaScript's safe-integer limit) are
rejected, so files stay readable by JavaScript implementations.

| Bytes | Type | Content |
|---|---|---|
| 0–3 | ASCII | magic `MDG1` |
| 4–5 | uint16 | container major version (`1`) |
| 6–7 | uint16 | container minor version (`0`) |
| 8–15 | uint64 | index offset, absolute from file start |
| 16–23 | uint64 | index length in bytes |
| 24–31 | — | reserved. Must be zero in container 1.0 |

## Index (UTF-8 JSON)

```json
{
  "schemaVersion": "1.0.0",
  "rootId": "application",
  "metadata": { "title": "Authentication Example", "description": "..." },
  "modules": {
    "api-service": {
      "id": "api-service",
      "parentId": "application",
      "name": "API Service",
      "descriptionRef": "module/api-service",
      "inputs":  [{ "id": "login-request", "name": "Login Request", "descriptionRef": "port/api-service/login-request" }],
      "outputs": [],
      "childIds": ["authentication", "user-routes"],
      "connections": [
        { "id": "api-login",
          "from": { "moduleId": "api-service", "portId": "login-request" },
          "to":   { "moduleId": "authentication", "portId": "login-request" } }
      ]
    }
  },
  "content": {
    "module/api-service": { "offset": 5120, "length": 231, "encoding": "utf-8" }
  },
  "layout": {
    "positions": { "api-service": { "x": 380, "y": 40 } },
    "boundaries": { "api-service": { "input": { "x": -400, "y": 0 }, "output": { "x": 900, "y": 0 } } }
  }
}
```

- `modules` is a flat map keyed by module ID. The hierarchy is in
  `parentId` / `childIds`.
- Connections are stored on the module whose internal graph contains them.
  Each endpoint references that module itself (a boundary port) or one of its
  direct children. Port direction is never stored: it comes from whether the
  port is listed in `inputs` or `outputs`.
- `content` maps content IDs to absolute byte ranges. Content IDs are
  `module/<moduleId>` and `port/<moduleId>/<portId>`. They are unambiguous
  because IDs cannot contain `/` or `:`.
- `layout` is optional presentation metadata with no effect on message-flow
  semantics; validation ignores it. `positions` holds each module's position
  in its parent's canvas. The optional `boundaries` holds, per module, the
  positions of its own input and output boundary panels inside its internal
  graph. Both `input` and `output` are optional; a missing one is computed by
  the editor.

### Unknown data

- Unknown keys at the top level of the index and inside `metadata` are
  preserved verbatim on read/write.
- Unknown fields inside module, port, connection, content and layout records
  are rejected as malformed: they cannot be represented in the model and
  would otherwise be dropped silently.

## Content blocks

Raw UTF-8 Markdown, one block per module description and one per port
description, including empty descriptions (zero-length blocks). Lengths are
measured in UTF-8 bytes. Graph structure is never stored in Markdown.

## Deterministic serialization

Writing an unchanged document always produces identical bytes:

- modules (index order and block order) are sorted by module ID in
  UTF-16 code-unit order;
- each module's blocks are ordered: description, inputs (stored order),
  outputs (stored order);
- ports, `childIds` and connections keep their stored order;
- layout positions, unknown metadata and extension keys are sorted by key;
  nested extension objects are written with sorted keys;
- no timestamps or random values are written.

The index is written exactly like JavaScript's `JSON.stringify`: no
whitespace, numbers in JavaScript's shortest form (`380`, not `380.0`), and
object keys in JavaScript enumeration order (keys that are canonical
integers, such as a module ID `10`, come first in ascending order). Files
written by this implementation and by the original TypeScript one are
byte-identical.

Offsets depend on the index length, which in turn depends on the offset
digits. The writer iterates to a fixed point; the index length is
non-decreasing in the offsets, so this always converges.

## Integrity checks on read

The reader rejects, with a `FORMAT_ERROR` or `UNSUPPORTED_VERSION`:

- files shorter than the header, or wrong magic bytes;
- unknown container major, or unknown schema major;
- non-zero reserved bytes (container 1.0);
- an index range that overlaps the header, is empty, or lies outside the file;
- invalid UTF-8 or invalid JSON in the index, or an index that does not match
  the schema;
- content ranges outside the file, inside the header or index, or overlapping
  each other;
- references to missing content blocks;
- invalid UTF-8 in any content block that is read.

All ranges are validated against the real file size **before** any buffer is
allocated, so a malformed header cannot trigger an oversized allocation.

Structural problems (a broken hierarchy, invalid connections) are **not**
format errors. Such a file can still be read so that `mdg validate` can
report them. Writers refuse to save structurally invalid documents.

## Partial reads

The header and index are enough to answer any structural question. The
reader can load the Markdown of a chosen set of modules only
(`read_document(source, Some(&module_ids))`). The CLI's `describe`, `inputs`
and `outputs` commands read just the blocks they print.

## Atomic writes

The writer (`write_mdg`, used by the CLI and the editor) validates and
encodes in memory, writes a temporary file in the destination directory,
flushes it to disk, then renames it over the destination. On failure the
temporary file is removed and the original is untouched.
