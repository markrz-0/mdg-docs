//! Byte-level compatibility with files written by the original TypeScript
//! implementation.

use mdg_core::*;

const EXAMPLE: &[u8] = include_bytes!("../../../examples/authentication.mdg");

#[test]
fn example_builder_reproduces_the_reference_file() {
    let bytes = encode_mdg(&create_authentication_example(), EncodeOptions::default()).unwrap();
    assert_eq!(bytes.len(), EXAMPLE.len());
    assert!(bytes == EXAMPLE, "encoded bytes differ from examples/authentication.mdg");
}

#[test]
fn reference_file_round_trips_byte_for_byte() {
    let doc = decode_mdg(EXAMPLE).unwrap();
    assert_eq!(doc, create_authentication_example());
    assert!(encode_mdg(&doc, EncodeOptions::default()).unwrap() == EXAMPLE);
}
