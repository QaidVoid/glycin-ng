//! PNG / APNG decoder backed by the `png` crate.

use std::io::Cursor;
use std::time::Duration;

use png::{BitDepth, ColorType, DecodingError, Transformations};

use crate::{Error, Frame, Image, MemoryFormat, Result, Texture};

use super::DecodeOptions;

pub(crate) fn decode(bytes: &[u8], opts: &DecodeOptions) -> Result<Image> {
    let max_bytes = opts
        .limits
        .decode_memory_mib
        .saturating_mul(1024 * 1024)
        .min(usize::MAX as u64) as usize;

    let mut decoder =
        png::Decoder::new_with_limits(Cursor::new(bytes), png::Limits { bytes: max_bytes });
    decoder.set_transformations(Transformations::EXPAND);
    decoder.set_ignore_text_chunk(true);

    let mut reader = decoder.read_info().map_err(map_err)?;
    let (out_color, out_depth) = reader.output_color_type();
    let width = reader.info().width;
    let height = reader.info().height;
    let frame_count = reader
        .info()
        .animation_control
        .map_or(1, |ac| ac.num_frames.max(1));

    opts.limits.check_dimensions(width, height, frame_count)?;

    let format = map_format(out_color, out_depth).ok_or_else(|| Error::Decoder {
        format: "png",
        message: format!("unsupported color/depth: {out_color:?}/{out_depth:?}"),
    })?;

    let icc = reader.info().icc_profile.as_ref().map(|c| c.to_vec());
    let exif = reader.info().exif_metadata.as_ref().map(|c| c.to_vec());

    let bytes_per_pixel = format.bytes_per_pixel() as u64;

    let mut frames = Vec::with_capacity(frame_count as usize);
    for _ in 0..frame_count {
        let buffer_size = reader.output_buffer_size().ok_or_else(|| Error::Decoder {
            format: "png",
            message: "decoder reported no buffer size".into(),
        })?;
        let mut buf = vec![0u8; buffer_size];
        let output_info = reader.next_frame(&mut buf).map_err(map_err)?;

        let frame_w = output_info.width;
        let frame_h = output_info.height;
        buf.truncate(output_info.buffer_size());

        if matches!(out_depth, BitDepth::Sixteen) {
            be_to_native_u16(&mut buf);
        }

        let frame_stride = bytes_per_pixel
            .checked_mul(frame_w as u64)
            .filter(|s| *s <= u32::MAX as u64)
            .ok_or(Error::LimitExceeded("stride"))? as u32;

        let texture = Texture::from_parts(
            frame_w,
            frame_h,
            frame_stride,
            format,
            buf.into_boxed_slice(),
        )
        .ok_or_else(|| Error::Decoder {
            format: "png",
            message: "texture construction failed".into(),
        })?;

        let delay = reader.info().frame_control.map(|fc| {
            let den = fc.delay_den as u64;
            let num = fc.delay_num as u64;
            let denom = if den == 0 { 100 } else { den };
            Duration::from_micros(num.saturating_mul(1_000_000) / denom)
        });

        frames.push(Frame::new(texture, delay));
    }

    let mut image = Image::from_parts("png", width, height, frames);
    if let Some(profile) = icc {
        image.set_icc_profile(profile);
    }
    if let Some(e) = exif {
        image.set_exif(e);
    }

    let _ = opts.apply_transformations;
    Ok(image)
}

fn map_format(color: ColorType, depth: BitDepth) -> Option<MemoryFormat> {
    match (color, depth) {
        (ColorType::Grayscale, BitDepth::Eight) => Some(MemoryFormat::G8),
        (ColorType::Grayscale, BitDepth::Sixteen) => Some(MemoryFormat::G16),
        (ColorType::GrayscaleAlpha, BitDepth::Eight) => Some(MemoryFormat::G8a8),
        (ColorType::GrayscaleAlpha, BitDepth::Sixteen) => Some(MemoryFormat::G16a16),
        (ColorType::Rgb, BitDepth::Eight) => Some(MemoryFormat::R8g8b8),
        (ColorType::Rgb, BitDepth::Sixteen) => Some(MemoryFormat::R16g16b16),
        (ColorType::Rgba, BitDepth::Eight) => Some(MemoryFormat::R8g8b8a8),
        (ColorType::Rgba, BitDepth::Sixteen) => Some(MemoryFormat::R16g16b16a16),
        _ => None,
    }
}

fn map_err(e: DecodingError) -> Error {
    match e {
        DecodingError::IoError(e) => Error::Io(e),
        DecodingError::LimitsExceeded => Error::LimitExceeded("png internal"),
        DecodingError::Format(_) | DecodingError::Parameter(_) => Error::Malformed(e.to_string()),
    }
}

