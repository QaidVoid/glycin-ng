//! Netpbm decoder via the `image` crate.

use super::DecodeOptions;
use super::image_rs::decode_with;
use crate::{Image, Result};

pub(crate) fn decode(bytes: &[u8], opts: &DecodeOptions) -> Result<Image> {
    decode_with("pnm", image::ImageFormat::Pnm, bytes, opts)
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
    fn decodes_p6_ppm() {
        // P6 header for 2x2 RGB followed by 12 bytes of pixel data.
        let mut bytes = b"P6\n2 2\n255\n".to_vec();
        bytes.extend_from_slice(&[255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255]);
        let image = decode(&bytes, &opts()).unwrap();
        assert_eq!(image.width(), 2);
        assert_eq!(image.height(), 2);
    }

    #[test]
    fn rejects_garbage() {
        let err = decode(b"P0 garbage", &opts()).unwrap_err();
        assert!(matches!(
            err,
            crate::Error::Malformed(_) | crate::Error::Io(_)
        ));
    }
}
