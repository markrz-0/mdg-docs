use std::collections::HashSet;
use std::io;
use std::path::Path;

use mdg_core::{
    index_to_document, js, read_content, read_index, with_mdg_source, Document, IndexRead, MdgError, Result,
};
use serde::Serialize;

use crate::commander::Output;

/// Output/input channels, injectable for in-process tests.
pub trait CliIo: Output {
    fn read_stdin(&mut self) -> String;
}

/// Exit codes. Stable; documented in docs/cli.md.
pub mod exit {
    pub const OK: i32 = 0;
    /// Validation errors, or an edit rejected by structural rules.
    pub const FAILURE: i32 = 1;
    /// Bad command-line usage or malformed argument.
    pub const USAGE: i32 = 2;
    /// File missing, unreadable, corrupted or unsupported version.
    pub const FILE: i32 = 3;
    /// A referenced module, port or connection does not exist.
    pub const NOT_FOUND: i32 = 4;
}

pub struct Context<'a> {
    pub io: &'a mut dyn CliIo,
    pub exit_code: i32,
}

impl Context<'_> {
    /// Prints text, adding a final newline when missing.
    pub fn print(&mut self, text: &str) {
        if text.ends_with('\n') {
            self.io.out(text);
        } else {
            self.io.out(&format!("{text}\n"));
        }
    }

    /// Stable JSON output: 2-space indentation, trailing newline.
    pub fn print_json<T: Serialize + ?Sized>(&mut self, value: &T) {
        self.io.out(&format!("{}\n", js::to_pretty_json(value)));
    }
}

/// Describes a filesystem error the way Node.js does, e.g.
/// `ENOENT: no such file or directory, open 'notes.md'`.
pub fn fs_message(error: &io::Error, syscall: &str, path: &Path) -> String {
    // Node.js reports absolute paths on Windows and the given path elsewhere.
    let shown = if cfg!(windows) {
        std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
    } else {
        path.to_path_buf()
    };
    let shown = shown.display();
    if path.is_dir() && syscall == "open" {
        return "EISDIR: illegal operation on a directory, read".to_string();
    }
    match error.kind() {
        io::ErrorKind::NotFound => format!("ENOENT: no such file or directory, {syscall} '{shown}'"),
        io::ErrorKind::PermissionDenied => format!("EPERM: operation not permitted, {syscall} '{shown}'"),
        io::ErrorKind::AlreadyExists => format!("EEXIST: file already exists, {syscall} '{shown}'"),
        io::ErrorKind::NotADirectory => format!("ENOTDIR: not a directory, {syscall} '{shown}'"),
        _ => error.to_string(),
    }
}

/// Reads Markdown from `--description` or `--description-file` (`-` = stdin).
pub fn read_description(
    ctx: &mut Context<'_>,
    description: Option<String>,
    description_file: Option<String>,
) -> Result<Option<String>> {
    if description.is_some() && description_file.is_some() {
        return Err(MdgError::argument("Use either --description or --description-file, not both."));
    }
    let Some(file) = description_file else { return Ok(description) };
    if file == "-" {
        return Ok(Some(ctx.io.read_stdin()));
    }
    let path = Path::new(&file);
    std::fs::read(path)
        .map(|bytes| Some(String::from_utf8_lossy(&bytes).into_owned()))
        .map_err(|e| MdgError::io(format!("Cannot read {file}: {}", fs_message(&e, "open", path))))
}

pub struct PartialRead {
    pub read: IndexRead,
    /// Document with Markdown loaded only for the selected modules.
    pub doc: Document,
}

/// Opens a file once, reads the index, lets `select` choose which modules'
/// Markdown is needed (based on the structure), and loads only those blocks.
pub fn read_partial(file: &str, select: impl FnOnce(&Document) -> Result<Vec<String>>) -> Result<PartialRead> {
    with_mdg_source(Path::new(file), |source| {
        let read = read_index(source)?;
        let skeleton = index_to_document(&read.index, |_, _| Ok(String::new()))?;
        let wanted: HashSet<String> = select(&skeleton)?.into_iter().collect();
        if wanted.is_empty() {
            return Ok(PartialRead { read, doc: skeleton });
        }
        let doc = index_to_document(&read.index, |module_id, r| {
            if wanted.contains(module_id) {
                read_content(source, &read.index, r)
            } else {
                Ok(String::new())
            }
        })?;
        Ok(PartialRead { read, doc })
    })
}

/// Structure only; no Markdown blocks are read.
pub fn read_skeleton(file: &str) -> Result<PartialRead> {
    read_partial(file, |_| Ok(vec![]))
}

pub fn write_text_file(path: &Path, text: &str) -> Result<()> {
    std::fs::write(path, text)
        .map_err(|e| MdgError::io(format!("Cannot write {}: {}", path.display(), fs_message(&e, "open", path))))
}

/// The "Examples:" block appended to a command's help.
pub fn examples(lines: &[&str]) -> String {
    let lines: Vec<String> = lines.iter().map(|l| format!("  $ {l}")).collect();
    format!("\nExamples:\n{}\n", lines.join("\n"))
}

/// Commander option parser for non-negative integers.
pub fn non_negative_int(value: &str) -> std::result::Result<crate::commander::Value, String> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err("Expected a non-negative integer.".to_string());
    }
    // Like JavaScript's Number(): huge values saturate rather than fail.
    Ok(crate::commander::Value::Int(value.parse::<u64>().unwrap_or(u64::MAX)))
}

/// Commander option parser for positive integers.
pub fn positive_int(value: &str) -> std::result::Result<crate::commander::Value, String> {
    match non_negative_int(value)? {
        crate::commander::Value::Int(0) => Err("Expected a positive integer.".to_string()),
        other => Ok(other),
    }
}