fn be_to_native_u16(buf: &mut [u8]) {
    if cfg!(target_endian = "little") {
        for pair in buf.chunks_exact_mut(2) {
            pair.swap(0, 1);
        }
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

    fn opts_with(limits: Limits) -> DecodeOptions {
        DecodeOptions {
            limits,
            apply_transformations: true,
        }
    }

    fn make_png(width: u32, height: u32, color: ColorType, depth: BitDepth) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, width, height);
            enc.set_color(color);
            enc.set_depth(depth);
            let mut writer = enc.write_header().unwrap();
            let samples = color.samples();
            let bytes_per_sample = match depth {
                BitDepth::Eight => 1,
                BitDepth::Sixteen => 2,
                _ => 1,
            };
            let row_bytes = (width as usize) * samples * bytes_per_sample;
            let data = vec![0x80; row_bytes * height as usize];
            writer.write_image_data(&data).unwrap();
        }
        out
    }

    #[test]
    fn decodes_4x4_rgba8() {
        let bytes = make_png(4, 4, ColorType::Rgba, BitDepth::Eight);
        let image = decode(&bytes, &opts()).unwrap();
        assert_eq!(image.width(), 4);
        assert_eq!(image.height(), 4);
        assert_eq!(image.frames().len(), 1);
        let frame = image.first_frame().unwrap();
        assert_eq!(frame.texture().format(), MemoryFormat::R8g8b8a8);
        assert_eq!(frame.texture().data().len(), 4 * 4 * 4);
    }

    #[test]
    fn decodes_grayscale_8() {
        let bytes = make_png(8, 8, ColorType::Grayscale, BitDepth::Eight);
        let image = decode(&bytes, &opts()).unwrap();
        let frame = image.first_frame().unwrap();
        assert_eq!(frame.texture().format(), MemoryFormat::G8);
        assert_eq!(frame.texture().data().len(), 8 * 8);
    }

    #[test]
    fn decodes_rgba_16() {
        let bytes = make_png(2, 2, ColorType::Rgba, BitDepth::Sixteen);
        let image = decode(&bytes, &opts()).unwrap();
        let frame = image.first_frame().unwrap();
        assert_eq!(frame.texture().format(), MemoryFormat::R16g16b16a16);
        assert_eq!(frame.texture().data().len(), 2 * 2 * 8);
    }

    #[test]
    fn rejects_oversized_width() {
        let bytes = make_png(10, 10, ColorType::Rgba, BitDepth::Eight);
        let limits = Limits {
            max_width: 5,
            ..Limits::default()
        };
        let err = decode(&bytes, &opts_with(limits)).unwrap_err();
        assert!(matches!(err, Error::LimitExceeded("max_width")));
    }

    #[test]
    fn rejects_oversized_pixels() {
        let bytes = make_png(64, 64, ColorType::Rgba, BitDepth::Eight);
        let limits = Limits {
            max_pixels: 1024,
            ..Limits::default()
        };
        let err = decode(&bytes, &opts_with(limits)).unwrap_err();
        assert!(matches!(err, Error::LimitExceeded("max_pixels")));
    }

    #[test]
    fn rejects_truncated() {
        let mut bytes = make_png(4, 4, ColorType::Rgba, BitDepth::Eight);
        bytes.truncate(bytes.len() / 2);
        let err = decode(&bytes, &opts()).unwrap_err();
        assert!(matches!(err, Error::Malformed(_) | Error::Io(_)));
    }

    #[test]
    fn rejects_garbage() {
        let err = decode(b"not a png", &opts()).unwrap_err();
        assert!(matches!(err, Error::Malformed(_) | Error::Io(_)));
    }

    #[test]
    fn rejects_corrupt_header() {
        let mut bytes = make_png(4, 4, ColorType::Rgba, BitDepth::Eight);
        // Flip a byte in the IHDR chunk payload (after the 8-byte
        // PNG signature + 4-byte length + 4-byte "IHDR" tag).
        bytes[16] = bytes[16].wrapping_add(7);
        let err = decode(&bytes, &opts()).unwrap_err();
        assert!(matches!(err, Error::Malformed(_)));
    }

    #[test]
    fn empty_input_errors() {
        let err = decode(b"", &opts()).unwrap_err();
        assert!(matches!(err, Error::Malformed(_) | Error::Io(_)));
    }

    #[test]
    fn texture_stride_matches_format() {
        let bytes = make_png(7, 5, ColorType::Rgb, BitDepth::Eight);
        let image = decode(&bytes, &opts()).unwrap();
        let texture = image.first_frame().unwrap().texture();
        assert_eq!(texture.stride(), 7 * 3);
        assert_eq!(texture.width(), 7);
        assert_eq!(texture.height(), 5);
    }
}
