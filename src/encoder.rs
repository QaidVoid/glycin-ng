//! Image encoding.
//!
//! Build an [`Encoder`] for a target [`KnownFormat`], queue one or
//! more [`EncodeFrame`]s into it, then call [`Encoder::encode`] to
//! emit the encoded bytes.
//!
//! Encoders run in-process. Codec support is gated by both the
//! `encode` capability group and the per-format feature gate; with
//! `encode` on, the currently supported targets are PNG, JPEG, GIF,
//! WebP (lossless only), TIFF, and BMP.

#[cfg(feature = "encode")]
use std::io::Cursor;

#[cfg(feature = "encode")]
use image::{ExtendedColorType as ECT, ImageEncoder};

use crate::{Error, KnownFormat, MemoryFormat, PixelDensity, Result};

/// Maximum output-buffer size [`to_rgba8`] will allocate. 1 GiB
/// covers any reasonable screenshot or photo while rejecting hostile
/// inputs (e.g. 65535x65535xRgba8 = ~16 GiB) before the allocation
/// request reaches the allocator.
const MAX_OUTPUT_BYTES: usize = 1 << 30;

/// One frame queued for encoding.
#[derive(Debug, Clone)]
pub struct EncodeFrame {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Byte distance between the start of consecutive rows in `data`.
    /// Must be at least `width * format.bytes_per_pixel()`.
    pub stride: u32,
    /// Pixel layout of `data`.
    pub format: MemoryFormat,
    /// Raw pixel bytes. Must contain at least `(height - 1) * stride
    /// + width * format.bytes_per_pixel()` bytes.
    pub data: Vec<u8>,
}

/// Builder that turns one or more [`EncodeFrame`]s into encoded
/// bytes for a chosen [`KnownFormat`].
///
/// Construct via [`Encoder::new`]; configure with the various setters;
/// finalize with [`Encoder::encode`]. Encoders consume themselves on
/// `encode` to release frame data promptly.
#[derive(Debug)]
pub struct Encoder {
    target: KnownFormat,
    frames: Vec<EncodeFrame>,
    quality: u8,
    compression: u8,
    icc_profile: Option<Vec<u8>>,
    metadata: Vec<(String, String)>,
    pixel_density: Option<PixelDensity>,
}

impl Encoder {
    /// Create an encoder for `target`. Returns
    /// [`Error::UnsupportedFormat`] when no encoder for that format
    /// is compiled in (either `encode` is off or the per-format
    /// gate is off).
    pub fn new(target: KnownFormat) -> Result<Self> {
        if !is_supported(target) {
            return Err(Error::UnsupportedFormat);
        }
        Ok(Self {
            target,
            frames: Vec::new(),
            quality: 75,
            compression: 6,
            icc_profile: None,
            metadata: Vec::new(),
            pixel_density: None,
        })
    }

    /// Target format the encoder was built for.
    pub fn target(&self) -> KnownFormat {
        self.target
    }

    /// Append a frame to encode.
    pub fn add_frame(&mut self, frame: EncodeFrame) -> &mut Self {
        self.frames.push(frame);
        self
    }

    /// Set the lossy quality for codecs that honor it (JPEG).
    /// Lossless codecs (PNG, BMP, GIF, TIFF, lossless WebP) ignore
    /// this value. The encoder clamps as the underlying codec
    /// requires.
    pub fn set_quality(&mut self, quality: u8) -> &mut Self {
        self.quality = quality;
        self
    }

    /// Set the compression level for codecs that honor it. The
    /// current image-crate encoders ignore this; the value is
    /// retained for future PNG / WebP tuning.
    pub fn set_compression(&mut self, compression: u8) -> &mut Self {
        self.compression = compression;
        self
    }

    /// Attach (or clear, with `None`) an ICC profile that the encoder
    /// will embed for codecs that can carry it (PNG iCCP, JPEG APP2
    /// ICC_PROFILE, WebP ICCP). Other codecs accept the profile but
    /// cannot embed it.
    pub fn set_icc_profile(&mut self, icc: Option<Vec<u8>>) -> &mut Self {
        self.icc_profile = icc;
        self
    }

    /// Attach a metadata key/value pair. Retained on the encoder for
    /// future format-specific lowering (PNG tEXt, JPEG EXIF). Current
    /// encoders do not embed it.
    pub fn add_metadata(&mut self, key: String, value: String) -> &mut Self {
        self.metadata.push((key, value));
        self
    }

