//! Pixel memory formats.

/// Pixel memory layout of a decoded [`Texture`](crate::Texture).
///
/// Variant names follow channel order and bit depth: `R8g8b8a8` is
/// red, green, blue, alpha at 8 bits per channel; `B8g8r8a8` swaps
/// the color-channel order. Float variants use IEEE 754 binary16 or
/// binary32 per channel. `Premultiplied` means the alpha is already
/// folded into the color channels.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
#[non_exhaustive]
pub enum MemoryFormat {
    /// 8 bit luminance, no alpha.
    G8,
    /// 8 bit luminance, 8 bit straight alpha.
    G8a8,
    /// 8 bit luminance, 8 bit premultiplied alpha.
    G8a8Premultiplied,
    /// 16 bit luminance, no alpha.
    G16,
    /// 16 bit luminance, 16 bit straight alpha.
    G16a16,
    /// 16 bit luminance, 16 bit premultiplied alpha.
    G16a16Premultiplied,
    /// 8 bit per channel RGB.
    R8g8b8,
    /// 8 bit per channel RGBA, straight.
    R8g8b8a8,
    /// 8 bit per channel RGBA, premultiplied.
    R8g8b8a8Premultiplied,
    /// 8 bit per channel BGR.
    B8g8r8,
    /// 8 bit per channel BGRA, straight.
    B8g8r8a8,
    /// 8 bit per channel BGRA, premultiplied.
    B8g8r8a8Premultiplied,
    /// 8 bit per channel ARGB, straight.
    A8r8g8b8,
    /// 8 bit per channel ARGB, premultiplied.
    A8r8g8b8Premultiplied,
    /// 8 bit per channel ABGR, straight.
    A8b8g8r8,
    /// 16 bit per channel RGB.
    R16g16b16,
    /// 16 bit per channel RGBA, straight.
    R16g16b16a16,
    /// 16 bit per channel RGBA, premultiplied.
    R16g16b16a16Premultiplied,
    /// IEEE 754 binary16 per channel RGB.
    R16g16b16Float,
    /// IEEE 754 binary16 per channel RGBA, straight.
    R16g16b16a16Float,
    /// IEEE 754 binary32 per channel RGB.
    R32g32b32Float,
    /// IEEE 754 binary32 per channel RGBA, straight.
    R32g32b32a32Float,
    /// IEEE 754 binary32 per channel RGBA, premultiplied.
    R32g32b32a32FloatPremultiplied,
}

impl MemoryFormat {
    /// Total channels in the format (color + alpha).
    pub const fn channels(self) -> u8 {
        match self {
            Self::G8 | Self::G16 => 1,
            Self::G8a8 | Self::G8a8Premultiplied | Self::G16a16 | Self::G16a16Premultiplied => 2,
            Self::R8g8b8
            | Self::B8g8r8
            | Self::R16g16b16
            | Self::R16g16b16Float
            | Self::R32g32b32Float => 3,
            Self::R8g8b8a8
            | Self::R8g8b8a8Premultiplied
            | Self::B8g8r8a8
            | Self::B8g8r8a8Premultiplied
            | Self::A8r8g8b8
            | Self::A8r8g8b8Premultiplied
            | Self::A8b8g8r8
            | Self::R16g16b16a16
            | Self::R16g16b16a16Premultiplied
            | Self::R16g16b16a16Float
            | Self::R32g32b32a32Float
            | Self::R32g32b32a32FloatPremultiplied => 4,
        }
    }

    /// Bytes per pixel.
    pub const fn bytes_per_pixel(self) -> u8 {
        match self {
            Self::G8 => 1,
            Self::G8a8 | Self::G8a8Premultiplied | Self::G16 => 2,
            Self::R8g8b8 | Self::B8g8r8 => 3,
            Self::R8g8b8a8
            | Self::R8g8b8a8Premultiplied
            | Self::B8g8r8a8
            | Self::B8g8r8a8Premultiplied
            | Self::A8r8g8b8
            | Self::A8r8g8b8Premultiplied
            | Self::A8b8g8r8
            | Self::G16a16
            | Self::G16a16Premultiplied => 4,
            Self::R16g16b16 | Self::R16g16b16Float => 6,
            Self::R16g16b16a16 | Self::R16g16b16a16Premultiplied | Self::R16g16b16a16Float => 8,
            Self::R32g32b32Float => 12,
            Self::R32g32b32a32Float | Self::R32g32b32a32FloatPremultiplied => 16,
        }
    }

