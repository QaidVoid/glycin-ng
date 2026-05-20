//! JPEG XL decoder backed by the `jxl-oxide` crate.

use std::io::Cursor;
use std::time::Duration;

use jxl_oxide::{JxlImage, PixelFormat};

use crate::{Error, Frame, Image, MemoryFormat, Result, Texture};

use super::DecodeOptions;

pub(crate) fn decode(bytes: &[u8], opts: &DecodeOptions) -> Result<Image> {
    let jxl = JxlImage::builder()
        .read(Cursor::new(bytes))
        .map_err(|e| Error::Decoder {
            format: "jxl",
            message: e.to_string(),
        })?;
    let width = jxl.width();
    let height = jxl.height();
    let num_keyframes = jxl.num_loaded_keyframes().max(1) as u32;
    opts.limits.check_dimensions(width, height, num_keyframes)?;

    let (format, channels) = match jxl.pixel_format() {
        PixelFormat::Gray => (MemoryFormat::R32g32b32a32Float, 1),
        PixelFormat::Graya => (MemoryFormat::R32g32b32a32Float, 2),
        PixelFormat::Rgb => (MemoryFormat::R32g32b32Float, 3),
        PixelFormat::Rgba => (MemoryFormat::R32g32b32a32Float, 4),
        other => {
            return Err(Error::Decoder {
                format: "jxl",
                message: format!("unsupported pixel format: {other:?}"),
            });
        }
    };
    let _ = channels;

    let pixel_count = (width as u64)
        .checked_mul(height as u64)
        .ok_or(Error::LimitExceeded("max_pixels"))? as usize;
    let total_floats = pixel_count
        .checked_mul(4)
        .ok_or(Error::LimitExceeded("buffer size"))?;

    let mut frames = Vec::with_capacity(num_keyframes as usize);
    let mut total_animation = Duration::ZERO;

    for idx in 0..num_keyframes as usize {
        let render = jxl.render_frame(idx).map_err(|e| Error::Decoder {
            format: "jxl",
            message: e.to_string(),
        })?;

        let mut buf = vec![0.0_f32; total_floats];
        let mut stream = render.stream();
        let _written = stream.write_to_buffer(&mut buf);

        let bytes_vec = floats_to_bytes(buf);
        let stride = (format.bytes_per_pixel() as u64)
            .checked_mul(width as u64)
            .filter(|s| *s <= u32::MAX as u64)
            .ok_or(Error::LimitExceeded("stride"))? as u32;

        let texture =
            Texture::from_parts(width, height, stride, format, bytes_vec.into_boxed_slice())
                .ok_or_else(|| Error::Decoder {
                    format: "jxl",
                    message: "texture construction failed".into(),
                })?;

        let delay = if num_keyframes > 1 {
            let header = jxl.frame_header(idx);
            let duration_ticks = header.map(|h| h.duration as u64).unwrap_or(0);
            let tps = jxl
                .image_header()
                .metadata
                .animation
                .as_ref()
                .map(|a| {
                    let num = a.tps_numerator.max(1) as u64;
                    let denom = a.tps_denominator.max(1) as u64;
                    (num, denom)
                })
                .unwrap_or((1, 1));
            let micros = duration_ticks
                .saturating_mul(1_000_000)
                .saturating_mul(tps.1)
                / tps.0.max(1);
            let d = Duration::from_micros(micros);
            total_animation = total_animation.saturating_add(d);
            if total_animation > opts.limits.max_animation_duration {
                return Err(Error::LimitExceeded("max_animation_duration"));
            }
            Some(d)
        } else {
            None
        };

        frames.push(Frame::new(texture, delay));
    }

    let mut image = Image::from_parts("jxl", width, height, frames);
    if let Some(icc) = jxl.original_icc() {
        image.set_icc_profile(icc.to_vec());
    }

    let _ = opts.apply_transformations;
    Ok(image)
}

fn floats_to_bytes(v: Vec<f32>) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for sample in v {
        out.extend_from_slice(&sample.to_ne_bytes());
    }
    out
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
        let err = decode(b"not a jxl", &opts).unwrap_err();
        assert!(matches!(
            err,
            Error::Decoder { .. } | Error::Malformed(_) | Error::Io(_)
        ));
    }
}
