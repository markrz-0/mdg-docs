use crate::error::{MdgError, Result};
use crate::model::Document;
use crate::validate::{validate_document, ValidationOptions};

/// Returns a `VALIDATION_FAILED` error if the document has structural errors.
pub fn assert_valid(doc: &Document) -> Result<()> {
    let result = validate_document(doc, ValidationOptions { warnings: false });
    if result.valid {
        return Ok(());
    }
    let summary: Vec<&str> = result.errors.iter().take(5).map(|e| e.message.as_str()).collect();
    Err(MdgError::validation(
        format!("Document has {} structural error(s). {}", result.errors.len(), summary.join(" ")),
        result.errors,
    ))
}

/// Runs a multi-step edit atomically.
///
/// Operations are pure, so `f` works on immutable snapshots. If any step
/// fails, or the final document fails validation, the error propagates and
/// the caller's original document is untouched.
pub fn transaction(doc: &Document, f: impl FnOnce(&Document) -> Result<Document>) -> Result<Document> {
    let result = f(doc)?;
    assert_valid(&result)?;
    Ok(result)
}
