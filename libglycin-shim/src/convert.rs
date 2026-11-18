//! Memory-format conversion for `gly_loader_set_accepted_memory_formats`.
//!
//! Upstream glycin honours the caller-supplied
//! `GlyMemoryFormatSelection` bitmask: if the decoded frame's format
//! is not in the set, the loader converts it before returning. Our
//! decoders almost always produce a straight-alpha 8-bit format, but
//! the SVG path produces `R8G8B8A8Premultiplied`. If the consumer
//! (typically gdk-pixbuf, which has no premultiplied surface) gets
//! that without knowing it, GTK's icon-recolor mask reads the wrong
//! alpha and symbolic icons appear blank.
//!
//! This module implements the minimum subset of upstream's
//! `change_memory_format` we need: pick the best target format from
//! the caller's selection and convert the frame's bytes to it.

use std::ffi::c_uint;

use crate::memformat;
use crate::ngapi;

bitflags::bitflags! {
    /// Mirror of `GlyMemoryFormatSelection` from `glycin_common`.
    /// Bit positions are stable ABI; do not reorder.
    #[derive(Debug, Clone, Copy)]
    pub(crate) struct Selection: u32 {
        const B8G8R8A8_PREMULTIPLIED        = 1 << 0;
        const A8R8G8B8_PREMULTIPLIED        = 1 << 1;
        const R8G8B8A8_PREMULTIPLIED        = 1 << 2;
        const B8G8R8A8                      = 1 << 3;
        const A8R8G8B8                      = 1 << 4;
        const R8G8B8A8                      = 1 << 5;
        const A8B8G8R8                      = 1 << 6;
        const R8G8B8                        = 1 << 7;
        const B8G8R8                        = 1 << 8;
        const R16G16B16                     = 1 << 9;
        const R16G16B16A16_PREMULTIPLIED    = 1 << 10;
        const R16G16B16A16                  = 1 << 11;
        const R16G16B16_FLOAT               = 1 << 12;
        const R16G16B16A16_FLOAT            = 1 << 13;
        const R32G32B32_FLOAT               = 1 << 14;
        const R32G32B32A32_FLOAT_PREMULTIPLIED = 1 << 15;
        const R32G32B32A32_FLOAT            = 1 << 16;
        const G8A8_PREMULTIPLIED            = 1 << 17;
        const G8A8                          = 1 << 18;
        const G8                            = 1 << 19;
        const G16A16_PREMULTIPLIED          = 1 << 20;
        const G16A16                        = 1 << 21;
        const G16                           = 1 << 22;
    }
}

/// Raw frame as it sits inside [`FrameState`](crate::types::FrameState).
#[derive(Clone)]
pub(crate) struct RawFrame {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: c_uint,
    pub data: Vec<u8>,
    pub delay_ms: u64,
}

/// Convert `frame` so the consumer-facing texture sits in one of the
/// formats permitted by `selection_bits`. If `selection_bits == 0`
/// (caller didn't call `gly_loader_set_accepted_memory_formats`), or
/// the frame's format already satisfies the selection, the input is
/// returned unchanged. If the conversion is not implemented for the
/// (source, target) pair the frame is also returned unchanged.
pub(crate) fn maybe_convert(frame: RawFrame, selection_bits: u32) -> RawFrame {
    if selection_bits == 0 {
        return frame;
    }
    let selection = Selection::from_bits_truncate(selection_bits);
    if selection_accepts(selection, frame.format) {
        return frame;
    }
    let Some(target) = pick_target(frame.format, selection) else {
        return frame;
    };
    convert_layout(&frame, target).unwrap_or(frame)
}

fn selection_accepts(selection: Selection, ng_format: c_uint) -> bool {
    let gly = memformat::ng_to_gly(ng_format);
    if !(0..32).contains(&gly) {
        return false;
    }
    let Some(bit) = Selection::from_bits_truncate(1u32 << gly).bits().into() else {
        return false;
    };
    selection.bits() & bit != 0
}

