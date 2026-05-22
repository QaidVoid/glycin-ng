//! Map between glycin's `GlyMemoryFormat` enum and glycin-ng's
//! [`MemoryFormat`](glycin_ng::MemoryFormat).

use glycin_ng::MemoryFormat;

use crate::ffi::gboolean;

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

pub(crate) fn to_gly(format: MemoryFormat) -> i32 {
    match format {
        MemoryFormat::B8g8r8a8Premultiplied => GLY_MEMORY_B8G8R8A8_PREMULTIPLIED,
        MemoryFormat::A8r8g8b8Premultiplied => GLY_MEMORY_A8R8G8B8_PREMULTIPLIED,
        MemoryFormat::R8g8b8a8Premultiplied => GLY_MEMORY_R8G8B8A8_PREMULTIPLIED,
        MemoryFormat::B8g8r8a8 => GLY_MEMORY_B8G8R8A8,
        MemoryFormat::A8r8g8b8 => GLY_MEMORY_A8R8G8B8,
        MemoryFormat::R8g8b8a8 => GLY_MEMORY_R8G8B8A8,
        MemoryFormat::A8b8g8r8 => GLY_MEMORY_A8B8G8R8,
        MemoryFormat::R8g8b8 => GLY_MEMORY_R8G8B8,
        MemoryFormat::B8g8r8 => GLY_MEMORY_B8G8R8,
        MemoryFormat::R16g16b16 => GLY_MEMORY_R16G16B16,
        MemoryFormat::R16g16b16a16Premultiplied => GLY_MEMORY_R16G16B16A16_PREMULTIPLIED,
        MemoryFormat::R16g16b16a16 => GLY_MEMORY_R16G16B16A16,
        MemoryFormat::R16g16b16Float => GLY_MEMORY_R16G16B16_FLOAT,
        MemoryFormat::R16g16b16a16Float => GLY_MEMORY_R16G16B16A16_FLOAT,
        MemoryFormat::R32g32b32Float => GLY_MEMORY_R32G32B32_FLOAT,
        MemoryFormat::R32g32b32a32FloatPremultiplied => GLY_MEMORY_R32G32B32A32_FLOAT_PREMULTIPLIED,
        MemoryFormat::R32g32b32a32Float => GLY_MEMORY_R32G32B32A32_FLOAT,
        MemoryFormat::G8a8Premultiplied => GLY_MEMORY_G8A8_PREMULTIPLIED,
        MemoryFormat::G8a8 => GLY_MEMORY_G8A8,
        MemoryFormat::G8 => GLY_MEMORY_G8,
        MemoryFormat::G16a16Premultiplied => GLY_MEMORY_G16A16_PREMULTIPLIED,
        MemoryFormat::G16a16 => GLY_MEMORY_G16A16,
        MemoryFormat::G16 => GLY_MEMORY_G16,
        _ => GLY_MEMORY_R8G8B8A8,
    }
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

pub(crate) fn from_gly(value: i32) -> Option<MemoryFormat> {
    match value {
        GLY_MEMORY_B8G8R8A8_PREMULTIPLIED => Some(MemoryFormat::B8g8r8a8Premultiplied),
        GLY_MEMORY_A8R8G8B8_PREMULTIPLIED => Some(MemoryFormat::A8r8g8b8Premultiplied),
        GLY_MEMORY_R8G8B8A8_PREMULTIPLIED => Some(MemoryFormat::R8g8b8a8Premultiplied),
        GLY_MEMORY_B8G8R8A8 => Some(MemoryFormat::B8g8r8a8),
        GLY_MEMORY_A8R8G8B8 => Some(MemoryFormat::A8r8g8b8),
        GLY_MEMORY_R8G8B8A8 => Some(MemoryFormat::R8g8b8a8),
        GLY_MEMORY_A8B8G8R8 => Some(MemoryFormat::A8b8g8r8),
        GLY_MEMORY_R8G8B8 => Some(MemoryFormat::R8g8b8),
        GLY_MEMORY_B8G8R8 => Some(MemoryFormat::B8g8r8),
        _ => None,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_to_gly_matches_documented_order() {
        assert_eq!(
            to_gly(MemoryFormat::B8g8r8a8Premultiplied),
            GLY_MEMORY_B8G8R8A8_PREMULTIPLIED
        );
        assert_eq!(to_gly(MemoryFormat::R8g8b8a8), GLY_MEMORY_R8G8B8A8);
        assert_eq!(to_gly(MemoryFormat::G8), GLY_MEMORY_G8);
        assert_eq!(to_gly(MemoryFormat::G16), GLY_MEMORY_G16);
        assert_eq!(
            to_gly(MemoryFormat::R32g32b32a32Float),
            GLY_MEMORY_R32G32B32A32_FLOAT
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
}
