//! Structured failures shared by codecs, transports, and PLC operations.

use std::{fmt, io};

/// A fallible library operation.
pub type Result<T> = std::result::Result<T, Error>;

/// An input, communication, or PLC failure. Protocol codes remain available.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// A caller supplied an invalid address, length, or value.
    InvalidInput(&'static str),
    /// A packet violated its declared layout or expected operation.
    Protocol(&'static str),
    /// An underlying socket or device operation failed.
    Io(io::Error),
    /// A bounded operation exhausted its deadline.
    Timeout,
    /// An operation requires an established session.
    Disconnected,
    /// A PLC rejected an entire request with this two-byte error code.
    Plc(u16),
    /// A PLC rejected an individual read/write item with this return code.
    Item(u8),
    /// A password or protection operation was refused with this code.
    Authentication(u16),
    /// A stale response reference was received.
    StaleReference {
        /// Expected request reference.
        expected: u16,
        /// Received response reference.
        received: u16,
    },
    /// A response reference indicates a lost or unmatched request.
    LostReference {
        /// Expected request reference.
        expected: u16,
        /// Received response reference.
        received: u16,
    },
    /// The request limiter rejected an exchange.
    RateLimited,
    /// An explicitly unsupported operation or representation.
    Unsupported(&'static str),
    /// Earlier chunks were acknowledged before a definite later failure.
    PartialWrite {
        /// Positively acknowledged bytes.
        confirmed: usize,
        /// Failure reported for the next chunk.
        source: Box<Error>,
    },
    /// A mutating operation may have executed despite a communication failure.
    AmbiguousWrite {
        /// Bytes positively acknowledged before this exchange.
        confirmed: usize,
        /// Bytes in the exchange whose outcome is unknown.
        uncertain: usize,
        /// Original communication failure.
        source: Box<Error>,
    },
}

impl fmt::Display for Error {
    /// Render a stable category together with available context or wire codes.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(s) => write!(f, "invalid input: {s}"),
            Self::Protocol(s) => write!(f, "invalid protocol: {s}"),
            Self::Io(e) => write!(f, "I/O: {e}"),
            Self::Timeout => f.write_str("operation timed out"),
            Self::Disconnected => f.write_str("not connected"),
            Self::Plc(code) => write!(f, "PLC error 0x{code:04x}"),
            Self::Item(code) => write!(f, "item error 0x{code:02x}: {}", item_error_text(*code)),
            Self::Authentication(code) => write!(f, "authentication error 0x{code:04x}"),
            Self::StaleReference { expected, received } => write!(
                f,
                "stale reference: expected {expected}, received {received}"
            ),
            Self::LostReference { expected, received } => write!(
                f,
                "lost reference: expected {expected}, received {received}"
            ),
            Self::RateLimited => f.write_str("request rate limit reached"),
            Self::Unsupported(s) => write!(f, "unsupported: {s}"),
            Self::PartialWrite { confirmed, source } => {
                write!(f, "partial write: {confirmed} bytes confirmed: {source}")
            }
            Self::AmbiguousWrite {
                confirmed,
                uncertain,
                source,
            } => write!(
                f,
                "write outcome unknown: {confirmed} bytes confirmed, {uncertain} uncertain: {source}"
            ),
        }
    }
}

impl std::error::Error for Error {
    /// Preserve the original transport failure for diagnostic callers.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::AmbiguousWrite { source, .. } | Self::PartialWrite { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    /// Normalize platform timeout representations without losing other causes.
    fn from(value: io::Error) -> Self {
        match value.kind() {
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => Self::Timeout,
            _ => Self::Io(value),
        }
    }
}

/// Describe a data-item return code, including an explicit unknown fallback.
pub fn item_error_text(code: u8) -> &'static str {
    match code {
        0x00 => "reserved",
        0x01 => "hardware error",
        0x03 => "access denied",
        0x05 => "invalid address",
        0x06 => "unsupported data type",
        0x07 => "inconsistent data type",
        0x0a => "object does not exist",
        0x10 => "invalid block type",
        0x11 => "block not found",
        0x12 => "block already exists",
        0x13 => "protected block",
        0x14 => "download requires initial block",
        0x19 => "download sequence error",
        0x1a => "insufficient working memory",
        0x1b => "insufficient load memory",
        0x1c => "insufficient retentive memory",
        0x1d => "interface error",
        0x1e => "delete refused",
        0x20 => "invalid parameter",
        0x21 => "connection resources exhausted",
        0xff => "success",
        _ => "unknown error",
    }
}