fn pick_target(src: c_uint, selection: Selection) -> Option<c_uint> {
    // Prefer the straight-alpha equivalent of the source, then a
    // small priority list of widely-supported destinations.
    let straight = match src {
        ngapi::GLYCIN_NG_FORMAT_R8G8B8A8_PRE => Some(ngapi::GLYCIN_NG_FORMAT_R8G8B8A8),
        ngapi::GLYCIN_NG_FORMAT_B8G8R8A8_PRE => Some(ngapi::GLYCIN_NG_FORMAT_B8G8R8A8),
        ngapi::GLYCIN_NG_FORMAT_A8R8G8B8_PRE => Some(ngapi::GLYCIN_NG_FORMAT_A8R8G8B8),
        ngapi::GLYCIN_NG_FORMAT_G8A8_PRE => Some(ngapi::GLYCIN_NG_FORMAT_G8A8),
        _ => None,
    };
    let priority: &[c_uint] = &[
        ngapi::GLYCIN_NG_FORMAT_R8G8B8A8,
        ngapi::GLYCIN_NG_FORMAT_B8G8R8A8,
        ngapi::GLYCIN_NG_FORMAT_A8R8G8B8,
        ngapi::GLYCIN_NG_FORMAT_R8G8B8,
        ngapi::GLYCIN_NG_FORMAT_B8G8R8,
        ngapi::GLYCIN_NG_FORMAT_G8A8,
        ngapi::GLYCIN_NG_FORMAT_G8,
    ];
    straight
        .into_iter()
        .chain(priority.iter().copied())
        .find(|&fmt| selection_accepts(selection, fmt))
}

fn convert_layout(src: &RawFrame, target: c_uint) -> Option<RawFrame> {
    let src_bpp = memformat::bytes_per_pixel_ng(src.format);
    let dst_bpp = memformat::bytes_per_pixel_ng(target);
    if src_bpp == 0 || dst_bpp == 0 {
        return None;
    }
    let width = src.width as usize;
    let height = src.height as usize;
    let src_stride = src.stride as usize;
    if src_stride < width * src_bpp {
        return None;
    }
    let dst_stride = width * dst_bpp;
    let mut out = Vec::with_capacity(dst_stride * height);

    for y in 0..height {
        let row_off = y * src_stride;
        let row = &src.data.get(row_off..row_off + width * src_bpp)?;
        for x in 0..width {
            let px = &row[x * src_bpp..x * src_bpp + src_bpp];
            let (r, g, b, a) = sample_rgba8(src.format, px)?;
            emit_pixel(target, r, g, b, a, &mut out);
        }
    }

    Some(RawFrame {
        width: src.width,
        height: src.height,
        stride: dst_stride as u32,
        format: target,
        data: out,
        delay_ms: src.delay_ms,
    })
}