    /// Attach (or clear, with `None`) a pixel density the encoder
    /// will embed for codecs that can carry it (PNG `pHYs`, JPEG
    /// JFIF, TIFF resolution tags). Returns whether the target
    /// accepts the value: formats without density support accept
    /// only `None`.
    pub fn set_pixel_density(&mut self, density: Option<PixelDensity>) -> bool {
        if density.is_some() && !supports_density(self.target) {
            return false;
        }
        self.pixel_density = density;
        true
    }

    /// Encode the queued frames into a byte buffer.
    ///
    /// Takes `&self` so callers can encode through a shared handle
    /// without surrendering it (the C ABI and the shim's
    /// `Mutex`-guarded creator state both need this). Frames stay
    /// queued; call [`Encoder::clear_frames`] if you want to reuse
    /// the encoder for another image.
    ///
    /// Returns [`Error::Internal`] when no frames were added, or when
    /// more than one frame was queued (multi-frame animation encoding
    /// is not yet implemented; the C ABI in
    /// [`libglycin-shim`](../../libglycin_shim/index.html) does not
    /// carry per-frame delay, so emitting an animation here would
    /// need to fabricate timing). Format-specific errors surface as
    /// [`Error::Encoder`].
    pub fn encode(&self) -> Result<Vec<u8>> {
        if self.frames.is_empty() {
            return Err(Error::Internal("no frames queued for encode".into()));
        }
        if self.frames.len() > 1 {
            return Err(Error::Internal(
                "multi-frame encoding is not yet supported".into(),
            ));
        }
        let frame = &self.frames[0];
        let rgba = to_rgba8(
            &frame.data,
            frame.width,
            frame.height,
            frame.stride,
            frame.format,
        )?;
        encode_dispatch(self, rgba, frame.width, frame.height)
    }

    /// Drop any frames previously added with [`Encoder::add_frame`].
    /// Useful when reusing an encoder configuration across multiple
    /// images.
    pub fn clear_frames(&mut self) -> &mut Self {
        self.frames.clear();
        self
    }
}

#[cfg(feature = "encode")]
fn is_supported(target: KnownFormat) -> bool {
    matches!(
        target,
        KnownFormat::Png
            | KnownFormat::Jpeg
            | KnownFormat::Gif
            | KnownFormat::WebP
            | KnownFormat::Tiff
            | KnownFormat::Bmp,
    )
}

/// Targets that can embed pixel density (upstream marks
/// `CreatorPixelDensity` for JPEG, PNG, and TIFF). PNG and TIFF
/// support is only reported when the corresponding codec feature is
/// enabled, since the fallback encoders cannot write density.
fn supports_density(target: KnownFormat) -> bool {
    match target {
        KnownFormat::Jpeg => true,
        KnownFormat::Png => cfg!(feature = "png"),
        KnownFormat::Tiff => cfg!(feature = "tiff"),
        _ => false,
    }
}

#[cfg(not(feature = "encode"))]
fn is_supported(_target: KnownFormat) -> bool {
    false
}

#[cfg(feature = "encode")]
fn encode_dispatch(cfg: &Encoder, rgba: Vec<u8>, width: u32, height: u32) -> Result<Vec<u8>> {
    let mut out = Cursor::new(Vec::new());
    let result = match cfg.target {
        KnownFormat::Png => encode_png(cfg, &rgba, width, height, &mut out),
        KnownFormat::Jpeg => {
            let rgb = flatten_over_white(rgba);
            let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, cfg.quality);
            if let Some(p) = cfg.icc_profile.as_ref() {
                let _ = enc.set_icc_profile(p.clone());
            }
            if let Some(density) = cfg.pixel_density {
                enc.set_pixel_density(jpeg_density(density));
            }
            enc.write_image(&rgb, width, height, ECT::Rgb8)
        }
        KnownFormat::Gif => {
            let mut enc = image::codecs::gif::GifEncoder::new(&mut out);
            enc.encode(&rgba, width, height, ECT::Rgba8)
        }
        KnownFormat::WebP => {
            // image 0.25 only ships the lossless WebP encoder; the
            // `quality` knob on `Encoder` is captured but ignored
            // here.
            let mut enc = image::codecs::webp::WebPEncoder::new_lossless(&mut out);
            if let Some(p) = cfg.icc_profile.as_ref() {
                let _ = enc.set_icc_profile(p.clone());
            }
            enc.write_image(&rgba, width, height, ECT::Rgba8)
        }
        KnownFormat::Tiff => encode_tiff(cfg, &rgba, width, height, &mut out),
        KnownFormat::Bmp => {
            let enc = image::codecs::bmp::BmpEncoder::new(&mut out);
            enc.write_image(&rgba, width, height, ECT::Rgba8)
        }
        _ => return Err(Error::UnsupportedFormat),
    };
    result.map_err(|e| Error::Encoder {
        format: cfg.target.name(),
        message: e.to_string(),
    })?;
    Ok(out.into_inner())
}

