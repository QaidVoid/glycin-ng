//! JPEG decoder backed by the `jpeg-decoder` crate.

use std::io::Cursor;

use jpeg_decoder::PixelFormat;

use crate::{Error, Frame, Image, MemoryFormat, Result, Texture};

use super::DecodeOptions;

pub(crate) fn decode(bytes: &[u8], opts: &DecodeOptions) -> Result<Image> {
    let mut decoder = jpeg_decoder::Decoder::new(Cursor::new(bytes));
    let pixels = decoder.decode().map_err(map_err)?;
    let info = decoder.info().ok_or_else(|| Error::Decoder {
        format: "jpeg",
        message: "no image info after decode".into(),
    })?;
    let width = info.width as u32;
    let height = info.height as u32;
    opts.limits.check_dimensions(width, height, 1)?;

    let format = match info.pixel_format {
        PixelFormat::L8 => MemoryFormat::G8,
        PixelFormat::L16 => MemoryFormat::G16,
        PixelFormat::RGB24 => MemoryFormat::R8g8b8,
        PixelFormat::CMYK32 => {
            return Err(Error::Decoder {
                format: "jpeg",
                message: "CMYK pixel format is not supported".into(),
            });
        }
    };

    let stride = (format.bytes_per_pixel() as u64)
        .checked_mul(width as u64)
        .filter(|s| *s <= u32::MAX as u64)
        .ok_or(Error::LimitExceeded("stride"))? as u32;

    let mut pixels = pixels;
    if matches!(info.pixel_format, PixelFormat::L16) {
        be_to_native_u16(&mut pixels);
    }

    let texture = Texture::from_parts(width, height, stride, format, pixels.into_boxed_slice())
        .ok_or_else(|| Error::Decoder {
            format: "jpeg",
            message: "texture construction failed".into(),
        })?;

    let mut image = Image::from_parts("jpeg", width, height, vec![Frame::new(texture, None)]);
    if let Some(profile) = decoder.icc_profile() {
        image.set_icc_profile(profile);
    }
    let _ = opts.apply_transformations;
    Ok(image)
}

fn map_err(e: jpeg_decoder::Error) -> Error {
    use jpeg_decoder::Error as E;
    match e {
        E::Io(io) => Error::Io(io),
        E::Format(msg) => Error::Malformed(msg),
        E::Internal(_) => Error::Decoder {
            format: "jpeg",
            message: e.to_string(),
        },
        E::Unsupported(feature) => Error::Decoder {
            format: "jpeg",
            message: format!("unsupported jpeg feature: {feature:?}"),
        },
    }
}

fn be_to_native_u16(buf: &mut [u8]) {
    if cfg!(target_endian = "little") {
        for pair in buf.as_chunks_mut::<2>().0 {
            pair.swap(0, 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_garbage() {
        let opts = DecodeOptions {
            limits: crate::Limits::default(),
            apply_transformations: true,
            render_size_hint: None,
        };
        let err = decode(b"not a jpeg", &opts).unwrap_err();
        assert!(matches!(err, Error::Malformed(_) | Error::Io(_)));
    }

    #[test]
    fn rejects_empty_input() {
        let opts = DecodeOptions {
            limits: crate::Limits::default(),
            apply_transformations: true,
            render_size_hint: None,
        };
        let err = decode(b"", &opts).unwrap_err();
        assert!(matches!(err, Error::Malformed(_) | Error::Io(_)));
    }
}
