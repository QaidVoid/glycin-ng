//! Decoded image, frames, textures, and orientation.

use std::time::Duration;

use crate::{MemoryFormat, SandboxPosture};

/// EXIF orientation values, in TIFF orientation-tag order.
///
/// `Normal` is "no transform". The other variants describe the
/// transform that would map the stored pixels to their visually
/// correct orientation. `Loader::apply_transformations(true)`
/// applies the transform during decode and rewrites the field to
/// `Normal`.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum Orientation {
    /// No transform (EXIF orientation = 1).
    #[default]
    Normal,
    /// Mirror across the vertical axis (EXIF = 2).
    FlipHorizontal,
    /// Rotate 180 degrees (EXIF = 3).
    Rotate180,
    /// Mirror across the horizontal axis (EXIF = 4).
    FlipVertical,
    /// Transpose: mirror across the top-left to bottom-right
    /// diagonal (EXIF = 5).
    Transpose,
    /// Rotate 90 degrees clockwise (EXIF = 6).
    Rotate90,
    /// Transverse: mirror across the top-right to bottom-left
    /// diagonal (EXIF = 7).
    Transverse,
    /// Rotate 270 degrees clockwise (EXIF = 8).
    Rotate270,
}

impl Orientation {
    /// Construct from a raw EXIF orientation tag value.
    ///
    /// Values outside 1..=8 return [`Orientation::Normal`].
    pub fn from_exif(value: u16) -> Self {
        match value {
            2 => Self::FlipHorizontal,
            3 => Self::Rotate180,
            4 => Self::FlipVertical,
            5 => Self::Transpose,
            6 => Self::Rotate90,
            7 => Self::Transverse,
            8 => Self::Rotate270,
            _ => Self::Normal,
        }
    }

    /// EXIF orientation tag value matching this variant.
    pub fn exif_value(self) -> u16 {
        match self {
            Self::Normal => 1,
            Self::FlipHorizontal => 2,
            Self::Rotate180 => 3,
            Self::FlipVertical => 4,
            Self::Transpose => 5,
            Self::Rotate90 => 6,
            Self::Transverse => 7,
            Self::Rotate270 => 8,
        }
    }

    /// Whether the orientation swaps width and height when applied.
    pub fn swaps_axes(self) -> bool {
        matches!(
            self,
            Self::Transpose | Self::Rotate90 | Self::Transverse | Self::Rotate270
        )
    }
}

/// Raw pixel buffer with format and stride.
#[derive(Debug, Clone)]
pub struct Texture {
    width: u32,
    height: u32,
    stride: u32,
    format: MemoryFormat,
    data: Box<[u8]>,
}

impl Texture {
    /// Construct from raw parts.
    ///
    /// `data.len()` must equal `stride as usize * height as usize`;
    /// returns `None` if it does not.
    pub fn from_parts(
        width: u32,
        height: u32,
        stride: u32,
        format: MemoryFormat,
        data: Box<[u8]>,
    ) -> Option<Self> {
        let expected = (stride as usize).checked_mul(height as usize)?;
        if data.len() != expected {
            return None;
        }
        if (stride as u64) < (width as u64) * (format.bytes_per_pixel() as u64) {
            return None;
        }
        Some(Self {
            width,
            height,
            stride,
            format,
            data,
        })
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Stride in bytes between successive rows.
    pub fn stride(&self) -> u32 {
        self.stride
    }

    /// Pixel format of the buffer.
    pub fn format(&self) -> MemoryFormat {
        self.format
    }

    /// Borrowed view of the pixel data.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Take ownership of the pixel buffer.
    pub fn into_data(self) -> Box<[u8]> {
        self.data
    }
}

/// A single image frame.
#[derive(Debug, Clone)]
pub struct Frame {
    texture: Texture,
    delay: Option<Duration>,
}

impl Frame {
    /// Construct from a texture and optional animation delay.
    pub fn new(texture: Texture, delay: Option<Duration>) -> Self {
        Self { texture, delay }
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.texture.width()
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.texture.height()
    }

    /// Borrowed view of the underlying texture.
    pub fn texture(&self) -> &Texture {
        &self.texture
    }

    /// Take ownership of the texture.
    pub fn into_texture(self) -> Texture {
        self.texture
    }

    /// Animation delay until the next frame, or `None` for a still
    /// image.
    pub fn delay(&self) -> Option<Duration> {
        self.delay
    }
}

/// A decoded image: one or more frames plus optional metadata.
#[derive(Debug, Clone)]
pub struct Image {
    width: u32,
    height: u32,
    format_name: &'static str,
    orientation: Orientation,
    icc_profile: Option<Vec<u8>>,
    exif: Option<Vec<u8>>,
    frames: Vec<Frame>,
    sandbox_posture: SandboxPosture,
}

impl Image {
    /// Construct an image from already-decoded frames.
    ///
    /// Most callers should go through [`Loader::load`](crate::Loader::load)
    /// which produces an [`Image`] from raw bytes. This constructor
    /// is for crates that decode a format glycin-ng does not ship a
    /// decoder for and want to project the result through the same
    /// `Image` surface.
    pub fn from_parts(
        format_name: &'static str,
        width: u32,
        height: u32,
        frames: Vec<Frame>,
    ) -> Self {
        Self {
            width,
            height,
            format_name,
            orientation: Orientation::Normal,
            icc_profile: None,
            exif: None,
            frames,
            sandbox_posture: SandboxPosture::none(),
        }
    }

