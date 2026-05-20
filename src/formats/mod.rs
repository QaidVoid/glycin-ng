//! Format-specific decoders.
//!
//! Each decoder is gated by its Cargo feature and is wired into
//! [`dispatch`] once implemented. Formats whose feature is off, or
//! whose decoder has not yet been wired, return
//! [`Error::UnsupportedFormat`](crate::Error::UnsupportedFormat).

use crate::sniff::KnownFormat;
use crate::{Error, Image, Limits, Result};

/// Decoder input options shared across formats.
#[allow(dead_code)]
pub(crate) struct DecodeOptions {
    pub limits: Limits,
    pub apply_transformations: bool,
}

/// Route a sniffed format to its decoder.
pub(crate) fn dispatch(
    format: KnownFormat,
    bytes: &[u8],
    opts: &DecodeOptions,
) -> Result<Image> {
    let _ = (bytes, opts);
    let _ = format;
    Err(Error::UnsupportedFormat)
}