#[cfg(not(feature = "encode"))]
fn encode_dispatch(_cfg: &Encoder, _rgba: Vec<u8>, _width: u32, _height: u32) -> Result<Vec<u8>> {
    Err(Error::UnsupportedFormat)
}

/// Encode PNG with the `png` crate directly so pixel density
/// (`pHYs`, converted to meters like upstream) can be written. The
/// `image`-crate encoder exposes no density hook.
#[cfg(all(feature = "encode", feature = "png"))]
fn encode_png(
    cfg: &Encoder,
    rgba: &[u8],
    width: u32,
    height: u32,
    out: &mut Cursor<Vec<u8>>,
) -> image::ImageResult<()> {
    use image::ImageError;

    let mut info = png::Info::with_size(width, height);
    if let Some(p) = cfg.icc_profile.as_ref() {
        info.icc_profile = Some(std::borrow::Cow::Borrowed(p.as_slice()));
    }
    if let Some(density) = cfg.pixel_density {
        let meter = density.convert(crate::PhysicalDimensionUnit::Meter);
        let x = meter.x_value.round().clamp(1.0, u32::MAX as f64) as u32;
        let y = meter.y_value.round().clamp(1.0, u32::MAX as f64) as u32;
        info.pixel_dims = Some(png::PixelDimensions {
            xppu: x,
            yppu: y,
            unit: png::Unit::Meter,
        });
    }

    let mut enc = png::Encoder::with_info(out, info).map_err(|e| ImageError::IoError(e.into()))?;
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc
        .write_header()
        .map_err(|e| ImageError::IoError(e.into()))?;
    writer
        .write_image_data(rgba)
        .map_err(|e| ImageError::IoError(e.into()))
}

/// PNG fallback when the `png` decode feature is off: the
/// `image`-crate encoder without density or interlace support.
#[cfg(all(feature = "encode", not(feature = "png")))]
fn encode_png(
    cfg: &Encoder,
    rgba: &[u8],
    width: u32,
    height: u32,
    out: &mut Cursor<Vec<u8>>,
) -> image::ImageResult<()> {
    let mut enc = image::codecs::png::PngEncoder::new(out);
    if let Some(p) = cfg.icc_profile.as_ref() {
        let _ = enc.set_icc_profile(p.clone());
    }
    enc.write_image(rgba, width, height, ECT::Rgba8)
}

/// Map engine density to the `image`-crate JPEG density: centimeter
/// stays centimeter, every other unit converts to inch (like
/// upstream). Values round to `u16`, clamped to the valid range.
#[cfg(feature = "encode")]
fn jpeg_density(density: PixelDensity) -> image::codecs::jpeg::PixelDensity {
    use image::codecs::jpeg::{PixelDensity as JpegDensity, PixelDensityUnit as JpegUnit};

    let (unit, jpeg_unit) = if density.x_unit == crate::PhysicalDimensionUnit::Centimeter {
        (
            crate::PhysicalDimensionUnit::Centimeter,
            JpegUnit::Centimeters,
        )
    } else {
        (crate::PhysicalDimensionUnit::Inch, JpegUnit::Inches)
    };
    let converted = density.convert(unit);
    let round = |v: f64| v.round().clamp(1.0, u16::MAX as f64) as u16;
    JpegDensity {
        density: (round(converted.x_value), round(converted.y_value)),
        unit: jpeg_unit,
    }
}

