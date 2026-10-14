//! MDG (Markdown Graph) core: model, validation, operations, traversal,
//! documentation and `.mdg` (de)serialization. Used by both the `mdg` CLI
//! and the desktop editor, so every rule lives in one place.

pub mod documentation;
pub mod error;
pub mod example;
pub mod ids;
pub mod js;
pub mod jsmap;
pub mod model;
pub mod ops;
pub mod queries;
pub mod serialization;
pub mod trace;
pub mod validate;

pub use documentation::*;
pub use error::{ErrorCode, MdgError, Result};
pub use example::create_authentication_example;
pub use ids::*;
pub use jsmap::JsMap;
pub use model::*;
pub use ops::*;
pub use queries::*;
pub use serialization::*;
pub use trace::*;
pub use validate::*;
