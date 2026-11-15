//! Errors with stable codes, a plain-English description and a "what to do next" hint.

use std::fmt;

/// Stable, machine-readable error codes. Codes are part of the protocol: never rename one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    InvalidJson,
    UnknownSchema,
    SchemaTooNew,
    SchemaTooOld,
    InvalidScene,
    LimitExceeded,
    TooDeep,
    OutOfRange,
    InvalidPathData,
    InvalidSvg,
    InvalidCapture,
    EmptySelection,
    InvalidMessage,
    ProtocolMismatch,
    InvalidOptions,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::InvalidJson => "invalid_json",
            ErrorCode::UnknownSchema => "unknown_schema",
            ErrorCode::SchemaTooNew => "schema_too_new",
            ErrorCode::SchemaTooOld => "schema_too_old",
            ErrorCode::InvalidScene => "invalid_scene",
            ErrorCode::LimitExceeded => "limit_exceeded",
            ErrorCode::TooDeep => "too_deep",
            ErrorCode::OutOfRange => "out_of_range",
            ErrorCode::InvalidPathData => "invalid_path_data",
            ErrorCode::InvalidSvg => "invalid_svg",
            ErrorCode::InvalidCapture => "invalid_capture",
            ErrorCode::EmptySelection => "empty_selection",
            ErrorCode::InvalidMessage => "invalid_message",
            ErrorCode::ProtocolMismatch => "protocol_mismatch",
            ErrorCode::InvalidOptions => "invalid_options",
        }
    }
}

/// An error: what happened (`message`) and what the user should do next (`hint`).
#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    pub code: ErrorCode,
    pub message: String,
    pub hint: String,
}

impl Error {
    pub fn new(code: ErrorCode, message: impl Into<String>, hint: impl Into<String>) -> Self {
        Error { code, message: message.into(), hint: hint.into() }
    }

    /// Error caused by malformed input data rather than by a version mismatch.
    pub fn data(code: ErrorCode, message: impl Into<String>) -> Self {
        Error::new(
            code,
            message,
            "The design data could not be read. Try a smaller selection, and if it keeps \
             happening, save the log (Open log) and report it.",
        )
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.message, self.hint)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