/// Encode TIFF with the `tiff` crate directly so resolution tags can
/// be written. The `image`-crate encoder exposes no tag hooks.
#[cfg(all(feature = "encode", feature = "tiff"))]
fn encode_tiff(
    cfg: &Encoder,
    rgba: &[u8],
    width: u32,
    height: u32,
    out: &mut Cursor<Vec<u8>>,
) -> image::ImageResult<()> {
    use tiff::encoder::{TiffEncoder, colortype::RGBA8};
    use tiff::tags::ResolutionUnit;

    let mut enc = TiffEncoder::new(out).map_err(tiff_error)?;
    let mut img = enc.new_image::<RGBA8>(width, height).map_err(tiff_error)?;
    if let Some(density) = cfg.pixel_density {
        let (unit, tiff_unit) = if density.x_unit == crate::PhysicalDimensionUnit::Centimeter {
            (
                crate::PhysicalDimensionUnit::Centimeter,
                ResolutionUnit::Centimeter,
            )
        } else {
            (crate::PhysicalDimensionUnit::Inch, ResolutionUnit::Inch)
        };
        let converted = density.convert(unit);
        img.x_resolution(float_rational(converted.x_value));
        img.y_resolution(float_rational(converted.y_value));
        img.resolution_unit(tiff_unit);
    }
    img.write_data(rgba).map_err(tiff_error)?;
    Ok(())
}

/// Map a `tiff`-crate error into an `image` error so the dispatch
/// keeps a single error type.
#[cfg(all(feature = "encode", feature = "tiff"))]
fn tiff_error(e: tiff::TiffError) -> image::ImageError {
    image::ImageError::IoError(std::io::Error::other(e.to_string()))
}

/// TIFF fallback when the `tiff` decode feature is off: the
/// `image`-crate encoder without resolution support.
#[cfg(all(feature = "encode", not(feature = "tiff")))]
fn encode_tiff(
    _cfg: &Encoder,
    rgba: &[u8],
    width: u32,
    height: u32,
    out: &mut Cursor<Vec<u8>>,
) -> image::ImageResult<()> {
    let enc = image::codecs::tiff::TiffEncoder::new(out);
    enc.write_image(rgba, width, height, ECT::Rgba8)
}

/// Approximate a positive float as a TIFF RATIONAL, reduced to
/// lowest terms at micro precision.
#[cfg(all(feature = "encode", feature = "tiff"))]
fn float_rational(value: f64) -> tiff::encoder::Rational {
    use tiff::encoder::Rational;

    let scaled = (value * 1_000_000.0).round().clamp(1.0, u32::MAX as f64) as u32;
    let mut n = scaled;
    let mut d = 1_000_000u32;
    let g = gcd(n, d);
    n /= g;
    d /= g;
    Rational { n, d }
}

#[cfg(all(feature = "encode", feature = "tiff"))]
fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}

/// Convert raw pixel data from `format` into a flat RGBA8 row-major
/// buffer with no padding between rows. Bound-checks the input and
/// caps the output allocation at [`MAX_OUTPUT_BYTES`].
fn to_rgba8(
    data: &[u8],
    width: u32,
    height: u32,
    stride: u32,
    format: MemoryFormat,
) -> Result<Vec<u8>> {
    let bpp = format.bytes_per_pixel() as usize;
    let width = width as usize;
    let height = height as usize;
    let stride = stride as usize;

    let row_bytes = width
        .checked_mul(bpp)
        .ok_or(Error::LimitExceeded("row width overflow"))?;
    if row_bytes > stride {
        return Err(Error::Malformed(format!(
            "stride {stride} narrower than {row_bytes} bytes/row"
        )));
    }

    // Reject hostile width/height before any per-pixel arithmetic
    // or input bounds-check, so an absurd request fails on the cap
    // rather than first inflating `needed` and tripping a misleading
    // truncation error.
    let out_stride = width
        .checked_mul(4)
        .ok_or(Error::LimitExceeded("output stride overflow"))?;
    let out_total = out_stride
        .checked_mul(height)
        .ok_or(Error::LimitExceeded("output size overflow"))?;
    if out_total > MAX_OUTPUT_BYTES {
        return Err(Error::LimitExceeded("output exceeds 1 GiB"));
    }

    if height > 0 {
        let needed = (height - 1)
            .checked_mul(stride)
            .and_then(|n| n.checked_add(row_bytes))
            .ok_or(Error::LimitExceeded("input size overflow"))?;
        if data.len() < needed {
            return Err(Error::Truncated("texture data shorter than dimensions"));
        }
    }

    let mut out = Vec::with_capacity(out_total);
    for y in 0..height {
        let row_offset = y * stride;
        let row = &data[row_offset..row_offset + row_bytes];
        for x in 0..width {
            let p = &row[x * bpp..x * bpp + bpp];
            let (r, g, b, a) = sample_rgba8(format, p)
                .ok_or(Error::Internal(format!("no rgba8 sampler for {format:?}")))?;
            out.extend_from_slice(&[r, g, b, a]);
        }
    }
    Ok(out)
}

