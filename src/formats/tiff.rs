//! TIFF decoder backed by the `tiff` crate.

use std::io::Cursor;

use tiff::{ColorType, decoder::DecodingResult, tags::Tag};

use crate::{
    Error, Frame, Image, MemoryFormat, PhysicalDimensionUnit, PixelDensity, Result, Texture,
};

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

    let density = resolution_density(&mut decoder);

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
    let mut image = Image::from_parts("tiff", width, height, vec![Frame::new(texture, None)]);
    if let Some(density) = density {
        image.set_pixel_density(density);
    }
    Ok(image)
}

/// Pixel density from the resolution tags. Requires `XResolution`
/// and `YResolution` rationals plus `ResolutionUnit` (2 = inch,
/// 3 = centimeter); anything missing or otherwise valued reports as
/// absent (matching upstream gufo).
fn resolution_density<R: std::io::Read + std::io::Seek>(
    decoder: &mut tiff::decoder::Decoder<R>,
) -> Option<PixelDensity> {
    use tiff::decoder::ifd::Value;

    fn rational(value: Value) -> Option<f64> {
        match value {
            Value::Rational(n, d) if d != 0 => Some(n as f64 / d as f64),
            _ => None,
        }
    }

    let x = decoder
        .find_tag(Tag::XResolution)
        .ok()
        .flatten()
        .and_then(rational)?;
    let y = decoder
        .find_tag(Tag::YResolution)
        .ok()
        .flatten()
        .and_then(rational)?;
    let unit = match decoder.find_tag_unsigned::<u16>(Tag::ResolutionUnit).ok()? {
        Some(2) => PhysicalDimensionUnit::Inch,
        Some(3) => PhysicalDimensionUnit::Centimeter,
        _ => return None,
    };
    if x <= 0.0 || y <= 0.0 || !x.is_finite() || !y.is_finite() {
        return None;
    }
    Some(PixelDensity::new(x, unit, y, unit))
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
            render_size_hint: None,
        };
        let err = decode(b"II*\0garbage", &opts).unwrap_err();
        assert!(matches!(
            err,
            Error::Malformed(_) | Error::Io(_) | Error::Decoder { .. }
        ));
    }

    #[test]
    fn reports_resolution_tags() {
        use std::io::Cursor;
        use tiff::encoder::{Rational, TiffEncoder, colortype};
        use tiff::tags::ResolutionUnit;

        let mut buf = Cursor::new(Vec::new());
        {
            let mut enc = TiffEncoder::new(&mut buf).unwrap();
            let mut img = enc.new_image::<colortype::Gray8>(2, 2).unwrap();
            img.resolution(ResolutionUnit::Inch, Rational { n: 300, d: 1 });
            img.write_data(&[0u8; 4]).unwrap();
        }
        let bytes = buf.into_inner();
        let opts = DecodeOptions {
            limits: Limits::default(),
            apply_transformations: true,
            render_size_hint: None,
        };
        let image = decode(&bytes, &opts).unwrap();
        let density = image.pixel_density().expect("density");
        assert_eq!(density.x_value, 300.0);
        assert_eq!(density.y_value, 300.0);
        assert_eq!(density.x_unit, PhysicalDimensionUnit::Inch);
    }
}
