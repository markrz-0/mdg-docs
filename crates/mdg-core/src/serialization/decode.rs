use std::collections::HashSet;
use std::sync::Arc;

use serde_json::Value;

use super::format::{CONTAINER_MAJOR, HEADER_SIZE, MAGIC, MAGIC_BYTES, MAX_SAFE_INTEGER};
use super::index::{parse_index, Index};
use super::source::{ByteSource, BytesSource};
use crate::error::{MdgError, Result};
use crate::model::{default_tags, Document, Module, Port, SCHEMA_MAJOR};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub container_major: u16,
    pub container_minor: u16,
    pub index_offset: u64,
    pub index_length: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexRead {
    pub header: Header,
    pub index: Index,
    /// Total file size in bytes.
    pub size: u64,
}

/// Major version of a `MAJOR.MINOR.PATCH` version string.
pub fn schema_major(version: &str) -> Option<f64> {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() != 3 || parts.iter().any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit())) {
        return None;
    }
    parts[0].parse::<f64>().ok()
}

fn read_u64(bytes: &[u8], offset: usize, field: &str) -> Result<u64> {
    let value = u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("8 bytes"));
    if value > MAX_SAFE_INTEGER {
        return Err(MdgError::format(format!("Header {field} ({value}) exceeds the safely representable range.")));
    }
    Ok(value)
}

fn decode_utf8(bytes: Vec<u8>, what: &str) -> Result<String> {
    String::from_utf8(bytes).map_err(|_| MdgError::format(format!("{what} is not valid UTF-8.")))
}

/// Reads and validates the fixed-size header.
pub fn read_header(source: &mut dyn ByteSource) -> Result<Header> {
    let size = source.size();
    if size < HEADER_SIZE {
        return Err(MdgError::format(format!(
            "File is truncated: {size} bytes is smaller than the {HEADER_SIZE}-byte MDG header."
        )));
    }
    let bytes = source.read(0, HEADER_SIZE)?;
    if bytes[..4] != MAGIC_BYTES {
        return Err(MdgError::format(format!("Not an MDG file: missing \"{MAGIC}\" magic bytes.")));
    }
    let container_major = u16::from_le_bytes([bytes[4], bytes[5]]);
    let container_minor = u16::from_le_bytes([bytes[6], bytes[7]]);
    if container_major != CONTAINER_MAJOR {
        return Err(MdgError::unsupported_version(format!(
            "Unsupported MDG container version {container_major}.{container_minor}; this implementation reads container version {CONTAINER_MAJOR}.x."
        )));
    }
    let index_offset = read_u64(&bytes, 8, "index offset")?;
    let index_length = read_u64(&bytes, 16, "index length")?;
    if container_minor == 0 && bytes[24..32].iter().any(|&b| b != 0) {
        return Err(MdgError::format("Reserved header bytes must be zero in container 1.0."));
    }
    if index_offset < HEADER_SIZE {
        return Err(MdgError::format(format!("Invalid index offset {index_offset}: it overlaps the header.")));
    }
    if index_length == 0 {
        return Err(MdgError::format("Index length is zero."));
    }
    if index_offset > size || index_length > size - index_offset {
        return Err(MdgError::format(format!(
            "Index range {index_offset}+{index_length} lies outside the file ({size} bytes). The file may be truncated."
        )));
    }
    Ok(Header { container_major, container_minor, index_offset, index_length })
}

fn assert_safe_keys(value: Option<&Value>, place: &str) -> Result<()> {
    if let Some(Value::Object(map)) = value {
        if map.contains_key("__proto__") {
            return Err(MdgError::format(format!("Forbidden key \"__proto__\" in {place}.")));
        }
    }
    Ok(())
}

fn check_content_ranges(index: &Index, header: &Header, size: u64) -> Result<()> {
    let index_end = header.index_offset + header.index_length;
    let ranges: Vec<(&str, u64, u64)> = index.content.iter().map(|(r, e)| (r, e.offset, e.length)).collect();
    for &(r, offset, length) in &ranges {
        if offset < HEADER_SIZE || offset > size || length > size - offset {
            return Err(MdgError::format(format!(
                "Content block \"{r}\" ({offset}+{length}) lies outside the file or inside the header."
            )));
        }
        if length > 0 && offset < index_end && offset + length > header.index_offset {
            return Err(MdgError::format(format!("Content block \"{r}\" overlaps the index.")));
        }
    }
    let mut sorted: Vec<&(&str, u64, u64)> = ranges.iter().filter(|r| r.2 > 0).collect();
    sorted.sort_by_key(|r| r.1); // stable, like Array.prototype.sort
    for pair in sorted.windows(2) {
        let (prev, cur) = (pair[0], pair[1]);
        if prev.1 + prev.2 > cur.1 {
            return Err(MdgError::format(format!("Content blocks \"{}\" and \"{}\" overlap.", prev.0, cur.0)));
        }
    }
    for (id, m) in index.modules.iter() {
        let require = |r: &str, owner: String| -> Result<()> {
            if index.content.contains_key(r) {
                Ok(())
            } else {
                Err(MdgError::format(format!("{owner} references missing content block \"{r}\".")))
            }
        };
        require(&m.description_ref, format!("Module \"{id}\""))?;
        for p in m.inputs.iter().chain(m.outputs.iter()) {
            require(&p.description_ref, format!("Port \"{id}:{}\"", p.id))?;
            if let Some(response_ref) = &p.response_ref {
                require(response_ref, format!("Port \"{id}:{}\"", p.id))?;
            }
        }
    }
    Ok(())
}

