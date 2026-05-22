//! Map between glycin v2's `GlyMemoryFormat` enum and glycin-ng's
//! `GLYCIN_NG_FORMAT_*` constants.

use crate::ffi::gboolean;
use crate::ngapi;

// `GlyMemoryFormat` C enum values from glycin-2's `glycin.h`. The
// integer values are positional in the C `enum`, so they must stay
// in this exact order.
pub(crate) const GLY_MEMORY_B8G8R8A8_PREMULTIPLIED: i32 = 0;
pub(crate) const GLY_MEMORY_A8R8G8B8_PREMULTIPLIED: i32 = 1;
pub(crate) const GLY_MEMORY_R8G8B8A8_PREMULTIPLIED: i32 = 2;
pub(crate) const GLY_MEMORY_B8G8R8A8: i32 = 3;
pub(crate) const GLY_MEMORY_A8R8G8B8: i32 = 4;
pub(crate) const GLY_MEMORY_R8G8B8A8: i32 = 5;
pub(crate) const GLY_MEMORY_A8B8G8R8: i32 = 6;
pub(crate) const GLY_MEMORY_R8G8B8: i32 = 7;
pub(crate) const GLY_MEMORY_B8G8R8: i32 = 8;
pub(crate) const GLY_MEMORY_R16G16B16: i32 = 9;
pub(crate) const GLY_MEMORY_R16G16B16A16_PREMULTIPLIED: i32 = 10;
pub(crate) const GLY_MEMORY_R16G16B16A16: i32 = 11;
pub(crate) const GLY_MEMORY_R16G16B16_FLOAT: i32 = 12;
pub(crate) const GLY_MEMORY_R16G16B16A16_FLOAT: i32 = 13;
pub(crate) const GLY_MEMORY_R32G32B32_FLOAT: i32 = 14;
pub(crate) const GLY_MEMORY_R32G32B32A32_FLOAT_PREMULTIPLIED: i32 = 15;
pub(crate) const GLY_MEMORY_R32G32B32A32_FLOAT: i32 = 16;
pub(crate) const GLY_MEMORY_G8A8_PREMULTIPLIED: i32 = 17;
pub(crate) const GLY_MEMORY_G8A8: i32 = 18;
pub(crate) const GLY_MEMORY_G8: i32 = 19;
pub(crate) const GLY_MEMORY_G16A16_PREMULTIPLIED: i32 = 20;
pub(crate) const GLY_MEMORY_G16A16: i32 = 21;
pub(crate) const GLY_MEMORY_G16: i32 = 22;

/// Convert a glycin-ng format constant into the matching glycin v2
/// `GlyMemoryFormat` value. Returns `GLY_MEMORY_R8G8B8A8` for
/// formats we do not have a direct mapping for (rare 16-bit /
/// float-only formats), matching the legacy behavior the shim
/// inherited from upstream glycin.
pub(crate) fn ng_to_gly(format: std::ffi::c_uint) -> i32 {
    match format {
        ngapi::GLYCIN_NG_FORMAT_B8G8R8A8_PRE => GLY_MEMORY_B8G8R8A8_PREMULTIPLIED,
        ngapi::GLYCIN_NG_FORMAT_A8R8G8B8_PRE => GLY_MEMORY_A8R8G8B8_PREMULTIPLIED,
        ngapi::GLYCIN_NG_FORMAT_R8G8B8A8_PRE => GLY_MEMORY_R8G8B8A8_PREMULTIPLIED,
        ngapi::GLYCIN_NG_FORMAT_B8G8R8A8 => GLY_MEMORY_B8G8R8A8,
        ngapi::GLYCIN_NG_FORMAT_A8R8G8B8 => GLY_MEMORY_A8R8G8B8,
        ngapi::GLYCIN_NG_FORMAT_R8G8B8A8 => GLY_MEMORY_R8G8B8A8,
        ngapi::GLYCIN_NG_FORMAT_A8B8G8R8 => GLY_MEMORY_A8B8G8R8,
        ngapi::GLYCIN_NG_FORMAT_R8G8B8 => GLY_MEMORY_R8G8B8,
        ngapi::GLYCIN_NG_FORMAT_B8G8R8 => GLY_MEMORY_B8G8R8,
        ngapi::GLYCIN_NG_FORMAT_R16G16B16 => GLY_MEMORY_R16G16B16,
        ngapi::GLYCIN_NG_FORMAT_R16G16B16A16_PRE => GLY_MEMORY_R16G16B16A16_PREMULTIPLIED,
        ngapi::GLYCIN_NG_FORMAT_R16G16B16A16 => GLY_MEMORY_R16G16B16A16,
        ngapi::GLYCIN_NG_FORMAT_R16G16B16_F => GLY_MEMORY_R16G16B16_FLOAT,
        ngapi::GLYCIN_NG_FORMAT_R16G16B16A16_F => GLY_MEMORY_R16G16B16A16_FLOAT,
        ngapi::GLYCIN_NG_FORMAT_R32G32B32_F => GLY_MEMORY_R32G32B32_FLOAT,
        ngapi::GLYCIN_NG_FORMAT_R32G32B32A32_F_PRE => GLY_MEMORY_R32G32B32A32_FLOAT_PREMULTIPLIED,
        ngapi::GLYCIN_NG_FORMAT_R32G32B32A32_F => GLY_MEMORY_R32G32B32A32_FLOAT,
        ngapi::GLYCIN_NG_FORMAT_G8A8_PRE => GLY_MEMORY_G8A8_PREMULTIPLIED,
        ngapi::GLYCIN_NG_FORMAT_G8A8 => GLY_MEMORY_G8A8,
        ngapi::GLYCIN_NG_FORMAT_G8 => GLY_MEMORY_G8,
        ngapi::GLYCIN_NG_FORMAT_G16A16_PRE => GLY_MEMORY_G16A16_PREMULTIPLIED,
        ngapi::GLYCIN_NG_FORMAT_G16A16 => GLY_MEMORY_G16A16,
        ngapi::GLYCIN_NG_FORMAT_G16 => GLY_MEMORY_G16,
        _ => GLY_MEMORY_R8G8B8A8,
    }
}

