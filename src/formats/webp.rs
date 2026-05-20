//! WebP decoder backed by the `image-webp` crate.

use std::io::Cursor;
use std::time::Duration;

use crate::{Error, Frame, Image, MemoryFormat, Result, Texture};

use super::DecodeOptions;

pub(crate) fn decode(bytes: &[u8], opts: &DecodeOptions) -> Result<Image> {
    let mut decoder = image_webp::WebPDecoder::new(Cursor::new(bytes)).map_err(map_err)?;
    let (width, height) = decoder.dimensions();
    let has_alpha = decoder.has_alpha();
    let format = if has_alpha {
        MemoryFormat::R8g8b8a8
    } else {
        MemoryFormat::R8g8b8
    };

    let frame_count = if decoder.is_animated() {
        decoder.num_frames().max(1)
    } else {
        1
    };
    opts.limits.check_dimensions(width, height, frame_count)?;

    let icc = decoder.icc_profile().map_err(map_err)?;
    let exif = decoder.exif_metadata().map_err(map_err)?;

    let bytes_per_pixel = format.bytes_per_pixel() as u64;
    let stride = bytes_per_pixel
        .checked_mul(width as u64)
        .filter(|s| *s <= u32::MAX as u64)
        .ok_or(Error::LimitExceeded("stride"))? as u32;

    let mut frames = Vec::with_capacity(frame_count as usize);
    let mut total_animation = Duration::ZERO;

    if decoder.is_animated() {
        for _ in 0..frame_count {
            let buf_size = decoder.output_buffer_size().ok_or_else(|| Error::Decoder {
                format: "webp",
                message: "no buffer size".into(),
            })?;
            let mut buf = vec![0u8; buf_size];
            let delay_ms = decoder.read_frame(&mut buf).map_err(map_err)?;
            let delay = Duration::from_millis(delay_ms as u64);
            total_animation = total_animation.saturating_add(delay);
            if total_animation > opts.limits.max_animation_duration {
                return Err(Error::LimitExceeded("max_animation_duration"));
            }
            let texture =
                Texture::from_parts(width, height, stride, format, buf.into_boxed_slice())
                    .ok_or_else(|| Error::Decoder {
                        format: "webp",
                        message: "texture construction failed".into(),
                    })?;
            frames.push(Frame::new(texture, Some(delay)));
        }
    } else {
        let buf_size = decoder.output_buffer_size().ok_or_else(|| Error::Decoder {
            format: "webp",
            message: "no buffer size".into(),
        })?;
        let mut buf = vec![0u8; buf_size];
        decoder.read_image(&mut buf).map_err(map_err)?;
        let texture = Texture::from_parts(width, height, stride, format, buf.into_boxed_slice())
            .ok_or_else(|| Error::Decoder {
                format: "webp",
                message: "texture construction failed".into(),
            })?;
        frames.push(Frame::new(texture, None));
    }

    let mut image = Image::from_parts("webp", width, height, frames);
    if let Some(profile) = icc {
        image.set_icc_profile(profile);
    }
    if let Some(e) = exif {
        image.set_exif(e);
    }

    let _ = opts.apply_transformations;
    Ok(image)
}

fn map_err(e: image_webp::DecodingError) -> Error {
    match e {
        image_webp::DecodingError::IoError(io) => Error::Io(io),
        image_webp::DecodingError::MemoryLimitExceeded => Error::LimitExceeded("webp memory limit"),
        other => Error::Malformed(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Limits;

    fn opts() -> DecodeOptions {
        DecodeOptions {
            limits: Limits::default(),
            apply_transformations: true,
        }
    }

    #[test]
    fn rejects_garbage() {
        let err = decode(b"RIFF\0\0\0\0WEBPnonsense", &opts()).unwrap_err();
        assert!(matches!(err, Error::Malformed(_) | Error::Io(_)));
    }

    #[test]
    fn rejects_empty() {
        let err = decode(b"", &opts()).unwrap_err();
        assert!(matches!(err, Error::Malformed(_) | Error::Io(_)));
    }
}
