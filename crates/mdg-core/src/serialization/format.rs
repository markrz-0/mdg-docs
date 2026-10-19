//! `.mdg` container constants.
//!
//! Layout (v1):
//!   [32-byte header][UTF-8 JSON index][UTF-8 Markdown block]...
//!
//! Header (all integers little-endian):
//!   0-3   ASCII magic "MDG1"
//!   4-5   container major version (uint16)
//!   6-7   container minor version (uint16)
//!   8-15  index offset (uint64, absolute)
//!   16-23 index byte length (uint64)
//!   24-31 reserved, zero in v1.0

pub const MAGIC: &str = "MDG1";
pub const MAGIC_BYTES: [u8; 4] = *b"MDG1";
pub const HEADER_SIZE: u64 = 32;
pub const CONTAINER_MAJOR: u16 = 1;
pub const CONTAINER_MINOR: u16 = 0;
pub const CONTENT_ENCODING: &str = "utf-8";
/// Largest integer JavaScript can represent exactly (2^53 − 1). Offsets and
/// lengths above it are rejected so files stay readable everywhere.
pub const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

/// Content ID of a module description block.
pub fn module_content_id(module_id: &str) -> String {
    format!("module/{module_id}")
}

/// Content ID of a port description block.
pub fn port_content_id(module_id: &str, port_id: &str) -> String {
    format!("port/{module_id}/{port_id}")
}

/// Content ID of a request port's response block.
pub fn port_response_content_id(module_id: &str, port_id: &str) -> String {
    format!("port/{module_id}/{port_id}/response")
}
