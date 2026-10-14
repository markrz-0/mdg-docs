mod common;

use std::collections::HashSet;

use common::*;
use mdg_core::*;
use serde_json::{json, Map, Value};

fn encode(doc: &Document) -> Vec<u8> {
    encode_mdg(doc, EncodeOptions::default()).unwrap()
}

fn good() -> Vec<u8> {
    encode(&create_authentication_example())
}

fn decode_err(bytes: &[u8]) -> MdgError {
    decode_mdg(bytes).expect_err("decoding should fail")
}

fn patch_u64(bytes: &[u8], at: usize, value: u64) -> Vec<u8> {
    let mut copy = bytes.to_vec();
    copy[at..at + 8].copy_from_slice(&value.to_le_bytes());
    copy
}

fn index_range(bytes: &[u8]) -> (usize, usize) {
    let offset = u64::from_le_bytes(bytes[8..16].try_into().unwrap()) as usize;
    let length = u64::from_le_bytes(bytes[16..24].try_into().unwrap()) as usize;
    (offset, length)
}

/// Rebuilds a file with a modified index while keeping content blocks where they are.
fn with_index(bytes: &[u8], mutate: impl FnOnce(&mut Map<String, Value>)) -> Vec<u8> {
    let (offset, length) = index_range(bytes);
    let mut index: Value = serde_json::from_slice(&bytes[offset..offset + length]).unwrap();
    mutate(index.as_object_mut().unwrap());
    let json = js::stringify(&index).into_bytes();
    assert!(json.len() <= length, "mutated index grew; test helper requires same or smaller size");
    let mut copy = bytes.to_vec();
    copy[offset..offset + length].fill(b' ');
    copy[offset..offset + json.len()].copy_from_slice(&json);
    copy
}

fn obj<'a>(v: &'a mut Map<String, Value>, path: &[&str]) -> &'a mut Map<String, Value> {
    let mut cur = v;
    for key in path {
        cur = cur.get_mut(*key).unwrap().as_object_mut().unwrap();
    }
    cur
}

// ------------------------------------------------------------ round trips

#[test]
fn round_trips_an_empty_document() {
    let doc = Builder::new().doc;
    assert_eq!(decode_mdg(&encode(&doc)).unwrap(), doc);
}

#[test]
fn round_trips_a_nested_document_with_layout_metadata_and_extensions() {
    let mut doc = create_authentication_example();
    doc.extensions = Some([("x-tool", json!({ "b": 1, "a": [true, null] }))].into_iter().collect());
    doc.metadata.extra.insert("owner", json!("platform-team"));
    let decoded = decode_mdg(&encode(&doc)).unwrap();
    // Extension objects come back with canonical (sorted) keys.
    assert_eq!(
        js::stringify(decoded.extensions.as_ref().unwrap().get("x-tool").unwrap()),
        r#"{"a":[true,null],"b":1}"#
    );
    assert_eq!(decoded.metadata, doc.metadata);
    assert_eq!(decoded.modules, doc.modules);
}

#[test]
fn preserves_unicode_markdown_and_computes_utf8_offsets() {
    let doc = Builder::new().module("a").input("a", "in").doc;
    let text = "Zażółć gęślą jaźń — 日本語 — 🚀👩‍💻 — \u{1F600}\r\n\u{0}tail";
    let doc = update_module_description(&doc, "a", text).unwrap();
    let doc = update_port_description(&doc, "a", "in", "∑ émoji 🎉").unwrap();
    let bytes = encode(&doc);
    let index = decode_mdg_index(&bytes).unwrap();
    let entry = *index.content.get("module/a").unwrap();
    assert_eq!(entry.length as usize, text.len());
    assert!(entry.length as usize > text.encode_utf16().count());
    assert_eq!(&bytes[entry.offset as usize..(entry.offset + entry.length) as usize], text.as_bytes());
    assert_eq!(decode_mdg(&bytes).unwrap(), doc);
}

#[test]
fn handles_large_descriptions() {
    let big = format!("# Big\n{}", "Lorem ipsum ąę 🚀 ".repeat(200_000));
    let doc = update_module_description(&Builder::new().module("a").doc, "a", &big).unwrap();
    assert_eq!(module(&decode_mdg(&encode(&doc)).unwrap(), "a").description, big);
}

