//! AVIF decoder: in-house HEIF parsing with AV1 decoding by `rav1d`.
//!
//! Still images only. For image sequences (`avis`) the primary still
//! item is decoded. Grid images are rejected.

mod av1;
mod heif;
mod yuv;

use crate::{Error, Frame, Image, MemoryFormat, Orientation, Result, Texture};

use super::DecodeOptions;

pub(crate) fn decode(bytes: &[u8], opts: &DecodeOptions) -> Result<Image> {
    let avif = heif::parse(bytes)?;
    let props = &avif.color.props;
    opts.limits.check_dimensions(props.width, props.height, 1)?;

    let max_pixels = opts.limits.max_pixels;
    let color = av1::decode(&avif.color.data, max_pixels)?;
    let (width, height) = (color.width(), color.height());
    opts.limits.check_dimensions(width, height, 1)?;

    let alpha = match &avif.alpha {
        Some(item) => {
            let alpha = av1::decode(&item.data, max_pixels)?;
            if (alpha.width(), alpha.height(), alpha.bit_depth())
                != (width, height, color.bit_depth())
            {
                return Err(Error::Malformed("alpha plane does not match color".into()));
            }
            Some(alpha)
        }
        None => None,
    };

    let seq = color.color();
    let nclx = props.colr.nclx;
    let matrix = yuv::Matrix {
        coefficients: nclx.map_or(seq.matrix, |n| n.matrix),
        full_range: seq.full_range,
    };
    let (pixels, wide) = yuv::to_rgb(&color, alpha.as_ref(), matrix);

    let format = match (wide, alpha.is_some(), avif.premultiplied) {
        (false, false, _) => MemoryFormat::R8g8b8,
        (false, true, false) => MemoryFormat::R8g8b8a8,
        (false, true, true) => MemoryFormat::R8g8b8a8Premultiplied,
        (true, false, _) => MemoryFormat::R16g16b16,
        (true, true, false) => MemoryFormat::R16g16b16a16,
        (true, true, true) => MemoryFormat::R16g16b16a16Premultiplied,
    };
    let stride = width * format.bytes_per_pixel() as u32;
    let texture = Texture::from_parts(width, height, stride, format, pixels.into_boxed_slice())
        .ok_or_else(|| Error::Decoder {
            format: "avif",
            message: "texture construction failed".into(),
        })?;

    let mut image = Image::from_parts("avif", width, height, vec![Frame::new(texture, None)]);
    if let Some(icc) = props.colr.icc.clone() {
        image.set_icc_profile(icc);
    } else {
        let (primaries, transfer) =
            nclx.map_or((seq.primaries, seq.transfer), |n| (n.primaries, n.transfer));
        if primaries != 2 && transfer != 2 {
            image.set_cicp([primaries, transfer, 0, 1]);
        }
    }

    let orientation = orientation(props.rotation, props.mirror);
    #[cfg(feature = "metadata")]
    if opts.apply_transformations {
        crate::metadata::transform::bake_into_frames(&mut image, orientation);
        return Ok(image);
    }
    image.set_orientation(orientation);
    Ok(image)
}

/// Combine HEIF `irot` (counter-clockwise quarter turns) followed by
/// `imir` (0 swaps top/bottom, 1 swaps left/right) into one
/// orientation.
fn orientation(rotation: u8, mirror: Option<u8>) -> Orientation {
    use Orientation::*;
    match (rotation & 3, mirror) {
        (0, None) => Normal,
        (0, Some(0)) => FlipVertical,
        (0, Some(_)) => FlipHorizontal,
        (1, None) => Rotate270,
        (1, Some(0)) => Transpose,
        (1, Some(_)) => Transverse,
        (2, None) => Rotate180,
        (2, Some(0)) => FlipHorizontal,
        (2, Some(_)) => FlipVertical,
        (_, None) => Rotate90,
        (_, Some(0)) => Transverse,
        (_, Some(_)) => Transpose,
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
            render_size_hint: None,
        }
    }

    #[test]
    fn rejects_garbage() {
        let err = decode(b"not an avif", &opts()).unwrap_err();
        assert!(matches!(err, Error::Malformed(_)));
    }

    fn pixel(image: &Image, x: usize, y: usize) -> Vec<u8> {
        let tex = image.frames()[0].texture();
        let bpp = tex.format().bytes_per_pixel() as usize;
        let at = y * tex.stride() as usize + x * bpp;
        tex.data()[at..at + bpp].to_vec()
    }

    fn close(got: &[u8], want: &[u8]) -> bool {
        got.iter().zip(want).all(|(g, w)| g.abs_diff(*w) <= 2)
    }

    #[test]
    fn decodes_alpha_item() {
        let image = decode(include_bytes!("../../tests/data/alpha.avif"), &opts()).unwrap();
        assert_eq!((image.width(), image.height()), (16, 16));
        assert_eq!(image.frames()[0].texture().format(), MemoryFormat::R8g8b8a8);
        assert!(close(&pixel(&image, 0, 0), &[255, 0, 0, 255]));
        assert!(close(&pixel(&image, 15, 15), &[0, 0, 255, 128]));
    }

    #[test]
    fn decodes_high_bit_depth_to_16_bit() {
        let image = decode(include_bytes!("../../tests/data/10bit.avif"), &opts()).unwrap();
        assert_eq!(
            image.frames()[0].texture().format(),
            MemoryFormat::R16g16b16
        );
        let px = pixel(&image, 8, 8);
        let px = px.as_chunks().0.iter().map(|&b| u16::from_ne_bytes(b));
        for (got, want) in px.zip([128 * 257, 64 * 257, 32 * 257]) {
            assert!(got.abs_diff(want) <= 512, "{got} vs {want}");
        }
    }

    #[test]
    fn lossless_round_trips_exactly() {
        let image = decode(include_bytes!("../../tests/data/lossless.avif"), &opts()).unwrap();
        assert_eq!(pixel(&image, 3, 5), [200, 100, 50]);
    }

    #[test]
    fn corrupt_av1_payload_is_an_error() {
        let err = decode(include_bytes!("../../tests/data/corrupt-av1.avif"), &opts()).unwrap_err();
        assert!(matches!(err, Error::Decoder { .. }), "{err:?}");
    }

    #[test]
    fn irot_and_imir_compose() {
        assert_eq!(orientation(0, None), Orientation::Normal);
        assert_eq!(orientation(1, None), Orientation::Rotate270);
        assert_eq!(orientation(3, None), Orientation::Rotate90);
        assert_eq!(orientation(0, Some(1)), Orientation::FlipHorizontal);
        assert_eq!(orientation(2, Some(0)), Orientation::FlipHorizontal);
    }
}
