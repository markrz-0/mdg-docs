//! Byte-level compatibility: with files written by the original TypeScript
//! implementation (schema 1.0, event ports only), and between
//! `examples/authentication.mdg` and its builder.

use std::path::Path;

use mdg_core::*;

/// `examples/authentication.mdg` as written by the TypeScript implementation.
const EXAMPLE_V1: &[u8] = include_bytes!("fixtures/authentication-v1.0.mdg");

#[test]
fn v1_example_builder_reproduces_the_typescript_file() {
    let bytes = encode_mdg(&create_authentication_example_v1(), EncodeOptions::default()).unwrap();
    assert_eq!(bytes.len(), EXAMPLE_V1.len());
    assert!(bytes == EXAMPLE_V1, "encoded bytes differ from tests/fixtures/authentication-v1.0.mdg");
}

#[test]
fn typescript_file_round_trips_byte_for_byte() {
    let doc = decode_mdg(EXAMPLE_V1).unwrap();
    assert_eq!(doc.format_version, "1.0.0");
    assert_eq!(doc, create_authentication_example_v1());
    assert!(encode_mdg(&doc, EncodeOptions::default()).unwrap() == EXAMPLE_V1);
}

/// Regenerate with `UPDATE_FIXTURES=1 cargo test -p mdg-core --test compat`.
#[test]
fn example_file_matches_the_builder() {
    let bytes = encode_mdg(&create_authentication_example(), EncodeOptions::default()).unwrap();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/authentication.mdg");
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(&path, &bytes).unwrap();
    }
    let current = std::fs::read(&path).unwrap_or_default();
    assert!(current == bytes, "stale examples/authentication.mdg; rerun with UPDATE_FIXTURES=1");

    let doc = decode_mdg(&current).unwrap();
    assert_eq!(doc.format_version, REQUEST_PORTS_SCHEMA_VERSION);
    assert_eq!(doc, create_authentication_example());
    assert!(encode_mdg(&doc, EncodeOptions::default()).unwrap() == current);
}

#[test]
fn examples_are_valid_without_warnings() {
    for doc in [create_authentication_example(), create_authentication_example_v1()] {
        let result = validate_document(&doc, ValidationOptions::default());
        assert!(result.valid, "{:?}", result.errors);
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    }
}
