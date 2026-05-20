//! Windows Bitmap decoder via the `image` crate.

use super::DecodeOptions;
use super::image_rs::decode_with;
use crate::{Image, Result};

pub(crate) fn decode(bytes: &[u8], opts: &DecodeOptions) -> Result<Image> {
    decode_with("bmp", image::ImageFormat::Bmp, bytes, opts)
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
        let err = decode(b"BMxx", &opts).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("malformed")
                || matches!(err, crate::Error::Io(_))
        );
    }
}