#[test]
fn serializes_deterministically() {
    let doc = create_authentication_example();
    let a = encode(&doc);
    assert_eq!(encode(&decode_mdg(&a).unwrap()), a);
    // Module insertion order must not matter.
    let mut reversed = doc.clone();
    reversed.modules = doc.modules.keys().into_iter().rev().map(|k| (k, doc.modules.get(k).unwrap().clone())).collect();
    assert_eq!(encode(&reversed), a);
}

#[test]
fn orders_numeric_ids_like_javascript() {
    let doc = Builder::new().module("10").module("9").module("b").module("a").doc;
    let bytes = encode(&doc);
    let (offset, length) = index_range(&bytes);
    let text = std::str::from_utf8(&bytes[offset..offset + length]).unwrap();
    let modules = &text[text.find("\"modules\"").unwrap()..];
    let pos = |id: &str| modules.find(&format!("\"{id}\":{{\"id\"")).unwrap();
    assert!(pos("9") < pos("10") && pos("10") < pos("a") && pos("a") < pos("b") && pos("b") < pos("root"));
    assert_eq!(decode_mdg(&bytes).unwrap(), doc);
}

#[test]
fn writes_the_documented_header() {
    let bytes =
        encode(&create_document(CreateDocumentOptions { root_id: Some("r".into()), ..Default::default() }).unwrap());
    assert_eq!(&bytes[0..4], b"MDG1");
    assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), 1);
    assert_eq!(u16::from_le_bytes([bytes[6], bytes[7]]), 0);
    assert_eq!(index_range(&bytes).0, 32);
    assert_eq!(&bytes[24..32], &[0; 8]);
}

#[test]
fn refuses_to_encode_invalid_documents() {
    let mut doc = Builder::new().doc;
    doc.root_id = "missing".into();
    assert_eq!(encode_mdg(&doc, EncodeOptions::default()).unwrap_err().code, ErrorCode::ValidationFailed);
}

// ------------------------------------------------------------ integrity checks

#[test]
fn rejects_invalid_magic_bytes() {
    let mut bytes = good();
    bytes[0] = 0x58;
    assert!(decode_err(&bytes).message.contains("magic"));
}

#[test]
fn rejects_truncated_files() {
    let bytes = good();
    assert!(decode_err(&bytes[..10]).message.contains("truncated"));
    assert_eq!(decode_err(&bytes[..32 + 20]).code, ErrorCode::FormatError);
    assert!(decode_err(&bytes[..bytes.len() - 5]).message.contains("outside the file"));
}

#[test]
fn rejects_invalid_index_offsets() {
    let bytes = good();
    assert!(decode_err(&patch_u64(&bytes, 8, 4)).message.contains("overlaps the header"));
    assert_eq!(
        decode_err(&patch_u64(&bytes, 8, 1 << 60)).message,
        "Header index offset (1152921504606846976) exceeds the safely representable range."
    );
    assert!(decode_err(&patch_u64(&bytes, 16, 10_000_000)).message.contains("outside the file"));
    assert!(decode_err(&patch_u64(&bytes, 16, 0)).message.contains("zero"));
}

#[test]
fn rejects_unsupported_versions() {
    let bytes = good();
    let mut v2 = bytes.clone();
    v2[4] = 2;
    assert_eq!(decode_err(&v2).code, ErrorCode::UnsupportedVersion);
    let schema2 = with_index(&bytes, |index| {
        index.insert("schemaVersion".into(), json!("2.0.0"));
    });
    assert_eq!(decode_err(&schema2).code, ErrorCode::UnsupportedVersion);
    let mut doc = create_document(CreateDocumentOptions::default()).unwrap();
    doc.format_version = "2.0.0".into();
    assert_eq!(encode_mdg(&doc, EncodeOptions::default()).unwrap_err().code, ErrorCode::UnsupportedVersion);
}

#[test]
fn rejects_invalid_json_and_utf8_in_the_index() {
    let bytes = good();
    let (offset, _) = index_range(&bytes);
    let mut bad_json = bytes.clone();
    bad_json[offset] = b'[';
    assert!(decode_err(&bad_json).message.contains("JSON"));
    let mut bad_utf8 = bytes.clone();
    bad_utf8[offset + 1] = 0xff;
    assert!(decode_err(&bad_utf8).message.contains("UTF-8"));
}

