//! Deterministic `.mdg` serializer.
//!
//! Stable ordering:
//! - modules (in the index and as content blocks) sorted by module ID, using
//!   UTF-16 code-unit order;
//! - per module, content blocks are: description, inputs in stored order,
//!   outputs in stored order;
//! - ports, child IDs and connections keep their stored order;
//! - layout positions, unknown metadata and extension keys sorted by key.
//!
//! No timestamps or random values are written, so an unchanged document
//! always serializes to identical bytes. The index JSON is written exactly
//! like `JSON.stringify`, so files are byte-identical with the original
//! TypeScript implementation.

use std::collections::HashSet;

use serde_json::{json, Map, Value};

use super::decode::schema_major;
use super::format::{
    module_content_id, port_content_id, CONTAINER_MAJOR, CONTAINER_MINOR, CONTENT_ENCODING, HEADER_SIZE, MAGIC_BYTES,
};
use super::index::KNOWN_INDEX_KEYS;
use crate::error::{MdgError, Result};
use crate::js::{self, compare_utf16};
use crate::jsmap::JsMap;
use crate::model::{Document, Port, Position, SCHEMA_MAJOR};
use crate::ops::assert_valid;

#[derive(Debug, Clone, Copy)]
pub struct EncodeOptions {
    /// Validate before encoding (default true). Invalid documents are rejected.
    pub validate: bool,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        EncodeOptions { validate: true }
    }
}

fn sorted_keys<V>(map: &JsMap<V>) -> Vec<&str> {
    let mut keys = map.keys();
    keys.sort_by(|a, b| compare_utf16(a, b));
    keys
}

/// Recursively sorts object keys so arbitrary JSON values serialize deterministically.
fn canonicalize(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(canonicalize).collect()),
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|a, b| compare_utf16(a, b));
            Value::Object(keys.into_iter().map(|k| (k.clone(), canonicalize(&map[k]))).collect())
        }
        other => other.clone(),
    }
}

fn position(p: &Position) -> Value {
    json!({ "x": p.x, "y": p.y })
}

struct Block {
    reference: String,
    bytes: Vec<u8>,
}

