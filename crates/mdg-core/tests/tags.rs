//! Module tags: the document's tag list, operations, validation,
//! serialization and documentation.

mod common;

use common::*;
use mdg_core::*;
use serde_json::Value;

fn index_json(bytes: &[u8]) -> Value {
    let offset = u64::from_le_bytes(bytes[8..16].try_into().unwrap()) as usize;
    let length = u64::from_le_bytes(bytes[16..24].try_into().unwrap()) as usize;
    serde_json::from_slice(&bytes[offset..offset + length]).unwrap()
}

fn ids(tags: &[Tag]) -> Vec<&str> {
    tags.iter().map(|t| t.id.as_str()).collect()
}

#[test]
fn new_documents_start_with_the_default_tags() {
    let doc = Builder::new().module("a").doc;
    assert_eq!(ids(&doc.tags), ["done", "partially-done", "not-done", "to-implement"]);
    let colors: Vec<&str> = doc.tags.iter().map(|t| t.color.as_str()).collect();
    assert_eq!(colors, ["#2a8a4a", "#d4a017", "#c43b3b", "#3b6fd8"]);
    // Untouched defaults need no schema 1.2 features.
    assert_eq!(doc.format_version, SCHEMA_VERSION);
    assert!(module(&doc, "a").tags.is_empty());
}

#[test]
fn untouched_default_tags_are_not_written() {
    let doc = Builder::new().module("a").doc;
    let bytes = encode_mdg(&doc, EncodeOptions::default()).unwrap();
    let index = index_json(&bytes);
    assert!(index.get("tags").is_none());
    assert!(index["modules"]["a"].get("tags").is_none());
    assert_eq!(index["schemaVersion"], "1.0.0");
    // ...and read back as the defaults.
    assert_eq!(decode_mdg(&bytes).unwrap().tags, default_tags());
}

#[test]
fn tagging_a_module_writes_schema_1_2_and_round_trips() {
    let doc = Builder::new().module("a").module("b").doc;
    let doc = tag_module(&doc, "a", &["done".into(), "partially-done".into()]).unwrap();
    assert_eq!(doc.format_version, TAGS_SCHEMA_VERSION);
    let bytes = encode_mdg(&doc, EncodeOptions::default()).unwrap();
    let index = index_json(&bytes);
    assert_eq!(index["schemaVersion"], "1.2.0");
    assert_eq!(index["modules"]["a"]["tags"], serde_json::json!(["done", "partially-done"]));
    assert!(index["modules"]["b"].get("tags").is_none());
    assert_eq!(index["tags"].as_array().unwrap().len(), 4);
    assert_eq!(index["tags"][0], serde_json::json!({ "id": "done", "name": "done", "color": "#2a8a4a" }));
    let back = decode_mdg(&bytes).unwrap();
    assert_eq!(back, doc);
    assert_eq!(encode_mdg(&back, EncodeOptions::default()).unwrap(), bytes);
}

#[test]
fn an_edited_tag_list_is_written_even_when_no_module_is_tagged() {
    let doc = Builder::new().module("a").doc;
    let doc = remove_tag(&doc, "not-done").unwrap();
    let doc = update_tag(&doc, "partially-done", Some("sketch"), Some("#ABCDEF")).unwrap();
    let bytes = encode_mdg(&doc, EncodeOptions::default()).unwrap();
    let back = decode_mdg(&bytes).unwrap();
    assert_eq!(ids(&back.tags), ["done", "partially-done", "to-implement"]);
    assert_eq!(back.find_tag("partially-done"), Some(&Tag::new("partially-done", "sketch", "#abcdef")));

    // Removing every tag is kept, not replaced by the defaults.
    let mut empty = back.clone();
    for id in ["done", "partially-done", "to-implement"] {
        empty = remove_tag(&empty, id).unwrap();
    }
    let back = decode_mdg(&encode_mdg(&empty, EncodeOptions::default()).unwrap()).unwrap();
    assert!(back.tags.is_empty());
}