/// Convert a glycin v2 `GlyMemoryFormat` value into the matching
/// `GLYCIN_NG_FORMAT_*` constant. Returns `None` for values we do
/// not yet round-trip; callers fall back to the original layout.
pub(crate) fn gly_to_ng(value: i32) -> Option<std::ffi::c_uint> {
    Some(match value {
        GLY_MEMORY_B8G8R8A8_PREMULTIPLIED => ngapi::GLYCIN_NG_FORMAT_B8G8R8A8_PRE,
        GLY_MEMORY_A8R8G8B8_PREMULTIPLIED => ngapi::GLYCIN_NG_FORMAT_A8R8G8B8_PRE,
        GLY_MEMORY_R8G8B8A8_PREMULTIPLIED => ngapi::GLYCIN_NG_FORMAT_R8G8B8A8_PRE,
        GLY_MEMORY_B8G8R8A8 => ngapi::GLYCIN_NG_FORMAT_B8G8R8A8,
        GLY_MEMORY_A8R8G8B8 => ngapi::GLYCIN_NG_FORMAT_A8R8G8B8,
        GLY_MEMORY_R8G8B8A8 => ngapi::GLYCIN_NG_FORMAT_R8G8B8A8,
        GLY_MEMORY_A8B8G8R8 => ngapi::GLYCIN_NG_FORMAT_A8B8G8R8,
        GLY_MEMORY_R8G8B8 => ngapi::GLYCIN_NG_FORMAT_R8G8B8,
        GLY_MEMORY_B8G8R8 => ngapi::GLYCIN_NG_FORMAT_B8G8R8,
        _ => return None,
    })
}

pub(crate) fn has_alpha_for_gly(value: i32) -> gboolean {
    match value {
        GLY_MEMORY_B8G8R8A8_PREMULTIPLIED
        | GLY_MEMORY_A8R8G8B8_PREMULTIPLIED
        | GLY_MEMORY_R8G8B8A8_PREMULTIPLIED
        | GLY_MEMORY_B8G8R8A8
        | GLY_MEMORY_A8R8G8B8
        | GLY_MEMORY_R8G8B8A8
        | GLY_MEMORY_A8B8G8R8
        | GLY_MEMORY_R16G16B16A16_PREMULTIPLIED
        | GLY_MEMORY_R16G16B16A16
        | GLY_MEMORY_R16G16B16A16_FLOAT
        | GLY_MEMORY_R32G32B32A32_FLOAT_PREMULTIPLIED
        | GLY_MEMORY_R32G32B32A32_FLOAT
        | GLY_MEMORY_G8A8_PREMULTIPLIED
        | GLY_MEMORY_G8A8
        | GLY_MEMORY_G16A16_PREMULTIPLIED
        | GLY_MEMORY_G16A16 => 1,
        _ => 0,
    }
}

