//! Shared helper for decoders backed by the `image` crate.

use image::{DynamicImage, ImageFormat};

use crate::{Error, Frame, Image, MemoryFormat, Result, Texture};

use super::DecodeOptions;

pub(crate) fn decode_with(
    format_name: &'static str,
    image_format: ImageFormat,
    bytes: &[u8],
    opts: &DecodeOptions,
) -> Result<Image> {
    let dynamic = image::load_from_memory_with_format(bytes, image_format).map_err(map_err)?;
    dynamic_into_image(format_name, dynamic, opts)
}

fn dynamic_into_image(
    format_name: &'static str,
    dynamic: DynamicImage,
    opts: &DecodeOptions,
) -> Result<Image> {
    let width = dynamic.width();
    let height = dynamic.height();
    opts.limits.check_dimensions(width, height, 1)?;

    let (format, bytes) = into_native(dynamic);
    let stride = (format.bytes_per_pixel() as u64)
        .checked_mul(width as u64)
        .filter(|s| *s <= u32::MAX as u64)
        .ok_or(Error::LimitExceeded("stride"))? as u32;

    let texture = Texture::from_parts(width, height, stride, format, bytes.into_boxed_slice())
        .ok_or_else(|| Error::Decoder {
            format: format_name,
            message: "texture construction failed".into(),
        })?;

    let _ = opts.apply_transformations;
    Ok(Image::from_parts(
        format_name,
        width,
        height,
        vec![Frame::new(texture, None)],
    ))
}

fn into_native(dynamic: DynamicImage) -> (MemoryFormat, Vec<u8>) {
    match dynamic {
        DynamicImage::ImageLuma8(b) => (MemoryFormat::G8, b.into_raw()),
        DynamicImage::ImageLumaA8(b) => (MemoryFormat::G8a8, b.into_raw()),
        DynamicImage::ImageRgb8(b) => (MemoryFormat::R8g8b8, b.into_raw()),
        DynamicImage::ImageRgba8(b) => (MemoryFormat::R8g8b8a8, b.into_raw()),
        DynamicImage::ImageLuma16(b) => (MemoryFormat::G16, u16_buf_to_bytes(b.into_raw())),
        DynamicImage::ImageLumaA16(b) => (MemoryFormat::G16a16, u16_buf_to_bytes(b.into_raw())),
        DynamicImage::ImageRgb16(b) => (MemoryFormat::R16g16b16, u16_buf_to_bytes(b.into_raw())),
        DynamicImage::ImageRgba16(b) => {
            (MemoryFormat::R16g16b16a16, u16_buf_to_bytes(b.into_raw()))
        }
        DynamicImage::ImageRgb32F(b) => {
            (MemoryFormat::R32g32b32Float, f32_buf_to_bytes(b.into_raw()))
        }
        DynamicImage::ImageRgba32F(b) => (
            MemoryFormat::R32g32b32a32Float,
            f32_buf_to_bytes(b.into_raw()),
        ),
        other => {
            let rgba = other.into_rgba8();
            (MemoryFormat::R8g8b8a8, rgba.into_raw())
        }
    }
}

fn u16_buf_to_bytes(v: Vec<u16>) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 2);
    for sample in v {
        out.extend_from_slice(&sample.to_ne_bytes());
    }
    out
}

fn f32_buf_to_bytes(v: Vec<f32>) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for sample in v {
        out.extend_from_slice(&sample.to_ne_bytes());
    }
    out
}

fn map_err(e: image::ImageError) -> Error {
    use image::ImageError as E;
    match e {
        E::IoError(io) => Error::Io(io),
        E::Limits(_) => Error::LimitExceeded("image internal"),
        E::Decoding(_) | E::Parameter(_) | E::Unsupported(_) | E::Encoding(_) => {
            Error::Malformed(e.to_string())
        }
    }
}
