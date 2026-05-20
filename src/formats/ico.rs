//! Windows icon and cursor decoder via the `image` crate.

use super::DecodeOptions;
use super::image_rs::decode_with;
use crate::{Image, Result};

pub(crate) fn decode(bytes: &[u8], opts: &DecodeOptions) -> Result<Image> {
    decode_with("ico", image::ImageFormat::Ico, bytes, opts)
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
        let err = decode(b"\x00\x00\x01\x00garbage", &opts).unwrap_err();
        assert!(matches!(
            err,
            crate::Error::Malformed(_) | crate::Error::Io(_)
        ));
    }
}
