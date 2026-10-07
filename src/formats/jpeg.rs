//! JPEG decoder backed by the `jpeg-decoder` crate.

use std::io::Cursor;

use jpeg_decoder::PixelFormat;

use crate::{
    Error, Frame, Image, MemoryFormat, PhysicalDimensionUnit, PixelDensity, Result, Texture,
};

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
    if let Some(exif) = decoder.exif_data() {
        image.set_exif(exif.to_vec());
    }
    if let Some(density) = jfif_density(bytes) {
        image.set_pixel_density(density);
    }
    let _ = opts.apply_transformations;
    Ok(image)
}

/// Pixel density from the JFIF APP0 header. Unit 1 is inch, 2 is
/// centimeter; unit 0 carries only an aspect ratio and unknown units
/// report as absent (matching upstream gufo).
fn jfif_density(bytes: &[u8]) -> Option<PixelDensity> {
    // SOI + APP0 marker + length + "JFIF\0" + version(2).
    if bytes.len() < 18 || bytes[0..4] != [0xFF, 0xD8, 0xFF, 0xE0] {
        return None;
    }
    if &bytes[6..11] != b"JFIF\0" {
        return None;
    }
    let unit = match bytes[13] {
        1 => PhysicalDimensionUnit::Inch,
        2 => PhysicalDimensionUnit::Centimeter,
        _ => return None,
    };
    let x = u16::from_be_bytes([bytes[14], bytes[15]]) as f64;
    let y = u16::from_be_bytes([bytes[16], bytes[17]]) as f64;
    if x <= 0.0 || y <= 0.0 {
        return None;
    }
    Some(PixelDensity::new(x, unit, y, unit))
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

    fn jfif_header(unit: u8, x: u16, y: u16) -> Vec<u8> {
        let mut b = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        b.extend_from_slice(b"JFIF\0");
        b.extend_from_slice(&[0x01, 0x02, unit]);
        b.extend_from_slice(&x.to_be_bytes());
        b.extend_from_slice(&y.to_be_bytes());
        b.extend_from_slice(&[0x00, 0x00]);
        b
    }

    #[test]
    fn parses_jfif_density_inch() {
        let d = jfif_density(&jfif_header(1, 300, 300)).expect("density");
        assert_eq!(d.x_value, 300.0);
        assert_eq!(d.x_unit, PhysicalDimensionUnit::Inch);
    }

    #[test]
    fn parses_jfif_density_centimeter() {
        let d = jfif_density(&jfif_header(2, 118, 119)).expect("density");
        assert_eq!(d.x_value, 118.0);
        assert_eq!(d.y_value, 119.0);
        assert_eq!(d.x_unit, PhysicalDimensionUnit::Centimeter);
    }

    #[test]
    fn rejects_aspect_only_and_zero_density() {
        assert_eq!(jfif_density(&jfif_header(0, 1, 1)), None);
        assert_eq!(jfif_density(&jfif_header(1, 0, 300)), None);
        assert_eq!(jfif_density(b"not a jpeg"), None);
    }
}
