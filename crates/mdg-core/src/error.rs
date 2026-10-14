use std::fmt;

use crate::validate::ValidationIssue;

/// Machine-readable error categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    NotFound,
    InvalidArgument,
    InvalidOperation,
    ValidationFailed,
    FormatError,
    UnsupportedVersion,
    IoError,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::NotFound => "NOT_FOUND",
            ErrorCode::InvalidArgument => "INVALID_ARGUMENT",
            ErrorCode::InvalidOperation => "INVALID_OPERATION",
            ErrorCode::ValidationFailed => "VALIDATION_FAILED",
            ErrorCode::FormatError => "FORMAT_ERROR",
            ErrorCode::UnsupportedVersion => "UNSUPPORTED_VERSION",
            ErrorCode::IoError => "IO_ERROR",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Every error raised by MDG. `code` is stable; `issues` is set only for
/// [`ErrorCode::ValidationFailed`].
#[derive(Debug, Clone, PartialEq)]
pub struct MdgError {
    pub code: ErrorCode,
    pub message: String,
    pub issues: Option<Vec<ValidationIssue>>,
}

impl MdgError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        MdgError { code, message: message.into(), issues: None }
    }

    /// A referenced module, port or connection does not exist.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }

    /// An argument is malformed (bad ID, empty name, ...).
    pub fn argument(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidArgument, message)
    }

    /// An operation would violate a structural rule and was rejected.
    pub fn operation(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidOperation, message)
    }

    /// A document failed structural validation.
    pub fn validation(message: impl Into<String>, issues: Vec<ValidationIssue>) -> Self {
        MdgError { code: ErrorCode::ValidationFailed, message: message.into(), issues: Some(issues) }
    }

    /// A `.mdg` file is malformed or corrupted.
    pub fn format(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::FormatError, message)
    }

    /// A `.mdg` file uses a container or schema version this implementation cannot read.
    pub fn unsupported_version(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::UnsupportedVersion, message)
    }

    /// A filesystem operation failed.
    pub fn io(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::IoError, message)
    }
}

impl fmt::Display for MdgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for MdgError {}

pub type Result<T> = std::result::Result<T, MdgError>;
