//! Memory-format conversion for `gly_loader_set_accepted_memory_formats`.
//!
//! Upstream glycin honours the caller-supplied
//! `GlyMemoryFormatSelection` bitmask: if the decoded frame's format
//! is not in the set, the loader converts it before returning. Our
//! decoders almost always produce a straight-alpha 8-bit format, but
//! the SVG path (via `resvg`) produces `R8g8b8a8Premultiplied`. If
//! the consumer (typically gdk-pixbuf, which has no premultiplied
//! surface) gets that without knowing it, GTK's icon-recolor mask
//! reads the wrong alpha and symbolic icons appear blank.
//!
//! This module implements the minimum subset of upstream's
//! `change_memory_format` we need: pick the best target format from
//! the caller's selection and convert the frame's bytes to it. For
//! source/target pairs we do not handle yet, we return `None` and
//! the caller leaves the frame untouched (graceful degradation).
//!
//! The selection bit values come from `GlyMemoryFormatSelection` in
//! upstream's `glycin_common`; see `memformat.rs` for the byte-order
//! mapping into glycin-ng's [`MemoryFormat`].

use glycin_ng::{Frame, MemoryFormat, Texture};

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

/// Convert `frame` so the consumer-facing texture sits in one of the
/// formats permitted by `selection`. Returns the (possibly rebuilt)
/// frame plus the actually-applied format. If `selection` is empty
/// or the source format is already accepted, or we cannot satisfy
/// the request, the input frame is returned unchanged.
pub(crate) fn maybe_convert(frame: Frame, selection_bits: u32) -> Frame {
    if selection_bits == 0 {
        return frame;
    }
    let selection = Selection::from_bits_truncate(selection_bits);
    let src_format = frame.texture().format();
    if selection_contains(selection, src_format) {
        return frame;
    }
    let Some(target) = pick_target(src_format, selection) else {
        return frame;
    };
    let Some(new_tex) = convert_texture(frame.texture(), target) else {
        return frame;
    };
    Frame::new(new_tex, frame.delay())
}

fn selection_contains(selection: Selection, fmt: MemoryFormat) -> bool {
    selection.contains(selection_bit_for(fmt))
}

fn selection_bit_for(fmt: MemoryFormat) -> Selection {
    match fmt {
        MemoryFormat::B8g8r8a8Premultiplied => Selection::B8G8R8A8_PREMULTIPLIED,
        MemoryFormat::A8r8g8b8Premultiplied => Selection::A8R8G8B8_PREMULTIPLIED,
        MemoryFormat::R8g8b8a8Premultiplied => Selection::R8G8B8A8_PREMULTIPLIED,
        MemoryFormat::B8g8r8a8 => Selection::B8G8R8A8,
        MemoryFormat::A8r8g8b8 => Selection::A8R8G8B8,
        MemoryFormat::R8g8b8a8 => Selection::R8G8B8A8,
        MemoryFormat::A8b8g8r8 => Selection::A8B8G8R8,
        MemoryFormat::R8g8b8 => Selection::R8G8B8,
        MemoryFormat::B8g8r8 => Selection::B8G8R8,
        MemoryFormat::R16g16b16 => Selection::R16G16B16,
        MemoryFormat::R16g16b16a16Premultiplied => Selection::R16G16B16A16_PREMULTIPLIED,
        MemoryFormat::R16g16b16a16 => Selection::R16G16B16A16,
        MemoryFormat::R16g16b16Float => Selection::R16G16B16_FLOAT,
        MemoryFormat::R16g16b16a16Float => Selection::R16G16B16A16_FLOAT,
        MemoryFormat::R32g32b32Float => Selection::R32G32B32_FLOAT,
        MemoryFormat::R32g32b32a32FloatPremultiplied => Selection::R32G32B32A32_FLOAT_PREMULTIPLIED,
        MemoryFormat::R32g32b32a32Float => Selection::R32G32B32A32_FLOAT,
        MemoryFormat::G8a8Premultiplied => Selection::G8A8_PREMULTIPLIED,
        MemoryFormat::G8a8 => Selection::G8A8,
        MemoryFormat::G8 => Selection::G8,
        MemoryFormat::G16a16Premultiplied => Selection::G16A16_PREMULTIPLIED,
        MemoryFormat::G16a16 => Selection::G16A16,
        MemoryFormat::G16 => Selection::G16,
        _ => Selection::empty(),
    }
}

