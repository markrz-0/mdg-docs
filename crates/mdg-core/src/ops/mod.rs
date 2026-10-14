//! Pure edit operations. Every operation takes a document and returns a new
//! one, or an error. The input is never modified, which gives atomic
//! multi-step edits, dry runs and cheap undo snapshots.

mod connections;
mod document;
mod modules;
mod ports;
mod routing;
mod transaction;

use std::sync::Arc;

pub use connections::*;
pub use document::*;
pub use modules::*;
pub use ports::*;
pub use routing::*;
pub use transaction::*;

use crate::error::{MdgError, Result};
use crate::js;
use crate::model::{Document, Module};
use crate::queries::get_module;

/// Result of an operation that creates a new element.
#[derive(Debug, Clone)]
pub struct CreateResult {
    pub document: Document,
    /// ID of the created element.
    pub id: String,
}

/// Returns a new document where `module_id` is replaced by `update(module)`.
pub fn update_module(doc: &Document, module_id: &str, update: impl FnOnce(&mut Module)) -> Result<Document> {
    let mut module = get_module(doc, module_id)?.clone();
    update(&mut module);
    let mut next = doc.clone();
    next.modules.insert(module_id, Arc::new(module));
    Ok(next)
}

/// Trims a name and rejects empty names.
pub fn require_name(name: &str, what: &str) -> Result<String> {
    let trimmed = js::trim(name);
    if trimmed.is_empty() {
        return Err(MdgError::argument(format!("{what} name must not be empty.")));
    }
    Ok(trimmed.to_string())
}

/// Inserts `item` at `index` (clamped), or appends when index is `None`.
pub fn insert_at<T>(list: &mut Vec<T>, item: T, index: Option<usize>) {
    match index {
        Some(i) if i < list.len() => list.insert(i, item),
        _ => list.push(item),
    }
}