pub(crate) fn is_premultiplied_for_gly(value: i32) -> gboolean {
    matches!(
        value,
        GLY_MEMORY_B8G8R8A8_PREMULTIPLIED
            | GLY_MEMORY_A8R8G8B8_PREMULTIPLIED
            | GLY_MEMORY_R8G8B8A8_PREMULTIPLIED
            | GLY_MEMORY_R16G16B16A16_PREMULTIPLIED
            | GLY_MEMORY_R32G32B32A32_FLOAT_PREMULTIPLIED
            | GLY_MEMORY_G8A8_PREMULTIPLIED
            | GLY_MEMORY_G16A16_PREMULTIPLIED
    ) as gboolean
}

/// Bytes per pixel for a `GLYCIN_NG_FORMAT_*` constant. Returns 0
/// for unknown values so callers can detect mismatches.
pub(crate) fn bytes_per_pixel_ng(format: std::ffi::c_uint) -> usize {
    match format {
        ngapi::GLYCIN_NG_FORMAT_G8 => 1,
        ngapi::GLYCIN_NG_FORMAT_G8A8
        | ngapi::GLYCIN_NG_FORMAT_G8A8_PRE
        | ngapi::GLYCIN_NG_FORMAT_G16 => 2,
        ngapi::GLYCIN_NG_FORMAT_R8G8B8 | ngapi::GLYCIN_NG_FORMAT_B8G8R8 => 3,
        ngapi::GLYCIN_NG_FORMAT_R8G8B8A8
        | ngapi::GLYCIN_NG_FORMAT_R8G8B8A8_PRE
        | ngapi::GLYCIN_NG_FORMAT_B8G8R8A8
        | ngapi::GLYCIN_NG_FORMAT_B8G8R8A8_PRE
        | ngapi::GLYCIN_NG_FORMAT_A8R8G8B8
        | ngapi::GLYCIN_NG_FORMAT_A8R8G8B8_PRE
        | ngapi::GLYCIN_NG_FORMAT_A8B8G8R8
        | ngapi::GLYCIN_NG_FORMAT_G16A16
        | ngapi::GLYCIN_NG_FORMAT_G16A16_PRE => 4,
        ngapi::GLYCIN_NG_FORMAT_R16G16B16 | ngapi::GLYCIN_NG_FORMAT_R16G16B16_F => 6,
        ngapi::GLYCIN_NG_FORMAT_R16G16B16A16
        | ngapi::GLYCIN_NG_FORMAT_R16G16B16A16_PRE
        | ngapi::GLYCIN_NG_FORMAT_R16G16B16A16_F => 8,
        ngapi::GLYCIN_NG_FORMAT_R32G32B32_F => 12,
        ngapi::GLYCIN_NG_FORMAT_R32G32B32A32_F | ngapi::GLYCIN_NG_FORMAT_R32G32B32A32_F_PRE => 16,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_r8g8b8a8() {
        assert_eq!(
            ng_to_gly(ngapi::GLYCIN_NG_FORMAT_R8G8B8A8),
            GLY_MEMORY_R8G8B8A8
        );
        assert_eq!(
            gly_to_ng(GLY_MEMORY_R8G8B8A8).unwrap(),
            ngapi::GLYCIN_NG_FORMAT_R8G8B8A8
        );
    }

    #[test]
    fn alpha_classification() {
        assert_eq!(has_alpha_for_gly(GLY_MEMORY_R8G8B8A8), 1);
        assert_eq!(has_alpha_for_gly(GLY_MEMORY_R8G8B8), 0);
        assert_eq!(has_alpha_for_gly(GLY_MEMORY_G8A8), 1);
        assert_eq!(has_alpha_for_gly(GLY_MEMORY_G8), 0);
    }

    #[test]
    fn premul_classification() {
        assert_eq!(
            is_premultiplied_for_gly(GLY_MEMORY_R8G8B8A8_PREMULTIPLIED),
            1
        );
        assert_eq!(is_premultiplied_for_gly(GLY_MEMORY_R8G8B8A8), 0);
        assert_eq!(is_premultiplied_for_gly(GLY_MEMORY_G8A8_PREMULTIPLIED), 1);
    }

    #[test]
    fn bytes_per_pixel_matches_ng_layout() {
        assert_eq!(bytes_per_pixel_ng(ngapi::GLYCIN_NG_FORMAT_R8G8B8), 3);
        assert_eq!(bytes_per_pixel_ng(ngapi::GLYCIN_NG_FORMAT_R8G8B8A8), 4);
        assert_eq!(bytes_per_pixel_ng(ngapi::GLYCIN_NG_FORMAT_G16A16), 4);
        assert_eq!(bytes_per_pixel_ng(ngapi::GLYCIN_NG_FORMAT_R16G16B16), 6);
        assert_eq!(
            bytes_per_pixel_ng(ngapi::GLYCIN_NG_FORMAT_R32G32B32A32_F),
            16
        );
    }
}
