//! AV1 still-frame decoding through `rav1d`'s Rust API.

use rav1d::{Decoder, PixelLayout, PlanarImageComponent, Rav1dError, Settings};

use crate::{Error, Result};

/// Chroma subsampling of a decoded picture.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) enum Layout {
    I400,
    I420,
    I422,
    I444,
}

/// CICP signalled in the AV1 sequence header.
#[derive(Copy, Clone, Debug)]
pub(super) struct SeqColor {
    pub primaries: u8,
    pub transfer: u8,
    pub matrix: u8,
    pub full_range: bool,
}

/// A decoded AV1 frame.
pub(super) struct Picture(rav1d::Picture);

impl Picture {
    pub fn width(&self) -> u32 {
        self.0.width()
    }

    pub fn height(&self) -> u32 {
        self.0.height()
    }

    /// Bits per sample: 8, 10 or 12.
    pub fn bit_depth(&self) -> u8 {
        self.0.bits_per_component().map_or(8, |b| b.0)
    }

    pub fn layout(&self) -> Layout {
        match self.0.pixel_layout() {
            PixelLayout::I400 => Layout::I400,
            PixelLayout::I420 => Layout::I420,
            PixelLayout::I422 => Layout::I422,
            PixelLayout::I444 => Layout::I444,
        }
    }

    pub fn color(&self) -> SeqColor {
        SeqColor {
            primaries: self.0.color_primaries() as u8,
            transfer: self.0.transfer_characteristic() as u8,
            matrix: self.0.matrix_coefficients() as u8,
            full_range: self.0.color_range() == rav1d::pixel::YUVRange::Full,
        }
    }

    /// Raw bytes and stride in bytes of plane `idx` (0 = Y, 1 = U,
    /// 2 = V).
    ///
    /// Samples are `u8` at 8 bits per channel and native-endian `u16`
    /// above that.
    pub fn plane(&self, idx: usize) -> (&[u8], usize) {
        if idx > 0 && self.layout() == Layout::I400 {
            return (&[], 0);
        }
        let component = match idx {
            0 => PlanarImageComponent::Y,
            1 => PlanarImageComponent::U,
            _ => PlanarImageComponent::V,
        };
        (self.0.plane(component), self.0.stride(component) as usize)
    }
}

/// Decode one AV1 still frame assembled from `extents`.
///
/// `max_pixels` is forwarded to rav1d's frame size limit so oversized
/// frames are rejected before any plane is allocated.
pub(super) fn decode(extents: &[&[u8]], max_pixels: u64) -> Result<Picture> {
    let mut settings = Settings::new();
    settings.set_n_threads(1);
    settings.set_max_frame_delay(1);
    settings.set_frame_size_limit(max_pixels.min(u32::MAX as u64) as u32);
    settings.set_logging(false);
    let mut decoder = Decoder::with_settings(&settings).map_err(map_err)?;

    let data: Box<[u8]> = extents.concat().into();
    if data.is_empty() {
        return Err(Error::Malformed("empty av1 item".into()));
    }
    match decoder.send_data(data, None, None, None) {
        Ok(()) | Err(Rav1dError::TryAgain) => {}
        Err(e) => return Err(map_err(e)),
    }

    let picture = decoder.get_picture().map_err(map_err)?;
    Ok(Picture(picture))
}

fn map_err(e: Rav1dError) -> Error {
    match e {
        Rav1dError::OutOfRange => Error::LimitExceeded("max_pixels"),
        e => Error::Decoder {
            format: "avif",
            message: format!("av1 decode failed: {e}"),
        },
    }
}