/// Composite straight-alpha RGBA8 over an opaque white background,
/// returning the same buffer rewritten as tightly packed RGB8.
///
/// JPEG carries no alpha channel, and the `image` crate's JPEG
/// encoder rejects `Rgba8` outright, so transparency has to be
/// resolved before encoding. White is the conventional flattening
/// background and is deliberately not configurable.
///
/// Takes `buf` by value and compacts it in place, so peak memory
/// stays at the size of the RGBA buffer rather than holding a second
/// buffer three quarters as large beside it. The forward pass cannot
/// clobber unread input: pixel `i` writes at most index `i * 3 + 2`,
/// always below the next pixel's first read at `(i + 1) * 4`.
#[cfg(feature = "encode")]
fn flatten_over_white(mut buf: Vec<u8>) -> Vec<u8> {
    let pixels = buf.len() / 4;
    for i in 0..pixels {
        let a = buf[i * 4 + 3] as u32;
        let over_white = |c: u8| ((c as u32 * a + 255 * (255 - a) + 127) / 255) as u8;
        let (r, g, b) = (
            over_white(buf[i * 4]),
            over_white(buf[i * 4 + 1]),
            over_white(buf[i * 4 + 2]),
        );
        buf[i * 3] = r;
        buf[i * 3 + 1] = g;
        buf[i * 3 + 2] = b;
    }
    buf.truncate(pixels * 3);
    buf
}

fn sample_rgba8(fmt: MemoryFormat, p: &[u8]) -> Option<(u8, u8, u8, u8)> {
    Some(match fmt {
        MemoryFormat::G8 => (p[0], p[0], p[0], 255),
        MemoryFormat::G8a8 => (p[0], p[0], p[0], p[1]),
        MemoryFormat::G8a8Premultiplied => {
            let (g, a) = unpremul_g8(p[0], p[1]);
            (g, g, g, a)
        }
        MemoryFormat::R8g8b8 => (p[0], p[1], p[2], 255),
        MemoryFormat::B8g8r8 => (p[2], p[1], p[0], 255),
        MemoryFormat::R8g8b8a8 => (p[0], p[1], p[2], p[3]),
        MemoryFormat::R8g8b8a8Premultiplied => unpremul_rgb8(p[0], p[1], p[2], p[3]),
        MemoryFormat::B8g8r8a8 => (p[2], p[1], p[0], p[3]),
        MemoryFormat::B8g8r8a8Premultiplied => unpremul_rgb8(p[2], p[1], p[0], p[3]),
        MemoryFormat::A8r8g8b8 => (p[1], p[2], p[3], p[0]),
        MemoryFormat::A8r8g8b8Premultiplied => unpremul_rgb8(p[1], p[2], p[3], p[0]),
        MemoryFormat::A8b8g8r8 => (p[3], p[2], p[1], p[0]),
        _ => return None,
    })
}

fn unpremul_g8(g: u8, a: u8) -> (u8, u8) {
    if a == 0 {
        return (0, 0);
    }
    let g = ((g as u32 * 255) / a as u32).min(255) as u8;
    (g, a)
}