    /// Whether the alpha channel is folded into the color channels.
    pub const fn is_premultiplied(self) -> bool {
        matches!(
            self,
            Self::G8a8Premultiplied
                | Self::G16a16Premultiplied
                | Self::R8g8b8a8Premultiplied
                | Self::B8g8r8a8Premultiplied
                | Self::A8r8g8b8Premultiplied
                | Self::R16g16b16a16Premultiplied
                | Self::R32g32b32a32FloatPremultiplied
        )
    }

    /// Whether the format carries an alpha channel.
    pub const fn has_alpha(self) -> bool {
        self.channels() == 2 || self.channels() == 4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: &[MemoryFormat] = &[
        MemoryFormat::G8,
        MemoryFormat::G8a8,
        MemoryFormat::G8a8Premultiplied,
        MemoryFormat::G16,
        MemoryFormat::G16a16,
        MemoryFormat::G16a16Premultiplied,
        MemoryFormat::R8g8b8,
        MemoryFormat::R8g8b8a8,
        MemoryFormat::R8g8b8a8Premultiplied,
        MemoryFormat::B8g8r8,
        MemoryFormat::B8g8r8a8,
        MemoryFormat::B8g8r8a8Premultiplied,
        MemoryFormat::A8r8g8b8,
        MemoryFormat::A8r8g8b8Premultiplied,
        MemoryFormat::A8b8g8r8,
        MemoryFormat::R16g16b16,
        MemoryFormat::R16g16b16a16,
        MemoryFormat::R16g16b16a16Premultiplied,
        MemoryFormat::R16g16b16Float,
        MemoryFormat::R16g16b16a16Float,
        MemoryFormat::R32g32b32Float,
        MemoryFormat::R32g32b32a32Float,
        MemoryFormat::R32g32b32a32FloatPremultiplied,
    ];

    #[test]
    fn bytes_per_pixel_matches_channel_width() {
        for &fmt in ALL {
            let expected = match fmt {
                MemoryFormat::G8 | MemoryFormat::G8a8 | MemoryFormat::G8a8Premultiplied => {
                    fmt.channels() as u32
                }
                MemoryFormat::G16 | MemoryFormat::G16a16 | MemoryFormat::G16a16Premultiplied => {
                    fmt.channels() as u32 * 2
                }
                MemoryFormat::R8g8b8
                | MemoryFormat::R8g8b8a8
                | MemoryFormat::R8g8b8a8Premultiplied
                | MemoryFormat::B8g8r8
                | MemoryFormat::B8g8r8a8
                | MemoryFormat::B8g8r8a8Premultiplied
                | MemoryFormat::A8r8g8b8
                | MemoryFormat::A8r8g8b8Premultiplied
                | MemoryFormat::A8b8g8r8 => fmt.channels() as u32,
                MemoryFormat::R16g16b16
                | MemoryFormat::R16g16b16a16
                | MemoryFormat::R16g16b16a16Premultiplied
                | MemoryFormat::R16g16b16Float
                | MemoryFormat::R16g16b16a16Float => fmt.channels() as u32 * 2,
                MemoryFormat::R32g32b32Float
                | MemoryFormat::R32g32b32a32Float
                | MemoryFormat::R32g32b32a32FloatPremultiplied => fmt.channels() as u32 * 4,
            };
            assert_eq!(
                fmt.bytes_per_pixel() as u32,
                expected,
                "bytes per pixel mismatch for {fmt:?}"
            );
        }
    }

    #[test]
    fn has_alpha_matches_channel_count() {
        for &fmt in ALL {
            let alpha = matches!(fmt.channels(), 2 | 4);
            assert_eq!(fmt.has_alpha(), alpha, "{fmt:?}");
        }
    }

    #[test]
    fn premultiplied_implies_has_alpha() {
        for &fmt in ALL {
            if fmt.is_premultiplied() {
                assert!(fmt.has_alpha(), "{fmt:?}");
            }
        }
    }
}