/// Serializes a document into `.mdg` bytes.
pub fn encode_mdg(doc: &Document, options: EncodeOptions) -> Result<Vec<u8>> {
    if schema_major(&doc.format_version) != Some(SCHEMA_MAJOR) {
        return Err(MdgError::unsupported_version(format!(
            "Cannot write schema version \"{}\"; this implementation supports {SCHEMA_MAJOR}.x.y.",
            doc.format_version
        )));
    }
    if options.validate {
        assert_valid(doc)?;
    }

    let mut blocks: Vec<Block> = Vec::new();
    let mut refs: HashSet<String> = HashSet::new();
    let mut add_block = |reference: String, text: &str| -> Result<String> {
        if !refs.insert(reference.clone()) {
            return Err(MdgError::format(format!("Duplicate content block \"{reference}\".")));
        }
        blocks.push(Block { reference: reference.clone(), bytes: text.as_bytes().to_vec() });
        Ok(reference)
    };

    let mut modules = Map::new();
    for id in sorted_keys(&doc.modules) {
        let m = &doc.modules.get(id).expect("key exists");
        let description_ref = add_block(module_content_id(id), &m.description)?;
        let mut port = |p: &Port| -> Result<Value> {
            Ok(json!({
                "id": p.id,
                "name": p.name,
                "descriptionRef": add_block(port_content_id(id, &p.id), &p.description)?,
            }))
        };
        let inputs = m.inputs.iter().map(&mut port).collect::<Result<Vec<_>>>()?;
        let outputs = m.outputs.iter().map(&mut port).collect::<Result<Vec<_>>>()?;
        let connections: Vec<Value> = m
            .connections
            .iter()
            .map(|c| {
                json!({
                    "id": c.id,
                    "from": { "moduleId": c.from.module_id, "portId": c.from.port_id },
                    "to": { "moduleId": c.to.module_id, "portId": c.to.port_id },
                })
            })
            .collect();
        modules.insert(
            id.to_string(),
            json!({
                "id": m.id,
                "parentId": m.parent_id,
                "name": m.name,
                "descriptionRef": description_ref,
                "inputs": inputs,
                "outputs": outputs,
                "childIds": m.child_ids,
                "connections": connections,
            }),
        );
    }

    let mut metadata = Map::new();
    metadata.insert("title".into(), Value::String(doc.metadata.title.clone()));
    if let Some(description) = &doc.metadata.description {
        metadata.insert("description".into(), Value::String(description.clone()));
    }
    for key in sorted_keys(&doc.metadata.extra) {
        if key == "title" || key == "description" {
            continue;
        }
        metadata.insert(key.to_string(), canonicalize(doc.metadata.extra.get(key).expect("key exists")));
    }

    let layout = doc.layout.as_ref().map(|layout| {
        let positions: Map<String, Value> = sorted_keys(&layout.positions)
            .into_iter()
            .map(|id| (id.to_string(), position(layout.positions.get(id).expect("key exists"))))
            .collect();
        let mut out = Map::new();
        out.insert("positions".into(), Value::Object(positions));
        if let Some(boundaries) = layout.boundaries.as_ref().filter(|b| !b.is_empty()) {
            let entries: Map<String, Value> = sorted_keys(boundaries)
                .into_iter()
                .map(|id| {
                    let b = boundaries.get(id).expect("key exists");
                    let mut entry = Map::new();
                    if let Some(input) = &b.input {
                        entry.insert("input".into(), position(input));
                    }
                    if let Some(output) = &b.output {
                        entry.insert("output".into(), position(output));
                    }
                    (id.to_string(), Value::Object(entry))
                })
                .collect();
            out.insert("boundaries".into(), Value::Object(entries));
        }
        Value::Object(out)
    });

    let extensions: Vec<(String, Value)> = match &doc.extensions {
        None => vec![],
        Some(ext) => sorted_keys(ext)
            .into_iter()
            .filter(|k| !KNOWN_INDEX_KEYS.contains(k))
            .map(|k| (k.to_string(), canonicalize(ext.get(k).expect("key exists"))))
            .collect(),
    };

    let mut relative_offsets = Vec::with_capacity(blocks.len());
    let mut total: u64 = 0;
    for block in &blocks {
        relative_offsets.push(total);
        total += block.bytes.len() as u64;
    }

    let modules = Value::Object(modules);
    let metadata = Value::Object(metadata);
    let build_index = |content_base: u64| -> Vec<u8> {
        let content: Map<String, Value> = blocks
            .iter()
            .zip(&relative_offsets)
            .map(|(block, offset)| {
                (
                    block.reference.clone(),
                    json!({ "offset": content_base + offset, "length": block.bytes.len(), "encoding": CONTENT_ENCODING }),
                )
            })
            .collect();
        let mut index = Map::new();
        index.insert("schemaVersion".into(), Value::String(doc.format_version.clone()));
        index.insert("rootId".into(), Value::String(doc.root_id.clone()));
        index.insert("metadata".into(), metadata.clone());
        index.insert("modules".into(), modules.clone());
        index.insert("content".into(), Value::Object(content));
        if let Some(layout) = &layout {
            index.insert("layout".into(), layout.clone());
        }
        for (key, value) in &extensions {
            index.insert(key.clone(), value.clone());
        }
        js::stringify(&Value::Object(index)).into_bytes()
    };

    // Offsets depend on the index length and vice versa. The index length is
    // non-decreasing in the offset values, so this fixed-point loop converges.
    let mut index_bytes = build_index(HEADER_SIZE);
    for _ in 0..32 {
        let next = build_index(HEADER_SIZE + index_bytes.len() as u64);
        let stable = next.len() == index_bytes.len();
        index_bytes = next;
        if stable {
            break;
        }
    }
    let content_base = HEADER_SIZE + index_bytes.len() as u64;
    if build_index(content_base).len() != index_bytes.len() {
        return Err(MdgError::format("Failed to compute stable index offsets."));
    }

    let mut output = Vec::with_capacity((content_base + total) as usize);
    output.extend_from_slice(&MAGIC_BYTES);
    output.extend_from_slice(&CONTAINER_MAJOR.to_le_bytes());
    output.extend_from_slice(&CONTAINER_MINOR.to_le_bytes());
    output.extend_from_slice(&HEADER_SIZE.to_le_bytes());
    output.extend_from_slice(&(index_bytes.len() as u64).to_le_bytes());
    output.extend_from_slice(&0u64.to_le_bytes());
    output.extend_from_slice(&index_bytes);
    for block in &blocks {
        output.extend_from_slice(&block.bytes);
    }
    Ok(output)
}