/// Pick the best accepted format to represent `src`. Prefers an
/// 8-bit straight-alpha format (what gdk-pixbuf consumes) when the
/// source has alpha; otherwise prefers a same-width opaque format.
fn pick_target(src: MemoryFormat, selection: Selection) -> Option<MemoryFormat> {
    let has_alpha = matches!(
        src,
        MemoryFormat::R8g8b8a8
            | MemoryFormat::R8g8b8a8Premultiplied
            | MemoryFormat::B8g8r8a8
            | MemoryFormat::B8g8r8a8Premultiplied
            | MemoryFormat::A8r8g8b8
            | MemoryFormat::A8r8g8b8Premultiplied
            | MemoryFormat::A8b8g8r8
            | MemoryFormat::G8a8
            | MemoryFormat::G8a8Premultiplied
            | MemoryFormat::G16a16
            | MemoryFormat::G16a16Premultiplied
            | MemoryFormat::R16g16b16a16
            | MemoryFormat::R16g16b16a16Premultiplied
            | MemoryFormat::R16g16b16a16Float
            | MemoryFormat::R32g32b32a32Float
            | MemoryFormat::R32g32b32a32FloatPremultiplied
    );

    let candidates: &[(Selection, MemoryFormat)] = if has_alpha {
        &[
            (Selection::R8G8B8A8, MemoryFormat::R8g8b8a8),
            (Selection::B8G8R8A8, MemoryFormat::B8g8r8a8),
            (Selection::A8R8G8B8, MemoryFormat::A8r8g8b8),
            (Selection::A8B8G8R8, MemoryFormat::A8b8g8r8),
            (Selection::R8G8B8, MemoryFormat::R8g8b8),
            (Selection::B8G8R8, MemoryFormat::B8g8r8),
        ]
    } else {
        &[
            (Selection::R8G8B8, MemoryFormat::R8g8b8),
            (Selection::B8G8R8, MemoryFormat::B8g8r8),
            (Selection::R8G8B8A8, MemoryFormat::R8g8b8a8),
            (Selection::B8G8R8A8, MemoryFormat::B8g8r8a8),
        ]
    };
    candidates
        .iter()
        .find(|(bit, _)| selection.contains(*bit))
        .map(|(_, fmt)| *fmt)
}

fn convert_texture(src: &Texture, target: MemoryFormat) -> Option<Texture> {
    let w = src.width();
    let h = src.height();
    let bpp_out = target.bytes_per_pixel() as u32;
    let stride_out = w.checked_mul(bpp_out)?;
    let mut out: Vec<u8> = Vec::with_capacity((stride_out as usize) * (h as usize));
    let src_data = src.data();
    let src_stride = src.stride() as usize;
    let src_bpp = src.format().bytes_per_pixel() as usize;

    for y in 0..(h as usize) {
        let row = &src_data[y * src_stride..y * src_stride + (w as usize) * src_bpp];
        for x in 0..(w as usize) {
            let p = &row[x * src_bpp..x * src_bpp + src_bpp];
            let (r, g, b, a) = sample_rgba8(src.format(), p)?;
            emit_pixel(target, r, g, b, a, &mut out);
        }
    }

    Texture::from_parts(w, h, stride_out, target, out.into_boxed_slice())
}