    /// Override the reported [`Orientation`].
    pub fn set_orientation(&mut self, orientation: Orientation) {
        self.orientation = orientation;
    }

    /// Attach an ICC profile blob.
    pub fn set_icc_profile(&mut self, profile: Vec<u8>) {
        self.icc_profile = Some(profile);
    }

    /// Attach an EXIF blob.
    pub fn set_exif(&mut self, exif: Vec<u8>) {
        self.exif = Some(exif);
    }

    pub(crate) fn set_sandbox_posture(&mut self, posture: SandboxPosture) {
        self.sandbox_posture = posture;
    }

    pub(crate) fn replace_frames(&mut self, frames: Vec<Frame>, width: u32, height: u32) {
        self.frames = frames;
        self.width = width;
        self.height = height;
    }
}

impl Image {
    /// Width in pixels of the first frame.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels of the first frame.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Short format-name string (e.g. `"png"`, `"jpeg"`).
    pub fn format_name(&self) -> &'static str {
        self.format_name
    }

    /// Effective orientation. After
    /// `Loader::apply_transformations(true)` this is always
    /// [`Orientation::Normal`].
    pub fn orientation(&self) -> Orientation {
        self.orientation
    }

    /// Embedded ICC profile bytes, if present.
    pub fn icc_profile(&self) -> Option<&[u8]> {
        self.icc_profile.as_deref()
    }

    /// Embedded EXIF blob, if present.
    pub fn exif(&self) -> Option<&[u8]> {
        self.exif.as_deref()
    }

    /// All decoded frames.
    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    /// First frame.
    ///
    /// Returns `None` only on a malformed decoder that produced an
    /// empty `Image`.
    pub fn first_frame(&self) -> Option<&Frame> {
        self.frames.first()
    }

    /// Whether the image is animated (more than one frame).
    pub fn is_animated(&self) -> bool {
        self.frames.len() > 1
    }

    /// Sandbox posture that was active during decode.
    pub fn sandbox_posture(&self) -> SandboxPosture {
        self.sandbox_posture
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation_round_trips_through_exif() {
        for v in 1..=8 {
            let o = Orientation::from_exif(v);
            assert_eq!(o.exif_value(), v);
        }
    }

    #[test]
    fn unknown_exif_orientation_is_normal() {
        assert_eq!(Orientation::from_exif(0), Orientation::Normal);
        assert_eq!(Orientation::from_exif(9), Orientation::Normal);
        assert_eq!(Orientation::from_exif(255), Orientation::Normal);
    }

    #[test]
    fn orientation_swaps_axes_correctly() {
        assert!(!Orientation::Normal.swaps_axes());
        assert!(!Orientation::FlipHorizontal.swaps_axes());
        assert!(!Orientation::Rotate180.swaps_axes());
        assert!(!Orientation::FlipVertical.swaps_axes());
        assert!(Orientation::Transpose.swaps_axes());
        assert!(Orientation::Rotate90.swaps_axes());
        assert!(Orientation::Transverse.swaps_axes());
        assert!(Orientation::Rotate270.swaps_axes());
    }

    #[test]
    fn texture_from_parts_validates_length() {
        let data = vec![0u8; 16].into_boxed_slice();
        let t = Texture::from_parts(2, 2, 8, MemoryFormat::R8g8b8a8, data).unwrap();
        assert_eq!(t.width(), 2);
        assert_eq!(t.height(), 2);
        assert_eq!(t.stride(), 8);
        assert_eq!(t.format(), MemoryFormat::R8g8b8a8);
        assert_eq!(t.data().len(), 16);
    }

    #[test]
    fn texture_from_parts_rejects_wrong_length() {
        let data = vec![0u8; 15].into_boxed_slice();
        assert!(Texture::from_parts(2, 2, 8, MemoryFormat::R8g8b8a8, data).is_none());
    }

    #[test]
    fn texture_from_parts_rejects_stride_below_row() {
        let data = vec![0u8; 12].into_boxed_slice();
        // 2-pixel-wide RGBA row needs 8 bytes; stride 6 is invalid.
        assert!(Texture::from_parts(2, 2, 6, MemoryFormat::R8g8b8a8, data).is_none());
    }

    #[test]
    fn image_accessors() {
        let texture = Texture::from_parts(
            1,
            1,
            4,
            MemoryFormat::R8g8b8a8,
            vec![1, 2, 3, 4].into_boxed_slice(),
        )
        .unwrap();
        let frame = Frame::new(texture, None);
        let mut img = Image::from_parts("png", 1, 1, vec![frame]);
        assert_eq!(img.width(), 1);
        assert_eq!(img.format_name(), "png");
        assert_eq!(img.orientation(), Orientation::Normal);
        assert!(!img.is_animated());
        img.set_orientation(Orientation::Rotate90);
        assert_eq!(img.orientation(), Orientation::Rotate90);
        img.set_icc_profile(b"icc".to_vec());
        assert_eq!(img.icc_profile(), Some(&b"icc"[..]));
        img.set_exif(b"exif".to_vec());
        assert_eq!(img.exif(), Some(&b"exif"[..]));
    }
}
