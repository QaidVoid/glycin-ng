//! Format-specific decoders.
//!
//! Each decoder is gated by its Cargo feature and is wired into
//! [`dispatch`] once implemented. Formats whose feature is off return
//! [`Error::UnsupportedFormat`](crate::Error::UnsupportedFormat).

#[cfg(any(
    feature = "bmp",
    feature = "ico",
    feature = "tga",
    feature = "pnm",
    feature = "dds",
    feature = "exr",
))]
mod image_rs;

#[cfg(feature = "bmp")]
mod bmp;
#[cfg(feature = "dds")]
mod dds;
#[cfg(feature = "exr")]
mod exr;
#[cfg(feature = "gif")]
mod gif;
#[cfg(feature = "ico")]
mod ico;
#[cfg(feature = "jpeg")]
mod jpeg;
#[cfg(feature = "jxl")]
mod jxl;
#[cfg(feature = "png")]
mod png;
#[cfg(feature = "pnm")]
mod pnm;
#[cfg(feature = "qoi")]
mod qoi;
#[cfg(feature = "svg")]
mod svg;
#[cfg(feature = "tga")]
mod tga;
#[cfg(feature = "tiff")]
mod tiff;
#[cfg(feature = "webp")]
mod webp;

use crate::sniff::KnownFormat;
use crate::{Error, Image, Limits, Result};

/// Decoder input options shared across formats.
#[allow(dead_code)]
pub(crate) struct DecodeOptions {
    pub limits: Limits,
    pub apply_transformations: bool,
}

/// Route a sniffed format to its decoder.
pub(crate) fn dispatch(format: KnownFormat, bytes: &[u8], opts: &DecodeOptions) -> Result<Image> {
    match format {
        #[cfg(feature = "png")]
        KnownFormat::Png => png::decode(bytes, opts),
        #[cfg(feature = "jpeg")]
        KnownFormat::Jpeg => jpeg::decode(bytes, opts),
        #[cfg(feature = "gif")]
        KnownFormat::Gif => gif::decode(bytes, opts),
        #[cfg(feature = "webp")]
        KnownFormat::WebP => webp::decode(bytes, opts),
        #[cfg(feature = "tiff")]
        KnownFormat::Tiff => tiff::decode(bytes, opts),
        #[cfg(feature = "bmp")]
        KnownFormat::Bmp => bmp::decode(bytes, opts),
        #[cfg(feature = "ico")]
        KnownFormat::Ico => ico::decode(bytes, opts),
        #[cfg(feature = "tga")]
        KnownFormat::Tga => tga::decode(bytes, opts),
        #[cfg(feature = "qoi")]
        KnownFormat::Qoi => qoi::decode(bytes, opts),
        #[cfg(feature = "exr")]
        KnownFormat::Exr => exr::decode(bytes, opts),
        #[cfg(feature = "pnm")]
        KnownFormat::Pnm => pnm::decode(bytes, opts),
        #[cfg(feature = "dds")]
        KnownFormat::Dds => dds::decode(bytes, opts),
        #[cfg(feature = "jxl")]
        KnownFormat::Jxl => jxl::decode(bytes, opts),
        #[cfg(feature = "svg")]
        KnownFormat::Svg => svg::decode(bytes, opts),
        #[allow(unreachable_patterns)]
        _ => {
            let _ = (bytes, opts);
            Err(Error::UnsupportedFormat)
        }
    }
}