fn unpremul_rgb8(r: u8, g: u8, b: u8, a: u8) -> (u8, u8, u8, u8) {
    if a == 0 {
        return (0, 0, 0, 0);
    }
    let unp = |c: u8| ((c as u32 * 255) / a as u32).min(255) as u8;
    (unp(r), unp(g), unp(b), a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_format_with_no_encoder() {
        // QOI / JXL / SVG / TGA / EXR / PNM / DDS / ICO have no
        // encoder behind the current `image`-crate setup.
        assert!(matches!(
            Encoder::new(KnownFormat::Qoi),
            Err(Error::UnsupportedFormat)
        ));
        assert!(matches!(
            Encoder::new(KnownFormat::Svg),
            Err(Error::UnsupportedFormat)
        ));
    }

    #[cfg(feature = "encode")]
    #[test]
    fn encode_with_no_frames_is_internal_error() {
        let enc = Encoder::new(KnownFormat::Png).unwrap();
        assert!(matches!(enc.encode(), Err(Error::Internal(_))));
    }

    #[cfg(feature = "encode")]
    #[test]
    fn encode_with_multiple_frames_is_internal_error() {
        let mut enc = Encoder::new(KnownFormat::Png).unwrap();
        let mk = || EncodeFrame {
            width: 1,
            height: 1,
            stride: 4,
            format: MemoryFormat::R8g8b8a8,
            data: vec![0, 0, 0, 255],
        };
        enc.add_frame(mk());
        enc.add_frame(mk());
        assert!(matches!(enc.encode(), Err(Error::Internal(_))));
    }

    #[test]
    fn to_rgba8_rejects_short_input() {
        let data = vec![1, 2, 3, 4, 5, 6];
        assert!(matches!(
            to_rgba8(&data, 2, 2, 6, MemoryFormat::R8g8b8),
            Err(Error::Truncated(_))
        ));
    }

    #[test]
    fn to_rgba8_rejects_oversized_output() {
        let stub = [0u8; 4];
        assert!(matches!(
            to_rgba8(&stub, 32768, 32768, 32768 * 4, MemoryFormat::R8g8b8a8),
            Err(Error::LimitExceeded(_))
        ));
    }

    #[test]
    fn to_rgba8_rejects_narrow_stride() {
        let data = vec![0u8; 64];
        assert!(matches!(
            to_rgba8(&data, 4, 4, 8, MemoryFormat::R8g8b8),
            Err(Error::Malformed(_))
        ));
    }

    #[cfg(feature = "encode")]
    #[test]
    fn flatten_over_white_composites_alpha() {
        // Fully opaque pixel passes through unchanged.
        assert_eq!(flatten_over_white(vec![10, 20, 30, 255]), [10, 20, 30]);
        // Fully transparent pixel becomes white.
        assert_eq!(flatten_over_white(vec![10, 20, 30, 0]), [255, 255, 255]);
        // Half-transparent red over white leaves red saturated and
        // lifts the other channels halfway to 255.
        assert_eq!(flatten_over_white(vec![255, 0, 0, 128]), [255, 127, 127]);
    }

    #[cfg(feature = "encode")]
    #[test]
    fn flatten_over_white_is_correct_across_overlapping_writes() {
        // Four pixels, so the compacted RGB output overlaps input
        // still waiting to be read. Anything but a correct forward
        // pass corrupts the tail.
        let buf = vec![
            255, 0, 0, 255, // opaque red
            0, 255, 0, 0, // transparent green
            0, 0, 255, 128, // half-transparent blue
            10, 20, 30, 64, // mostly transparent dark grey
        ];
        assert_eq!(
            flatten_over_white(buf),
            [255, 0, 0, 255, 255, 255, 127, 127, 255, 194, 196, 199]
        );
    }

    #[cfg(feature = "encode")]
    #[test]
    fn jpeg_flattens_transparency_to_white() {
        let mut enc = Encoder::new(KnownFormat::Jpeg).unwrap();
        enc.set_quality(100);
        enc.add_frame(EncodeFrame {
            width: 8,
            height: 8,
            stride: 32,
            format: MemoryFormat::R8g8b8a8,
            // Fully transparent black. Dropping alpha instead of
            // compositing would decode back as black.
            data: [0, 0, 0, 0].repeat(64),
        });
        let bytes = enc.encode().expect("jpeg encode of rgba should succeed");
        assert_eq!(&bytes[..2], &[0xFF, 0xD8]);

        let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::Jpeg)
            .expect("re-decode the encoded jpeg")
            .to_rgb8();
        for px in decoded.pixels() {
            assert!(
                px.0.iter().all(|&c| c >= 250),
                "transparent input should flatten to white, got {:?}",
                px.0
            );
        }
    }

    #[cfg(feature = "encode")]
    #[test]
    fn png_round_trip_recovers_pixel() {
        let mut enc = Encoder::new(KnownFormat::Png).unwrap();
        enc.add_frame(EncodeFrame {
            width: 2,
            height: 1,
            stride: 8,
            format: MemoryFormat::R8g8b8a8,
            data: vec![10, 20, 30, 255, 40, 50, 60, 255],
        });
        let bytes = enc.encode().expect("encode should succeed");
        assert!(bytes.starts_with(b"\x89PNG"));
    }

    #[cfg(feature = "encode")]
    fn rgba_frame() -> EncodeFrame {
        EncodeFrame {
            width: 4,
            height: 4,
            stride: 16,
            format: MemoryFormat::R8g8b8a8,
            data: vec![10, 20, 30, 255].repeat(16),
        }
    }

    #[cfg(feature = "encode")]
    #[test]
    fn density_rejected_for_formats_without_support() {
        use crate::density::{PhysicalDimensionUnit, PixelDensity};

        let density = PixelDensity::new(
            300.0,
            PhysicalDimensionUnit::Inch,
            300.0,
            PhysicalDimensionUnit::Inch,
        );
        for target in [KnownFormat::Gif, KnownFormat::WebP, KnownFormat::Bmp] {
            let mut enc = Encoder::new(target).unwrap();
            assert!(!enc.set_pixel_density(Some(density)), "{target:?}");
            assert!(enc.set_pixel_density(None), "{target:?}");
        }
        for target in [KnownFormat::Png, KnownFormat::Jpeg, KnownFormat::Tiff] {
            let mut enc = Encoder::new(target).unwrap();
            assert!(enc.set_pixel_density(Some(density)), "{target:?}");
        }
    }

    #[cfg(all(feature = "encode", feature = "png"))]
    #[test]
    fn png_encode_embeds_meter_density() {
        use crate::density::{PhysicalDimensionUnit, PixelDensity};

        let mut enc = Encoder::new(KnownFormat::Png).unwrap();
        enc.add_frame(rgba_frame());
        enc.set_pixel_density(Some(PixelDensity::new(
            300.0,
            PhysicalDimensionUnit::Inch,
            300.0,
            PhysicalDimensionUnit::Inch,
        )));
        let bytes = enc.encode().expect("encode should succeed");
        let image = crate::Loader::new_bytes(bytes).load().unwrap();
        let density = image.pixel_density().expect("density");
        assert_eq!(density.x_unit, PhysicalDimensionUnit::Meter);
        assert!(
            (density.x_value - 300.0 / 0.0254).abs() < 1.0,
            "{}",
            density.x_value
        );
    }

    #[cfg(all(feature = "encode", feature = "jpeg"))]
    #[test]
    fn jpeg_encode_embeds_jfif_density() {
        use crate::density::{PhysicalDimensionUnit, PixelDensity};

        let mut enc = Encoder::new(KnownFormat::Jpeg).unwrap();
        enc.add_frame(rgba_frame());
        enc.set_pixel_density(Some(PixelDensity::new(
            300.0,
            PhysicalDimensionUnit::Inch,
            300.0,
            PhysicalDimensionUnit::Inch,
        )));
        let bytes = enc.encode().expect("encode should succeed");
        assert_eq!(&bytes[..2], &[0xFF, 0xD8]);
        let image = crate::Loader::new_bytes(bytes).load().unwrap();
        let density = image.pixel_density().expect("density");
        assert_eq!(density.x_unit, PhysicalDimensionUnit::Inch);
        assert_eq!(density.x_value, 300.0);
    }

    #[cfg(all(feature = "encode", feature = "tiff"))]
    #[test]
    fn tiff_encode_embeds_resolution_tags() {
        use crate::density::{PhysicalDimensionUnit, PixelDensity};

        let mut enc = Encoder::new(KnownFormat::Tiff).unwrap();
        enc.add_frame(rgba_frame());
        enc.set_pixel_density(Some(PixelDensity::new(
            300.0,
            PhysicalDimensionUnit::Inch,
            300.0,
            PhysicalDimensionUnit::Inch,
        )));
        let bytes = enc.encode().expect("encode should succeed");
        let image = crate::Loader::new_bytes(bytes).load().unwrap();
        let density = image.pixel_density().expect("density");
        assert_eq!(density.x_unit, PhysicalDimensionUnit::Inch);
        assert_eq!(density.x_value, 300.0);
    }
}