/// Reads one pixel as straight 8-bit RGBA. Samples are native endian.
/// 16-bit samples are reduced to their high byte, after unpremultiplying
/// at full precision. Float samples are clamped to `0.0..=1.0` and
/// scaled.
fn sample_rgba8(fmt: c_uint, p: &[u8]) -> Option<(u8, u8, u8, u8)> {
    let u16_at = |i: usize| u16::from_ne_bytes([p[2 * i], p[2 * i + 1]]);
    let hi = |i: usize| (u16_at(i) >> 8) as u8;
    let f32_at =
        |i: usize| f32::from_ne_bytes([p[4 * i], p[4 * i + 1], p[4 * i + 2], p[4 * i + 3]]);
    Some(match fmt {
        ngapi::GLYCIN_NG_FORMAT_G16 => (hi(0), hi(0), hi(0), 255),
        ngapi::GLYCIN_NG_FORMAT_G16A16 => (hi(0), hi(0), hi(0), hi(1)),
        ngapi::GLYCIN_NG_FORMAT_G16A16_PRE => {
            let g = unpremul16(u16_at(0), u16_at(1));
            (g, g, g, hi(1))
        }
        ngapi::GLYCIN_NG_FORMAT_R16G16B16 => (hi(0), hi(1), hi(2), 255),
        ngapi::GLYCIN_NG_FORMAT_R16G16B16A16 => (hi(0), hi(1), hi(2), hi(3)),
        ngapi::GLYCIN_NG_FORMAT_R16G16B16A16_PRE => {
            let a = u16_at(3);
            (
                unpremul16(u16_at(0), a),
                unpremul16(u16_at(1), a),
                unpremul16(u16_at(2), a),
                hi(3),
            )
        }
        ngapi::GLYCIN_NG_FORMAT_R32G32B32_F => {
            (unorm8(f32_at(0)), unorm8(f32_at(1)), unorm8(f32_at(2)), 255)
        }
        ngapi::GLYCIN_NG_FORMAT_R32G32B32A32_F => (
            unorm8(f32_at(0)),
            unorm8(f32_at(1)),
            unorm8(f32_at(2)),
            unorm8(f32_at(3)),
        ),
        ngapi::GLYCIN_NG_FORMAT_R32G32B32A32_F_PRE => {
            let a = f32_at(3);
            let unpremul = |c: f32| if a > 0.0 { unorm8(c / a) } else { 0 };
            (
                unpremul(f32_at(0)),
                unpremul(f32_at(1)),
                unpremul(f32_at(2)),
                unorm8(a),
            )
        }
        ngapi::GLYCIN_NG_FORMAT_G8 => (p[0], p[0], p[0], 255),
        ngapi::GLYCIN_NG_FORMAT_G8A8 => (p[0], p[0], p[0], p[1]),
        ngapi::GLYCIN_NG_FORMAT_G8A8_PRE => {
            let (g, a) = unpremul_g8(p[0], p[1]);
            (g, g, g, a)
        }
        ngapi::GLYCIN_NG_FORMAT_R8G8B8 => (p[0], p[1], p[2], 255),
        ngapi::GLYCIN_NG_FORMAT_B8G8R8 => (p[2], p[1], p[0], 255),
        ngapi::GLYCIN_NG_FORMAT_R8G8B8A8 => (p[0], p[1], p[2], p[3]),
        ngapi::GLYCIN_NG_FORMAT_R8G8B8A8_PRE => unpremul_rgb8(p[0], p[1], p[2], p[3]),
        ngapi::GLYCIN_NG_FORMAT_B8G8R8A8 => (p[2], p[1], p[0], p[3]),
        ngapi::GLYCIN_NG_FORMAT_B8G8R8A8_PRE => unpremul_rgb8(p[2], p[1], p[0], p[3]),
        ngapi::GLYCIN_NG_FORMAT_A8R8G8B8 => (p[1], p[2], p[3], p[0]),
        ngapi::GLYCIN_NG_FORMAT_A8R8G8B8_PRE => unpremul_rgb8(p[1], p[2], p[3], p[0]),
        ngapi::GLYCIN_NG_FORMAT_A8B8G8R8 => (p[3], p[2], p[1], p[0]),
        _ => return None,
    })
}

fn emit_pixel(target: c_uint, r: u8, g: u8, b: u8, a: u8, out: &mut Vec<u8>) {
    match target {
        ngapi::GLYCIN_NG_FORMAT_R8G8B8A8 => out.extend_from_slice(&[r, g, b, a]),
        ngapi::GLYCIN_NG_FORMAT_B8G8R8A8 => out.extend_from_slice(&[b, g, r, a]),
        ngapi::GLYCIN_NG_FORMAT_A8R8G8B8 => out.extend_from_slice(&[a, r, g, b]),
        ngapi::GLYCIN_NG_FORMAT_A8B8G8R8 => out.extend_from_slice(&[a, b, g, r]),
        ngapi::GLYCIN_NG_FORMAT_R8G8B8 => out.extend_from_slice(&[r, g, b]),
        ngapi::GLYCIN_NG_FORMAT_B8G8R8 => out.extend_from_slice(&[b, g, r]),
        ngapi::GLYCIN_NG_FORMAT_G8A8 => {
            let y = luma(r, g, b);
            out.extend_from_slice(&[y, a]);
        }
        ngapi::GLYCIN_NG_FORMAT_G8 => out.push(luma(r, g, b)),
        _ => out.extend_from_slice(&[r, g, b, a]),
    }
}

fn luma(r: u8, g: u8, b: u8) -> u8 {
    let v = 30 * r as u32 + 59 * g as u32 + 11 * b as u32;
    (v / 100) as u8
}