fn sample_rgba8(fmt: MemoryFormat, p: &[u8]) -> Option<(u8, u8, u8, u8)> {
    match fmt {
        MemoryFormat::G8 => Some((p[0], p[0], p[0], 255)),
        MemoryFormat::G8a8 => Some((p[0], p[0], p[0], p[1])),
        MemoryFormat::G8a8Premultiplied => {
            let (g, a) = unpremul_g8(p[0], p[1]);
            Some((g, g, g, a))
        }
        MemoryFormat::R8g8b8 => Some((p[0], p[1], p[2], 255)),
        MemoryFormat::R8g8b8a8 => Some((p[0], p[1], p[2], p[3])),
        MemoryFormat::R8g8b8a8Premultiplied => {
            let (r, g, b, a) = unpremul_rgb8(p[0], p[1], p[2], p[3]);
            Some((r, g, b, a))
        }
        MemoryFormat::B8g8r8 => Some((p[2], p[1], p[0], 255)),
        MemoryFormat::B8g8r8a8 => Some((p[2], p[1], p[0], p[3])),
        MemoryFormat::B8g8r8a8Premultiplied => {
            let (r, g, b, a) = unpremul_rgb8(p[2], p[1], p[0], p[3]);
            Some((r, g, b, a))
        }
        MemoryFormat::A8r8g8b8 => Some((p[1], p[2], p[3], p[0])),
        MemoryFormat::A8r8g8b8Premultiplied => {
            let (r, g, b, a) = unpremul_rgb8(p[1], p[2], p[3], p[0]);
            Some((r, g, b, a))
        }
        MemoryFormat::A8b8g8r8 => Some((p[3], p[2], p[1], p[0])),
        _ => None,
    }
}

fn emit_pixel(target: MemoryFormat, r: u8, g: u8, b: u8, a: u8, out: &mut Vec<u8>) {
    match target {
        MemoryFormat::R8g8b8 => out.extend_from_slice(&[r, g, b]),
        MemoryFormat::B8g8r8 => out.extend_from_slice(&[b, g, r]),
        MemoryFormat::R8g8b8a8 => out.extend_from_slice(&[r, g, b, a]),
        MemoryFormat::B8g8r8a8 => out.extend_from_slice(&[b, g, r, a]),
        MemoryFormat::A8r8g8b8 => out.extend_from_slice(&[a, r, g, b]),
        MemoryFormat::A8b8g8r8 => out.extend_from_slice(&[a, b, g, r]),
        _ => out.extend_from_slice(&[r, g, b, a]),
    }
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

    fn tex(fmt: MemoryFormat, data: Vec<u8>, w: u32, h: u32) -> Texture {
        let stride = w * fmt.bytes_per_pixel() as u32;
        Texture::from_parts(w, h, stride, fmt, data.into_boxed_slice()).unwrap()
    }

    #[test]
    fn empty_selection_is_passthrough() {
        let t = tex(
            MemoryFormat::R8g8b8a8Premultiplied,
            vec![64, 64, 64, 128],
            1,
            1,
        );
        let f = Frame::new(t, None);
        let out = maybe_convert(f, 0);
        assert_eq!(out.texture().format(), MemoryFormat::R8g8b8a8Premultiplied);
    }

    #[test]
    fn premul_rgba_unpremultiplies_when_caller_wants_straight() {
        // Premul (64, 64, 64, 128) -> straight (128, 128, 128, 128).
        let t = tex(
            MemoryFormat::R8g8b8a8Premultiplied,
            vec![64, 64, 64, 128],
            1,
            1,
        );
        let f = Frame::new(t, None);
        let out = maybe_convert(f, Selection::R8G8B8A8.bits());
        assert_eq!(out.texture().format(), MemoryFormat::R8g8b8a8);
        assert_eq!(out.texture().data(), &[127, 127, 127, 128]);
    }

    #[test]
    fn straight_rgba_is_left_alone_when_accepted() {
        let t = tex(MemoryFormat::R8g8b8a8, vec![10, 20, 30, 40], 1, 1);
        let f = Frame::new(t, None);
        let out = maybe_convert(f, Selection::R8G8B8A8.bits());
        assert_eq!(out.texture().format(), MemoryFormat::R8g8b8a8);
        assert_eq!(out.texture().data(), &[10, 20, 30, 40]);
    }

    #[test]
    fn rgb8_to_rgba8_adds_opaque_alpha() {
        let t = tex(MemoryFormat::R8g8b8, vec![10, 20, 30], 1, 1);
        let f = Frame::new(t, None);
        let out = maybe_convert(f, Selection::R8G8B8A8.bits());
        assert_eq!(out.texture().format(), MemoryFormat::R8g8b8a8);
        assert_eq!(out.texture().data(), &[10, 20, 30, 255]);
    }
}
