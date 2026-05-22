//! Crate-wide error type.

use std::io;

/// Errors returned by the public API.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum Error {
    /// I/O failed while reading the image source.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// Image source did not match any enabled decoder.
    #[error("unsupported image format")]
    UnsupportedFormat,

    /// A required sandbox layer could not be applied and the caller
    /// asked for strict enforcement via
    /// [`Loader::require_sandbox`](crate::Loader::require_sandbox).
    #[error("requested sandbox layer unavailable: {0}")]
    SandboxUnavailable(&'static str),

    /// A configured [`Limits`](crate::Limits) value was exceeded.
    #[error("limit exceeded: {0}")]
    LimitExceeded(&'static str),

    /// Input ended before the decoder finished parsing.
    #[error("input truncated: {0}")]
    Truncated(&'static str),

    /// Input was syntactically invalid for the detected format.
    #[error("malformed input: {0}")]
    Malformed(String),

    /// An enabled decoder reported an error specific to its format.
    #[error("{format} decoder error: {message}")]
    Decoder {
        /// Format-name string (e.g. `"png"`, `"jpeg"`).
        format: &'static str,
        /// Decoder-provided message.
        message: String,
    },

    /// An enabled encoder reported an error specific to its format.
    #[error("{format} encoder error: {message}")]
    Encoder {
        /// Format-name string (e.g. `"png"`, `"jpeg"`).
        format: &'static str,
        /// Encoder-provided message.
        message: String,
    },

    /// Internal invariant violation. Should never escape; please
    /// report it.
    #[error("internal error: {0}")]
    Internal(String),
}

/// Convenience alias for `Result<T, Error>`.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_format() {
        let e = Error::UnsupportedFormat;
        assert_eq!(e.to_string(), "unsupported image format");

        let e = Error::Decoder {
            format: "png",
            message: "bad chunk".into(),
        };
        assert_eq!(e.to_string(), "png decoder error: bad chunk");

        let e = Error::LimitExceeded("max_pixels");
        assert_eq!(e.to_string(), "limit exceeded: max_pixels");
    }

    #[test]
    fn io_error_converts() {
        let ioe = io::Error::new(io::ErrorKind::UnexpectedEof, "short");
        let e: Error = ioe.into();
        assert!(matches!(e, Error::Io(_)));
    }
}
