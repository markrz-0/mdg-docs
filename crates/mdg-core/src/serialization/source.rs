use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::error::{MdgError, Result};

/// Random-access byte source. Lets the reader fetch only the header, the
/// index and the content blocks it needs, whether the bytes live in memory
/// or in a file.
pub trait ByteSource {
    /// Total size in bytes.
    fn size(&self) -> u64;
    /// Reads exactly `length` bytes at `offset`. Callers validate ranges first.
    fn read(&mut self, offset: u64, length: u64) -> Result<Vec<u8>>;
}

fn outside(offset: u64, length: u64) -> MdgError {
    MdgError::format(format!("Read of {length} bytes at {offset} is outside the file."))
}

/// Wraps an in-memory buffer.
pub struct BytesSource<'a> {
    bytes: &'a [u8],
}

impl<'a> BytesSource<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        BytesSource { bytes }
    }
}

impl ByteSource for BytesSource<'_> {
    fn size(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read(&mut self, offset: u64, length: u64) -> Result<Vec<u8>> {
        let end =
            offset.checked_add(length).filter(|&end| end <= self.size()).ok_or_else(|| outside(offset, length))?;
        Ok(self.bytes[offset as usize..end as usize].to_vec())
    }
}

/// Random-access reads from an open file. The file is closed on drop.
pub struct FileSource {
    file: File,
    size: u64,
}

pub(crate) fn io_error(error: &std::io::Error, path: &Path) -> MdgError {
    if error.kind() == std::io::ErrorKind::NotFound {
        MdgError::io(format!("File not found: {}", path.display()))
    } else {
        MdgError::io(format!("Cannot access {}: {error}", path.display()))
    }
}

impl FileSource {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(|e| io_error(&e, path))?;
        let metadata = file.metadata().map_err(|e| io_error(&e, path))?;
        if metadata.is_dir() {
            return Err(MdgError::io(format!(
                "Cannot access {}: EISDIR: illegal operation on a directory, read",
                path.display()
            )));
        }
        Ok(FileSource { file, size: metadata.len() })
    }
}

impl ByteSource for FileSource {
    fn size(&self) -> u64 {
        self.size
    }

    fn read(&mut self, offset: u64, length: u64) -> Result<Vec<u8>> {
        if offset.checked_add(length).is_none_or(|end| end > self.size) {
            return Err(outside(offset, length));
        }
        let mut buffer = vec![0u8; length as usize];
        self.file
            .seek(SeekFrom::Start(offset))
            .and_then(|_| self.file.read_exact(&mut buffer))
            .map_err(|_| MdgError::format("Unexpected end of file."))?;
        Ok(buffer)
    }
}
