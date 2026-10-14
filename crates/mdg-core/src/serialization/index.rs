//! The JSON index (schema version 1.x) and its schema check.
//!
//! Module, port, connection, content and layout records are strict: unknown
//! fields there would be silently lost, so they are rejected instead.
//! Unknown keys in `metadata` and at the index top level are preserved.
//!
//! Problems are reported with the same wording and order as the original
//! implementation's schema library (zod), e.g.
//! `modules.a.inputs.0.name: Expected string, received number`.

use serde_json::{Map, Value};

use crate::jsmap::{js_key_order, JsMap};
use crate::model::{BoundaryLayout, Connection, Endpoint, Layout, Metadata, Position};
use crate::serialization::format::MAX_SAFE_INTEGER;

/// Top-level index keys defined by schema 1.x. Others are preserved as extensions.
pub const KNOWN_INDEX_KEYS: [&str; 6] = ["schemaVersion", "rootId", "metadata", "modules", "content", "layout"];

#[derive(Debug, Clone, PartialEq)]
pub struct IndexPort {
    pub id: String,
    pub name: String,
    pub description_ref: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexModule {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub description_ref: String,
    pub inputs: Vec<IndexPort>,
    pub outputs: Vec<IndexPort>,
    pub child_ids: Vec<String>,
    pub connections: Vec<Connection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentEntry {
    pub offset: u64,
    pub length: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Index {
    pub schema_version: String,
    pub root_id: String,
    pub metadata: Metadata,
    pub modules: JsMap<IndexModule>,
    pub content: JsMap<ContentEntry>,
    pub layout: Option<Layout>,
    /// Unknown top-level keys, preserved verbatim.
    pub extensions: JsMap<Value>,
}

/// zod's name for a JSON value's type.
fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// `Some(value)` when `ok`; the value's own fields are checked by the caller first.
fn keep<T>(ok: bool, value: T) -> Option<T> {
    ok.then_some(value)
}

#[derive(Default)]
struct Checker {
    issues: Vec<String>,
}

type Path<'a> = &'a [String];

fn child(path: Path<'_>, key: impl ToString) -> Vec<String> {
    let mut p = path.to_vec();
    p.push(key.to_string());
    p
}

impl Checker {
    fn issue(&mut self, path: Path<'_>, message: impl Into<String>) {
        let where_ = if path.is_empty() { "(root)".to_string() } else { path.join(".") };
        self.issues.push(format!("{where_}: {}", message.into()));
    }

    /// Reports a missing or mistyped value; returns false when it was reported.
    fn expect_type(&mut self, value: Option<&Value>, expected: &str, path: Path<'_>) -> bool {
        match value {
            None => {
                self.issue(path, "Required");
                false
            }
            Some(v) if type_name(v) != expected => {
                self.issue(path, format!("Expected {expected}, received {}", type_name(v)));
                false
            }
            Some(_) => true,
        }
    }

    fn string(&mut self, value: Option<&Value>, path: Path<'_>) -> Option<String> {
        if self.expect_type(value, "string", path) {
            value.and_then(Value::as_str).map(str::to_string)
        } else {
            None
        }
    }

    fn non_empty_string(&mut self, value: Option<&Value>, path: Path<'_>) -> Option<String> {
        let s = self.string(value, path)?;
        if s.is_empty() {
            self.issue(path, "String must contain at least 1 character(s)");
            return None;
        }
        Some(s)
    }

    fn optional_string(&mut self, value: Option<&Value>, path: Path<'_>) -> Option<Option<String>> {
        match value {
            None => Some(None),
            Some(_) => self.string(value, path).map(Some),
        }
    }

    fn nullable_string(&mut self, value: Option<&Value>, path: Path<'_>) -> Option<Option<String>> {
        match value {
            Some(Value::Null) => Some(None),
            _ => self.string(value, path).map(Some),
        }
    }

    fn safe_uint(&mut self, value: Option<&Value>, path: Path<'_>) -> Option<u64> {
        if !self.expect_type(value, "number", path) {
            return None;
        }
        let n = value.and_then(Value::as_f64).unwrap_or(f64::NAN);
        let mut ok = true;
        if n.fract() != 0.0 {
            self.issue(path, "Expected integer, received float");
            ok = false;
        }
        if n < 0.0 {
            self.issue(path, "Number must be greater than or equal to 0");
            ok = false;
        }
        if n > MAX_SAFE_INTEGER as f64 {
            self.issue(path, format!("Number must be less than or equal to {MAX_SAFE_INTEGER}"));
            ok = false;
        }
        ok.then_some(n as u64)
    }

    fn number(&mut self, value: Option<&Value>, path: Path<'_>) -> Option<f64> {
        if self.expect_type(value, "number", path) {
            value.and_then(Value::as_f64)
        } else {
            None
        }
    }

    fn object<'v>(&mut self, value: Option<&'v Value>, path: Path<'_>) -> Option<&'v Map<String, Value>> {
        if self.expect_type(value, "object", path) {
            value.and_then(Value::as_object)
        } else {
            None
        }
    }

    fn array<'v>(&mut self, value: Option<&'v Value>, path: Path<'_>) -> Option<&'v Vec<Value>> {
        if self.expect_type(value, "array", path) {
            value.and_then(Value::as_array)
        } else {
            None
        }
    }

    /// Reports keys outside `known` (after the known keys were checked, like zod).
    fn strict(&mut self, map: &Map<String, Value>, known: &[&str], path: Path<'_>) -> bool {
        let extra: Vec<String> = js_key_order(map.keys().map(String::as_str))
            .into_iter()
            .filter(|k| !known.contains(k))
            .map(|k| format!("'{k}'"))
            .collect();
        if extra.is_empty() {
            return true;
        }
        self.issue(path, format!("Unrecognized key(s) in object: {}", extra.join(", ")));
        false
    }

    /// Checks every entry of a record in JavaScript key order.
    fn record<T>(
        &mut self,
        value: Option<&Value>,
        path: Path<'_>,
        mut item: impl FnMut(&mut Self, &Value, Path<'_>) -> Option<T>,
    ) -> Option<JsMap<T>> {
        let map = self.object(value, path)?;
        let mut out = JsMap::new();
        let mut ok = true;
        for key in js_key_order(map.keys().map(String::as_str)) {
            match item(self, &map[key], &child(path, key)) {
                Some(v) => {
                    out.insert(key, v);
                }
                None => ok = false,
            }
        }
        ok.then_some(out)
    }

    fn list<T>(
        &mut self,
        value: Option<&Value>,
        path: Path<'_>,
        mut item: impl FnMut(&mut Self, &Value, Path<'_>) -> Option<T>,
    ) -> Option<Vec<T>> {
        let items = self.array(value, path)?;
        let mut out = Vec::with_capacity(items.len());
        let mut ok = true;
        for (i, v) in items.iter().enumerate() {
            match item(self, v, &child(path, i)) {
                Some(v) => out.push(v),
                None => ok = false,
            }
        }
        ok.then_some(out)
    }

    fn port(&mut self, value: &Value, path: Path<'_>) -> Option<IndexPort> {
        let map = self.object(Some(value), path)?;
        let id = self.string(map.get("id"), &child(path, "id"));
        let name = self.string(map.get("name"), &child(path, "name"));
        let description_ref = self.non_empty_string(map.get("descriptionRef"), &child(path, "descriptionRef"));
        let strict = self.strict(map, &["id", "name", "descriptionRef"], path);
        keep(strict, IndexPort { id: id?, name: name?, description_ref: description_ref? })
    }

    fn endpoint(&mut self, value: Option<&Value>, path: Path<'_>) -> Option<Endpoint> {
        let map = self.object(value, path)?;
        let module_id = self.string(map.get("moduleId"), &child(path, "moduleId"));
        let port_id = self.string(map.get("portId"), &child(path, "portId"));
        let strict = self.strict(map, &["moduleId", "portId"], path);
        keep(strict, Endpoint { module_id: module_id?, port_id: port_id? })
    }

    fn connection(&mut self, value: &Value, path: Path<'_>) -> Option<Connection> {
        let map = self.object(Some(value), path)?;
        let id = self.string(map.get("id"), &child(path, "id"));
        let from = self.endpoint(map.get("from"), &child(path, "from"));
        let to = self.endpoint(map.get("to"), &child(path, "to"));
        let strict = self.strict(map, &["id", "from", "to"], path);
        keep(strict, Connection { id: id?, from: from?, to: to? })
    }

    fn module(&mut self, value: &Value, path: Path<'_>) -> Option<IndexModule> {
        let map = self.object(Some(value), path)?;
        let id = self.string(map.get("id"), &child(path, "id"));
        let parent_id = self.nullable_string(map.get("parentId"), &child(path, "parentId"));
        let name = self.string(map.get("name"), &child(path, "name"));
        let description_ref = self.non_empty_string(map.get("descriptionRef"), &child(path, "descriptionRef"));
        let inputs = self.list(map.get("inputs"), &child(path, "inputs"), Self::port);
        let outputs = self.list(map.get("outputs"), &child(path, "outputs"), Self::port);
        let child_ids = self.list(map.get("childIds"), &child(path, "childIds"), |c, v, p| c.string(Some(v), p));
        let connections = self.list(map.get("connections"), &child(path, "connections"), Self::connection);
        let strict = self.strict(
            map,
            &["id", "parentId", "name", "descriptionRef", "inputs", "outputs", "childIds", "connections"],
            path,
        );
        keep(
            strict,
            IndexModule {
                id: id?,
                parent_id: parent_id?,
                name: name?,
                description_ref: description_ref?,
                inputs: inputs?,
                outputs: outputs?,
                child_ids: child_ids?,
                connections: connections?,
            },
        )
    }

    fn content(&mut self, value: &Value, path: Path<'_>) -> Option<ContentEntry> {
        let map = self.object(Some(value), path)?;
        let offset = self.safe_uint(map.get("offset"), &child(path, "offset"));
        let length = self.safe_uint(map.get("length"), &child(path, "length"));
        let encoding_path = child(path, "encoding");
        let encoding_ok = map.get("encoding").and_then(Value::as_str) == Some("utf-8");
        if !encoding_ok {
            self.issue(&encoding_path, "Invalid literal value, expected \"utf-8\"");
        }
        let strict = self.strict(map, &["offset", "length", "encoding"], path);
        keep(strict && encoding_ok, ContentEntry { offset: offset?, length: length? })
    }

    fn position(&mut self, value: Option<&Value>, path: Path<'_>) -> Option<Position> {
        let map = self.object(value, path)?;
        let x = self.number(map.get("x"), &child(path, "x"));
        let y = self.number(map.get("y"), &child(path, "y"));
        let strict = self.strict(map, &["x", "y"], path);
        keep(strict, Position { x: x?, y: y? })
    }

    fn optional_position(&mut self, value: Option<&Value>, path: Path<'_>) -> Option<Option<Position>> {
        match value {
            None => Some(None),
            Some(_) => self.position(value, path).map(Some),
        }
    }

    fn boundary(&mut self, value: &Value, path: Path<'_>) -> Option<BoundaryLayout> {
        let map = self.object(Some(value), path)?;
        let input = self.optional_position(map.get("input"), &child(path, "input"));
        let output = self.optional_position(map.get("output"), &child(path, "output"));
        let strict = self.strict(map, &["input", "output"], path);
        keep(strict, BoundaryLayout { input: input?, output: output? })
    }

    fn layout(&mut self, value: Option<&Value>, path: Path<'_>) -> Option<Option<Layout>> {
        if value.is_none() {
            return Some(None);
        }
        let map = self.object(value, path)?;
        let positions = self.record(map.get("positions"), &child(path, "positions"), |c, v, p| c.position(Some(v), p));
        let boundaries = match map.get("boundaries") {
            None => Some(None),
            Some(v) => self.record(Some(v), &child(path, "boundaries"), Self::boundary).map(Some),
        };
        let strict = self.strict(map, &["positions", "boundaries"], path);
        keep(strict, Some(Layout { positions: positions?, boundaries: boundaries? }))
    }

    fn metadata(&mut self, value: Option<&Value>, path: Path<'_>) -> Option<Metadata> {
        let map = self.object(value, path)?;
        let title = self.string(map.get("title"), &child(path, "title"));
        let description = self.optional_string(map.get("description"), &child(path, "description"));
        let extra = js_key_order(map.keys().map(String::as_str))
            .into_iter()
            .filter(|k| *k != "title" && *k != "description")
            .map(|k| (k, map[k].clone()))
            .collect();
        Some(Metadata { title: title?, description: description?, extra })
    }
}

/// Checks a parsed index object against the schema. On failure, returns the
/// first five problems joined by `; `.
pub fn parse_index(raw: &Map<String, Value>) -> std::result::Result<Index, String> {
    let mut c = Checker::default();
    let field = |name: &str| vec![name.to_string()];
    let schema_version = c.string(raw.get("schemaVersion"), &field("schemaVersion"));
    let root_id = c.string(raw.get("rootId"), &field("rootId"));
    let metadata = c.metadata(raw.get("metadata"), &field("metadata"));
    let modules = c.record(raw.get("modules"), &field("modules"), Checker::module);
    let content = c.record(raw.get("content"), &field("content"), Checker::content);
    let layout = c.layout(raw.get("layout"), &field("layout"));
    if !c.issues.is_empty() {
        return Err(c.issues.iter().take(5).cloned().collect::<Vec<_>>().join("; "));
    }
    let extensions = js_key_order(raw.keys().map(String::as_str))
        .into_iter()
        .filter(|k| !KNOWN_INDEX_KEYS.contains(k))
        .map(|k| (k, raw[k].clone()))
        .collect();
    Ok(Index {
        schema_version: schema_version.expect("checked"),
        root_id: root_id.expect("checked"),
        metadata: metadata.expect("checked"),
        modules: modules.expect("checked"),
        content: content.expect("checked"),
        layout: layout.expect("checked"),
        extensions,
    })
}