fn unorm8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn unpremul16(c: u16, a: u16) -> u8 {
    if a == 0 {
        return 0;
    }
    ((c as u32 * 65535 / a as u32).min(65535) >> 8) as u8
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

    fn frame(format: c_uint, data: Vec<u8>, w: u32, h: u32) -> RawFrame {
        let stride = w * memformat::bytes_per_pixel_ng(format) as u32;
        RawFrame {
            width: w,
            height: h,
            stride,
            format,
            data,
            delay_ms: 0,
        }
    }

    #[test]
    fn empty_selection_is_passthrough() {
        let f = frame(
            ngapi::GLYCIN_NG_FORMAT_R8G8B8A8_PRE,
            vec![64, 64, 64, 128],
            1,
            1,
        );
        let out = maybe_convert(f, 0);
        assert_eq!(out.format, ngapi::GLYCIN_NG_FORMAT_R8G8B8A8_PRE);
    }

    #[test]
    fn premul_unpremultiplies_when_caller_wants_straight() {
        let f = frame(
            ngapi::GLYCIN_NG_FORMAT_R8G8B8A8_PRE,
            vec![64, 64, 64, 128],
            1,
            1,
        );
        let out = maybe_convert(f, Selection::R8G8B8A8.bits());
        assert_eq!(out.format, ngapi::GLYCIN_NG_FORMAT_R8G8B8A8);
        assert_eq!(out.data, &[127, 127, 127, 128]);
    }

    #[test]
    fn already_accepted_is_passthrough() {
        let f = frame(ngapi::GLYCIN_NG_FORMAT_R8G8B8A8, vec![10, 20, 30, 40], 1, 1);
        let out = maybe_convert(f, Selection::R8G8B8A8.bits());
        assert_eq!(out.format, ngapi::GLYCIN_NG_FORMAT_R8G8B8A8);
        assert_eq!(out.data, &[10, 20, 30, 40]);
    }

    #[test]
    fn rgb_to_rgba_adds_opaque_alpha() {
        let f = frame(ngapi::GLYCIN_NG_FORMAT_R8G8B8, vec![10, 20, 30], 1, 1);
        let out = maybe_convert(f, Selection::R8G8B8A8.bits());
        assert_eq!(out.format, ngapi::GLYCIN_NG_FORMAT_R8G8B8A8);
        assert_eq!(out.data, &[10, 20, 30, 255]);
    }

    // gdk-pixbuf only accepts R8G8B8A8/R8G8B8 and sizes the pixbuf
    // from the channel count, so a gray frame reaching it unconverted
    // is a crash, not a cosmetic issue.
    #[test]
    fn gray_alpha_to_rgba_expands_channels() {
        let f = frame(ngapi::GLYCIN_NG_FORMAT_G8A8, vec![40, 200], 1, 1);
        let out = maybe_convert(f, Selection::R8G8B8A8.bits() | Selection::R8G8B8.bits());
        assert_eq!(out.format, ngapi::GLYCIN_NG_FORMAT_R8G8B8A8);
        assert_eq!(out.stride, 4);
        assert_eq!(out.data, &[40, 40, 40, 200]);
    }

    #[test]
    fn gray_to_rgba_expands_channels() {
        let f = frame(ngapi::GLYCIN_NG_FORMAT_G8, vec![40], 1, 1);
        let out = maybe_convert(f, Selection::R8G8B8A8.bits() | Selection::R8G8B8.bits());
        assert_eq!(out.format, ngapi::GLYCIN_NG_FORMAT_R8G8B8A8);
        assert_eq!(out.data, &[40, 40, 40, 255]);
    }

    #[test]
    fn unknown_selection_bits_are_ignored() {
        let f = frame(ngapi::GLYCIN_NG_FORMAT_G8, vec![40], 1, 1);
        let out = maybe_convert(f, Selection::R8G8B8A8.bits() | 1 << 31);
        assert_eq!(out.format, ngapi::GLYCIN_NG_FORMAT_R8G8B8A8);
    }

    #[test]
    fn sixteen_bit_reduces_to_high_byte() {
        let px = [0x1234u16, 0xabcd]
            .iter()
            .flat_map(|v| v.to_ne_bytes())
            .collect();
        let f = frame(ngapi::GLYCIN_NG_FORMAT_G16A16, px, 1, 1);
        let out = maybe_convert(f, Selection::R8G8B8A8.bits() | Selection::R8G8B8.bits());
        assert_eq!(out.format, ngapi::GLYCIN_NG_FORMAT_R8G8B8A8);
        assert_eq!(out.data, &[0x12, 0x12, 0x12, 0xab]);
    }

    const RGBA8: u32 = Selection::R8G8B8A8.bits() | Selection::R8G8B8.bits();

    fn f32_bytes(v: &[f32]) -> Vec<u8> {
        v.iter().flat_map(|f| f.to_ne_bytes()).collect()
    }

    fn u16_bytes(v: &[u16]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_ne_bytes()).collect()
    }

    #[test]
    fn float_rgb_clamps_and_scales() {
        let f = frame(
            ngapi::GLYCIN_NG_FORMAT_R32G32B32_F,
            f32_bytes(&[1.0, 0.5, -0.5]),
            1,
            1,
        );
        let out = maybe_convert(f, RGBA8);
        assert_eq!(out.format, ngapi::GLYCIN_NG_FORMAT_R8G8B8A8);
        assert_eq!(out.data, &[255, 128, 0, 255]);
    }

    #[test]
    fn float_rgba_keeps_alpha() {
        let f = frame(
            ngapi::GLYCIN_NG_FORMAT_R32G32B32A32_F,
            f32_bytes(&[0.0, 1.5, 0.25, 0.25, 1.0, 1.0, 1.0, 0.0]),
            2,
            1,
        );
        let out = maybe_convert(f, RGBA8);
        assert_eq!(out.format, ngapi::GLYCIN_NG_FORMAT_R8G8B8A8);
        assert_eq!(out.stride, 8);
        assert_eq!(out.data, &[0, 255, 64, 64, 255, 255, 255, 0]);
    }

    #[test]
    fn float_premultiplied_is_unpremultiplied() {
        let f = frame(
            ngapi::GLYCIN_NG_FORMAT_R32G32B32A32_F_PRE,
            f32_bytes(&[0.25, 0.5, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0]),
            2,
            1,
        );
        let out = maybe_convert(f, RGBA8);
        assert_eq!(out.format, ngapi::GLYCIN_NG_FORMAT_R8G8B8A8);
        assert_eq!(out.data, &[128, 255, 0, 128, 0, 0, 0, 0]);
    }

    #[test]
    fn rgba16_premultiplied_is_unpremultiplied() {
        let f = frame(
            ngapi::GLYCIN_NG_FORMAT_R16G16B16A16_PRE,
            u16_bytes(&[0x4000, 0x8000, 0, 0x8000, 0, 0, 0, 0]),
            2,
            1,
        );
        let out = maybe_convert(f, RGBA8);
        assert_eq!(out.format, ngapi::GLYCIN_NG_FORMAT_R8G8B8A8);
        assert_eq!(out.data, &[127, 255, 0, 128, 0, 0, 0, 0]);
    }

    #[test]
    fn gray16_premultiplied_is_unpremultiplied() {
        let f = frame(
            ngapi::GLYCIN_NG_FORMAT_G16A16_PRE,
            u16_bytes(&[0x4000, 0x8000]),
            1,
            1,
        );
        let out = maybe_convert(f, RGBA8);
        assert_eq!(out.format, ngapi::GLYCIN_NG_FORMAT_R8G8B8A8);
        assert_eq!(out.data, &[127, 127, 127, 128]);
    }

    #[test]
    fn sixteen_bit_target_is_never_picked() {
        let f = frame(
            ngapi::GLYCIN_NG_FORMAT_G16A16_PRE,
            u16_bytes(&[0x4000, 0x8000]),
            1,
            1,
        );
        let out = maybe_convert(f, Selection::G16A16.bits());
        assert_eq!(out.format, ngapi::GLYCIN_NG_FORMAT_G16A16_PRE);
    }
}
