//! File access for `.mdg` files: random-access partial reads and atomic writes.

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::decode::{read_document, read_index, IndexRead};
use super::encode::{encode_mdg, EncodeOptions};
use super::source::{io_error, FileSource};
use crate::error::{MdgError, Result};
use crate::model::{Document, Module};
use crate::queries::find_module;

/// Runs `f` with an open file source; the file is closed afterwards.
pub fn with_mdg_source<T>(path: &Path, f: impl FnOnce(&mut FileSource) -> Result<T>) -> Result<T> {
    let mut source = FileSource::open(path)?;
    f(&mut source)
}

/// Reads a complete document including every Markdown block.
pub fn read_mdg(path: &Path) -> Result<Document> {
    with_mdg_source(path, |source| read_document(source, None))
}

/// Reads only the header and index. No Markdown blocks are read.
pub fn read_mdg_index(path: &Path) -> Result<IndexRead> {
    with_mdg_source(path, |source| read_index(source))
}

/// Reads the structure plus the Markdown of selected modules only. Content
/// blocks of unrelated modules are never read from disk. Never write the
/// result back: other descriptions are empty.
pub fn read_mdg_modules(path: &Path, module_ids: &HashSet<String>) -> Result<Document> {
    with_mdg_source(path, |source| read_document(source, Some(module_ids)))
}

/// Reads a single module's Markdown and the structure describing it.
pub fn read_mdg_module(path: &Path, module_id: &str) -> Result<(Module, Document)> {
    let document = read_mdg_modules(path, &HashSet::from([module_id.to_string()]))?;
    let module = find_module(&document, module_id)
        .cloned()
        .ok_or_else(|| MdgError::not_found(format!("Module \"{module_id}\" does not exist.")))?;
    Ok((module, document))
}

fn temp_path(path: &Path) -> PathBuf {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let dir = absolute.parent().map(Path::to_path_buf).unwrap_or_default();
    let base = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let millis = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    dir.join(format!(".{base}.{}.{millis}.tmp", std::process::id()))
}

/// Writes raw bytes atomically: temporary file in the same directory,
/// flushed to disk, then renamed over the destination. On failure the
/// temporary file is removed and the destination is untouched.
pub fn write_file_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = temp_path(path);
    let result = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&tmp);
        return Err(io_error(&error, path));
    }
    Ok(())
}

/// Validates and serializes a document, then writes it atomically. An
/// invalid document fails before anything touches the destination.
pub fn write_mdg(path: &Path, document: &Document) -> Result<()> {
    let bytes = encode_mdg(document, EncodeOptions::default())?;
    write_file_atomic(path, &bytes)
}
