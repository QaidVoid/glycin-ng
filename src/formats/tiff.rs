//! TIFF decoder backed by the `tiff` crate.

use std::io::Cursor;

use tiff::{ColorType, decoder::DecodingResult};

use crate::{Error, Frame, Image, MemoryFormat, Result, Texture};

use super::DecodeOptions;

pub(crate) fn decode(bytes: &[u8], opts: &DecodeOptions) -> Result<Image> {
    let mut decoder = tiff::decoder::Decoder::new(Cursor::new(bytes)).map_err(map_err)?;
    let (width, height) = decoder.dimensions().map_err(map_err)?;
    opts.limits.check_dimensions(width, height, 1)?;

    let color = decoder.colortype().map_err(map_err)?;
    let format = match color {
        ColorType::Gray(8) => MemoryFormat::G8,
        ColorType::Gray(16) => MemoryFormat::G16,
        ColorType::GrayA(8) => MemoryFormat::G8a8,
        ColorType::GrayA(16) => MemoryFormat::G16a16,
        ColorType::RGB(8) => MemoryFormat::R8g8b8,
        ColorType::RGB(16) => MemoryFormat::R16g16b16,
        ColorType::RGBA(8) => MemoryFormat::R8g8b8a8,
        ColorType::RGBA(16) => MemoryFormat::R16g16b16a16,
        other => {
            return Err(Error::Decoder {
                format: "tiff",
                message: format!("unsupported color type: {other:?}"),
            });
        }
    };

    let result = decoder.read_image().map_err(map_err)?;
    let bytes_vec = match result {
        DecodingResult::U8(v) => v,
        DecodingResult::U16(v) => u16_to_native_bytes(v),
        other => {
            return Err(Error::Decoder {
                format: "tiff",
                message: format!("unsupported sample format: {other:?}"),
            });
        }
    };

    let stride = (format.bytes_per_pixel() as u64)
        .checked_mul(width as u64)
        .filter(|s| *s <= u32::MAX as u64)
        .ok_or(Error::LimitExceeded("stride"))? as u32;

    let texture = Texture::from_parts(width, height, stride, format, bytes_vec.into_boxed_slice())
        .ok_or_else(|| Error::Decoder {
            format: "tiff",
            message: "texture construction failed".into(),
        })?;

    let _ = opts.apply_transformations;
    Ok(Image::from_parts(
        "tiff",
        width,
        height,
        vec![Frame::new(texture, None)],
    ))
}

fn u16_to_native_bytes(v: Vec<u16>) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 2);
    for sample in v {
        out.extend_from_slice(&sample.to_ne_bytes());
    }
    out
}

fn map_err(e: tiff::TiffError) -> Error {
    use tiff::TiffError as E;
    match e {
        E::IoError(io) => Error::Io(io),
        E::LimitsExceeded => Error::LimitExceeded("tiff internal"),
        E::FormatError(_) | E::IntSizeError | E::UsageError(_) => Error::Malformed(e.to_string()),
        E::UnsupportedError(_) => Error::Decoder {
            format: "tiff",
            message: e.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Limits;

    #[test]
    fn rejects_garbage() {
        let opts = DecodeOptions {
            limits: Limits::default(),
            apply_transformations: true,
        };
        let err = decode(b"II*\0garbage", &opts).unwrap_err();
        assert!(matches!(
            err,
            Error::Malformed(_) | Error::Io(_) | Error::Decoder { .. }
        ));
    }
}
