//! The `.mdg` file format: header, JSON index and Markdown content blocks.
//! See `docs/format.md`.

pub mod decode;
pub mod encode;
pub mod format;
pub mod fs;
pub mod index;
pub mod source;

pub use decode::*;
pub use encode::*;
pub use format::*;
pub use fs::*;
pub use index::*;
pub use source::*;