#[test]
fn rejects_invalid_utf8_in_content_blocks() {
    let bytes = good();
    let entry = *decode_mdg_index(&bytes).unwrap().content.get("module/application").unwrap();
    let mut copy = bytes.clone();
    copy[entry.offset as usize] = 0xc3;
    copy[entry.offset as usize + 1] = 0x41;
    assert_eq!(decode_err(&copy).message, "Content block \"module/application\" is not valid UTF-8.");
}

#[test]
fn rejects_content_ranges_outside_the_file_or_overlapping() {
    let bytes = good();
    let outside = with_index(&bytes, |index| {
        obj(index, &["metadata"]).remove("description"); // make room for longer digits
        obj(index, &["content", "module/application"]).insert("offset".into(), json!(9_999_999));
    });
    assert!(decode_err(&outside).message.contains("outside the file"));
    let overlapping = with_index(&bytes, |index| {
        let app = obj(index, &["content", "module/application"]).get("offset").cloned().unwrap();
        let frontend = obj(index, &["content", "module/frontend"]);
        frontend.insert("offset".into(), app);
        let len = frontend["length"].as_u64().unwrap().max(1);
        frontend.insert("length".into(), json!(len));
    });
    assert!(decode_err(&overlapping).message.contains("overlap"));
    let into_index = with_index(&bytes, |index| {
        obj(index, &["content", "module/application"]).insert("offset".into(), json!(34));
    });
    assert!(decode_err(&into_index).message.contains("overlaps the index"));
}

#[test]
fn rejects_malformed_index_records_and_missing_content_references() {
    let bytes = good();
    let unknown_field = with_index(&bytes, |index| {
        let frontend = obj(index, &["modules", "frontend"]);
        frontend.insert("x".into(), json!(1));
        frontend.remove("descriptionRef");
    });
    assert_eq!(
        decode_err(&unknown_field).message,
        "Malformed index: modules.frontend.descriptionRef: Required; modules.frontend: Unrecognized key(s) in object: 'x'"
    );
    let missing_ref = with_index(&bytes, |index| {
        obj(index, &["content"]).remove("module/frontend");
    });
    assert_eq!(
        decode_err(&missing_ref).message,
        "Module \"frontend\" references missing content block \"module/frontend\"."
    );
}

#[test]
fn reports_schema_problems_like_the_original() {
    let bytes = good();
    let broken = with_index(&bytes, |index| {
        obj(index, &["modules", "frontend"]).insert("name".into(), json!(5));
        obj(index, &["modules", "frontend"]).insert("parentId".into(), json!(true));
        obj(index, &["content", "module/frontend"]).insert("length".into(), json!(-1.5));
        obj(index, &["content", "module/frontend"]).insert("encoding".into(), json!("latin1"));
    });
    assert_eq!(
        decode_err(&broken).message,
        "Malformed index: modules.frontend.parentId: Expected string, received boolean; \
         modules.frontend.name: Expected string, received number; \
         content.module/frontend.length: Expected integer, received float; \
         content.module/frontend.length: Number must be greater than or equal to 0; \
         content.module/frontend.encoding: Invalid literal value, expected \"utf-8\""
    );
    let layout = with_index(&bytes, |index| {
        index.insert("layout".into(), json!(5));
        obj(index, &["metadata"]).remove("description");
    });
    assert_eq!(decode_err(&layout).message, "Malformed index: layout: Expected object, received number");
    let nested = with_index(&bytes, |index| {
        obj(index, &["layout", "positions"]).insert("frontend".into(), json!({ "x": "a" }));
        obj(index, &["modules", "frontend"]).insert("inputs".into(), json!({}));
        obj(index, &["metadata"]).remove("description");
    });
    assert_eq!(
        decode_err(&nested).message,
        "Malformed index: modules.frontend.inputs: Expected array, received object; \
         layout.positions.frontend.x: Expected number, received string; layout.positions.frontend.y: Required"
    );
    let proto = with_index(&bytes, |index| {
        obj(index, &["metadata"]).insert("__proto__".into(), json!({}));
        obj(index, &["metadata"]).remove("description");
    });
    assert_eq!(decode_err(&proto).message, "Forbidden key \"__proto__\" in metadata.");
}

#[test]
fn does_not_reject_structurally_invalid_documents_on_read() {
    let broken = with_index(&good(), |index| {
        obj(index, &["modules", "application"]).insert("childIds".into(), json!(["frontend"]));
    });
    let doc = decode_mdg(&broken).unwrap();
    assert_eq!(module(&doc, "application").child_ids, vec!["frontend"]);
}