/// Reads and validates the header and JSON index without reading any
/// Markdown content blocks.
pub fn read_index(source: &mut dyn ByteSource) -> Result<IndexRead> {
    let header = read_header(source)?;
    let text = decode_utf8(source.read(header.index_offset, header.index_length)?, "Index")?;
    let raw: Value =
        serde_json::from_str(&text).map_err(|e| MdgError::format(format!("Index is not valid JSON: {e}")))?;
    let Value::Object(record) = raw else {
        return Err(MdgError::format("Index must be a JSON object."));
    };
    let Some(Value::String(schema_version)) = record.get("schemaVersion") else {
        return Err(MdgError::format("Index is missing a string \"schemaVersion\"."));
    };
    let Some(major) = schema_major(schema_version) else {
        return Err(MdgError::format(format!("Malformed schema version \"{schema_version}\".")));
    };
    if major != SCHEMA_MAJOR {
        return Err(MdgError::unsupported_version(format!(
            "Unsupported MDG schema version {schema_version}; this implementation reads {SCHEMA_MAJOR}.x.y."
        )));
    }
    if record.contains_key("__proto__") {
        return Err(MdgError::format("Forbidden key \"__proto__\" in index."));
    }
    assert_safe_keys(record.get("modules"), "modules")?;
    assert_safe_keys(record.get("content"), "content")?;
    assert_safe_keys(record.get("metadata"), "metadata")?;
    if let Some(Value::Array(tags)) = record.get("tags") {
        for tag in tags {
            assert_safe_keys(Some(tag), "tags")?;
        }
    }
    if let Some(Value::Object(layout)) = record.get("layout") {
        assert_safe_keys(layout.get("positions"), "layout")?;
        assert_safe_keys(layout.get("boundaries"), "layout")?;
    }
    let index = parse_index(&record).map_err(|details| MdgError::format(format!("Malformed index: {details}")))?;
    let size = source.size();
    check_content_ranges(&index, &header, size)?;
    Ok(IndexRead { header, index, size })
}

/// Reads one content block as text. The range was validated by [`read_index`].
pub fn read_content(source: &mut dyn ByteSource, index: &Index, r: &str) -> Result<String> {
    let entry = index.content.get(r).ok_or_else(|| MdgError::format(format!("Missing content block \"{r}\".")))?;
    decode_utf8(source.read(entry.offset, entry.length)?, &format!("Content block \"{r}\""))
}

/// Converts an index into a document. `load_text(module_id, ref)` supplies
/// Markdown for a module's description and ports; return `""` to skip loading.
pub fn index_to_document(index: &Index, mut load_text: impl FnMut(&str, &str) -> Result<String>) -> Result<Document> {
    let mut modules = crate::jsmap::JsMap::new();
    for (key, m) in index.modules.iter() {
        // Same load order as the original: description, inputs, outputs
        // (each port: description, then response).
        let description = load_text(key, &m.description_ref)?;
        let mut port = |p: &super::index::IndexPort| -> Result<Port> {
            let description = load_text(key, &p.description_ref)?;
            let response = match &p.response_ref {
                Some(r) => load_text(key, r)?,
                None => String::new(),
            };
            Ok(Port { id: p.id.clone(), name: p.name.clone(), kind: p.kind, description, response })
        };
        let inputs = m.inputs.iter().map(&mut port).collect::<Result<Vec<_>>>()?;
        let outputs = m.outputs.iter().map(&mut port).collect::<Result<Vec<_>>>()?;
        let module = Module {
            id: m.id.clone(),
            parent_id: m.parent_id.clone(),
            name: m.name.clone(),
            description,
            inputs,
            outputs,
            child_ids: m.child_ids.clone(),
            connections: m.connections.clone(),
            tags: m.tags.clone(),
        };
        modules.insert(key, Arc::new(module));
    }
    Ok(Document {
        format_version: index.schema_version.clone(),
        root_id: index.root_id.clone(),
        modules,
        metadata: index.metadata.clone(),
        tags: index.tags.clone().unwrap_or_else(default_tags),
        layout: index.layout.clone(),
        extensions: if index.extensions.is_empty() { None } else { Some(index.extensions.clone()) },
    })
}

/// Reads a document from a byte source. With `descriptions_for`, Markdown is
/// loaded only for those modules (and their ports); other descriptions are
/// left empty and their blocks are never read.
pub fn read_document(source: &mut dyn ByteSource, descriptions_for: Option<&HashSet<String>>) -> Result<Document> {
    let read = read_index(source)?;
    index_to_document(&read.index, |module_id, r| match descriptions_for {
        Some(wanted) if !wanted.contains(module_id) => Ok(String::new()),
        _ => read_content(source, &read.index, r),
    })
}

/// Decodes a complete `.mdg` buffer.
pub fn decode_mdg(bytes: &[u8]) -> Result<Document> {
    read_document(&mut BytesSource::new(bytes), None)
}

/// Decodes only the header and index of a `.mdg` buffer.
pub fn decode_mdg_index(bytes: &[u8]) -> Result<Index> {
    Ok(read_index(&mut BytesSource::new(bytes))?.index)
}