#[test]
fn tag_operations() {
    let doc = Builder::new().module("a").doc;
    let created = add_tag(
        &doc,
        AddTagOptions {
            name: " review ".into(),
            color: "#8E44AD".into(),
            id: Some("review".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(created.id, "review");
    let doc = created.document;
    assert_eq!(doc.tags.last(), Some(&Tag::new("review", "review", "#8e44ad")));

    let generated = add_tag(&doc, AddTagOptions { name: "x".into(), color: "#000000".into(), ..Default::default() });
    assert!(generated.unwrap().id.starts_with("tag-"));

    let duplicate = add_tag(
        &doc,
        AddTagOptions { name: "x".into(), color: "#000000".into(), id: Some("done".into()), ..Default::default() },
    );
    assert_eq!(duplicate.unwrap_err().code, ErrorCode::InvalidArgument);
    let bad_color = add_tag(&doc, AddTagOptions { name: "x".into(), color: "red".into(), ..Default::default() });
    assert_eq!(bad_color.unwrap_err().code, ErrorCode::InvalidArgument);
    let no_name = add_tag(&doc, AddTagOptions { name: "  ".into(), color: "#000000".into(), ..Default::default() });
    assert_eq!(no_name.unwrap_err().code, ErrorCode::InvalidArgument);
    assert_eq!(update_tag(&doc, "nope", Some("x"), None).unwrap_err().code, ErrorCode::NotFound);

    let moved = move_tag(&doc, "review", 0).unwrap();
    assert_eq!(moved.tags[0].id, "review");
    assert_eq!(move_tag(&doc, "done", 99).unwrap().tags.last().unwrap().id, "done");
}

#[test]
fn module_tag_operations() {
    let doc = Builder::new().module("a").doc;
    let doc = set_module_tags(&doc, "a", &["partially-done".into(), "partially-done".into(), "done".into()]).unwrap();
    assert_eq!(module(&doc, "a").tags, ["partially-done", "done"]);
    let doc = tag_module(&doc, "a", &["done".into(), "to-implement".into()]).unwrap();
    assert_eq!(module(&doc, "a").tags, ["partially-done", "done", "to-implement"]);
    let doc = untag_module(&doc, "a", &["partially-done".into(), "not-done".into()]).unwrap();
    assert_eq!(module(&doc, "a").tags, ["done", "to-implement"]);
    assert_eq!(tag_module(&doc, "a", &["ghost".into()]).unwrap_err().code, ErrorCode::NotFound);
    assert_eq!(tag_module(&doc, "ghost", &["done".into()]).unwrap_err().code, ErrorCode::NotFound);

    // Removing a tag from the document untags every module.
    let doc = remove_tag(&doc, "done").unwrap();
    assert_eq!(module(&doc, "a").tags, ["to-implement"]);
    assert!(validate_document(&doc, ValidationOptions::default()).valid);
}

#[test]
fn validation_of_tags() {
    let doc = Builder::new().module("a").doc;
    let unknown = edit_module(&doc, "a", |m| m.tags = vec!["ghost".into()]);
    assert_eq!(error_codes(&unknown), ["UNKNOWN_TAG"]);
    let twice = edit_module(&doc, "a", |m| m.tags = vec!["done".into(), "done".into()]);
    assert_eq!(error_codes(&twice), ["DUPLICATE_MODULE_TAG"]);

    let mut bad = doc.clone();
    bad.tags.push(Tag::new("done", "again", "#000000"));
    bad.tags.push(Tag::new("-bad", "bad", "blue"));
    assert_eq!(error_codes(&bad), ["DUPLICATE_TAG_ID", "INVALID_ID", "INVALID_TAG_COLOR"]);
    assert!(encode_mdg(&bad, EncodeOptions::default()).is_err());
}

#[test]
fn malformed_tag_records_are_format_errors() {
    let doc = tag_module(&Builder::new().module("a").doc, "a", &["done".into()]).unwrap();
    let bytes = encode_mdg(&doc, EncodeOptions::default()).unwrap();
    // Same length, so offsets stay valid: an unknown field in a tag record.
    let (from, to) = (b"\"color\":\"#c43b3b\"", b"\"kolor\":\"#c43b3b\"");
    let at = bytes.windows(from.len()).position(|w| w == from).unwrap();
    let mut broken = bytes.clone();
    broken[at..at + to.len()].copy_from_slice(to);
    let err = decode_mdg(&broken).unwrap_err();
    assert_eq!(err.code, ErrorCode::FormatError);
    assert!(err.message.contains("tags.2"), "{}", err.message);
}

#[test]
fn describe_and_tree_show_tags() {
    let doc = Builder::new().module("a").module("b").doc;
    let doc = tag_module(&doc, "a", &["done".into(), "partially-done".into()]).unwrap();
    let markdown = describe_module_markdown(&doc, "a", Some(0)).unwrap();
    assert!(markdown.contains("- Tags: done (`done`), partially done (`partially-done`)"), "{markdown}");
    assert!(!describe_module_markdown(&doc, "b", Some(0)).unwrap().contains("Tags"));

    let json = js::to_value(&describe_module(&doc, "a", Some(0)).unwrap());
    assert_eq!(json["module"]["tags"][0]["color"], "#2a8a4a");
    assert!(js::to_value(&describe_module(&doc, "b", Some(0)).unwrap())["module"].get("tags").is_none());

    let tree = render_tree(&doc, &TreeOptions { ids: true, ..Default::default() }).unwrap();
    assert!(tree.contains("[a] #done #partially-done"), "{tree}");
    let nested = js::to_value(&build_tree(&doc, &TreeOptions::default()).unwrap());
    assert_eq!(nested["children"][0]["tags"], serde_json::json!(["done", "partially-done"]));
    assert!(nested["children"][1].get("tags").is_none());
}