// ------------------------------------------------------------ partial reads

struct Counting<'a> {
    inner: BytesSource<'a>,
    reads: Vec<(u64, u64)>,
}

impl ByteSource for Counting<'_> {
    fn size(&self) -> u64 {
        self.inner.size()
    }
    fn read(&mut self, offset: u64, length: u64) -> Result<Vec<u8>> {
        self.reads.push((offset, length));
        self.inner.read(offset, length)
    }
}

#[test]
fn reads_only_requested_content_blocks() {
    let bytes = good();
    let mut source = Counting { inner: BytesSource::new(&bytes), reads: vec![] };
    let wanted = HashSet::from(["jwt-validator".to_string()]);
    let doc = read_document(&mut source, Some(&wanted)).unwrap();
    assert!(module(&doc, "jwt-validator").description.contains("Stateless"));
    assert!(module(&doc, "jwt-validator").inputs[0].description.contains("bearer token"));
    assert_eq!(module(&doc, "frontend").description, "");
    // header + index + module description + 2 port descriptions
    assert_eq!(source.reads.len(), 5);
}

// ------------------------------------------------------------ file API

#[test]
fn writes_and_reads_files() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.mdg");
    let doc = create_authentication_example();
    write_mdg(&file, &doc).unwrap();
    assert_eq!(read_mdg(&file).unwrap(), doc);
    let read = read_mdg_index(&file).unwrap();
    assert_eq!(read.size, std::fs::metadata(&file).unwrap().len());
    assert_eq!(read.index.modules.len(), 9);
    let (m, document) = read_mdg_module(&file, "token-issuer").unwrap();
    assert!(m.description.contains("15 minutes"));
    assert_eq!(module(&document, "frontend").description, "");
    assert_eq!(read_mdg(&dir.path().join("missing.mdg")).unwrap_err().code, ErrorCode::IoError);
}

#[test]
fn writes_atomically_and_never_leaves_partial_output() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.mdg");
    let doc = create_authentication_example();
    write_mdg(&file, &doc).unwrap();
    let before = std::fs::read(&file).unwrap();
    let mut invalid = doc.clone();
    invalid.root_id = "missing".into();
    assert_eq!(write_mdg(&file, &invalid).unwrap_err().code, ErrorCode::ValidationFailed);
    assert_eq!(std::fs::read(&file).unwrap(), before);

    let target = dir.path().join("is-a-directory");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("keep"), "x").unwrap();
    assert!(write_mdg(&target, &doc).is_err());
    let mut names: Vec<String> =
        std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    assert_eq!(names, vec!["a.mdg", "is-a-directory"]);
}

// ------------------------------------------------------------ boundary layout

#[test]
fn round_trips_boundary_panel_positions_deterministically() {
    let mut doc = create_authentication_example();
    let mut boundaries = JsMap::new();
    boundaries.insert(
        "user-routes",
        BoundaryLayout { input: Some(Position::new(-400.0, 10.0)), output: Some(Position::new(600.0, 10.0)) },
    );
    boundaries.insert("api-service", BoundaryLayout { input: None, output: Some(Position::new(900.5, -20.0)) });
    doc.layout.as_mut().unwrap().boundaries = Some(boundaries);
    let bytes = encode(&doc);
    let decoded = decode_mdg(&bytes).unwrap();
    let (a, b) = (decoded.layout.as_ref().unwrap(), doc.layout.as_ref().unwrap());
    for id in ["user-routes", "api-service"] {
        assert_eq!(a.boundaries.as_ref().unwrap().get(id), b.boundaries.as_ref().unwrap().get(id));
    }
    assert_eq!(encode(&decoded), bytes);
}

#[test]
fn rejects_malformed_boundary_entries() {
    let mut doc = create_authentication_example();
    let mut boundaries = JsMap::new();
    boundaries.insert("api-service", BoundaryLayout { input: Some(Position::new(1.0, 2.0)), output: None });
    doc.layout = Some(Layout { positions: JsMap::new(), boundaries: Some(boundaries) });
    let broken = with_index(&encode(&doc), |index| {
        obj(index, &["layout", "boundaries", "api-service"]).insert("sideways".into(), json!({ "x": 0, "y": 0 }));
        obj(index, &["metadata"]).remove("description");
    });
    assert!(decode_err(&broken).message.contains("Malformed index"));
}
