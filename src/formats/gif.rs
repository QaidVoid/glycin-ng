//! GIF decoder backed by the `gif` crate.

use std::io::Cursor;
use std::time::Duration;

use crate::{Error, Frame, Image, MemoryFormat, Result, Texture};

use super::DecodeOptions;

pub(crate) fn decode(bytes: &[u8], opts: &DecodeOptions) -> Result<Image> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mem_bytes = opts
        .limits
        .decode_memory_mib
        .saturating_mul(1024 * 1024)
        .max(1);
    let nonzero = std::num::NonZeroU64::new(mem_bytes)
        .unwrap_or_else(|| std::num::NonZeroU64::new(1).unwrap());
    options.set_memory_limit(gif::MemoryLimit::Bytes(nonzero));

    let mut decoder = options.read_info(Cursor::new(bytes)).map_err(map_err)?;
    let width = decoder.width() as u32;
    let height = decoder.height() as u32;

    let mut frames = Vec::new();
    let mut total_pixels: u64 = 0;
    let mut total_animation = Duration::ZERO;

    while let Some(frame) = decoder.read_next_frame().map_err(map_err)? {
        let fw = frame.width as u32;
        let fh = frame.height as u32;
        let pixels = frame
            .width
            .checked_mul(frame.height)
            .ok_or(Error::LimitExceeded("frame size"))? as u64;
        total_pixels = total_pixels
            .checked_add(pixels)
            .ok_or(Error::LimitExceeded("max_pixels"))?;
        if total_pixels > opts.limits.max_pixels {
            return Err(Error::LimitExceeded("max_pixels"));
        }
        if frames.len() as u32 >= opts.limits.max_frames {
            return Err(Error::LimitExceeded("max_frames"));
        }

        let buffer = frame.buffer.to_vec();
        let stride = fw.checked_mul(4).ok_or(Error::LimitExceeded("stride"))?;
        let texture = Texture::from_parts(
            fw,
            fh,
            stride,
            MemoryFormat::R8g8b8a8,
            buffer.into_boxed_slice(),
        )
        .ok_or_else(|| Error::Decoder {
            format: "gif",
            message: "texture construction failed".into(),
        })?;

        let delay = Duration::from_millis(frame.delay as u64 * 10);
        total_animation = total_animation.saturating_add(delay);
        if total_animation > opts.limits.max_animation_duration {
            return Err(Error::LimitExceeded("max_animation_duration"));
        }

        frames.push(Frame::new(texture, Some(delay)));
    }

    if frames.is_empty() {
        return Err(Error::Malformed("gif contained no frames".into()));
    }

    let _ = opts.apply_transformations;
    Ok(Image::from_parts("gif", width, height, frames))
}

fn map_err(e: gif::DecodingError) -> Error {
    match e {
        gif::DecodingError::Io(io) => Error::Io(io),
        gif::DecodingError::Format(msg) => Error::Malformed(msg.to_string()),
        other => Error::Decoder {
            format: "gif",
            message: other.to_string(),
        },
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

    fn make_gif_2x2() -> Vec<u8> {
        let mut out = Vec::new();
        let palette: &[u8] = &[0, 0, 0, 255, 255, 255];
        let frame = gif::Frame {
            width: 2,
            height: 2,
            buffer: std::borrow::Cow::Owned(vec![0, 1, 1, 0]),
            ..gif::Frame::default()
        };
        let mut encoder = gif::Encoder::new(&mut out, 2, 2, palette).unwrap();
        encoder.write_frame(&frame).unwrap();
        drop(encoder);
        out
    }

    #[test]
    fn decodes_single_frame() {
        let bytes = make_gif_2x2();
        let image = decode(&bytes, &opts()).unwrap();
        assert_eq!(image.width(), 2);
        assert_eq!(image.height(), 2);
        assert_eq!(image.frames().len(), 1);
        let frame = image.first_frame().unwrap();
        assert_eq!(frame.texture().format(), MemoryFormat::R8g8b8a8);
    }

    #[test]
    fn rejects_garbage() {
        let err = decode(b"GIF89aGARBAGE", &opts()).unwrap_err();
        assert!(matches!(
            err,
            Error::Malformed(_) | Error::Io(_) | Error::Decoder { .. }
        ));
    }
}
