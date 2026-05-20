//! SVG decode path through `resvg`.
//!
//! Compiled only when the `svg` feature is on. Returns a
//! [`glycin_ng::Image`] containing a single RGBA8 frame at the SVG's
//! intrinsic size, or scaled to the requested target dimensions when
//! the caller provided them via `gly_frame_request_set_scale`.
//!
//! License note: the `resvg` family of crates is MPL-2.0. Builds
//! that enable this feature are MPL "Larger Work" combinations; the
//! `resvg` source files retain their MPL-2.0 terms.

use glycin_ng::{Frame, Image, MemoryFormat, Texture};

use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{Options, Tree};

/// Best-effort SVG detection on the first few hundred bytes.
///
/// Accepts an optional XML declaration, BOM, leading whitespace, and
/// then `<svg`. This is loose enough to catch most real-world SVGs
/// while not matching arbitrary XML.
pub(crate) fn looks_like_svg(bytes: &[u8]) -> bool {
    let probe_len = bytes.len().min(512);
    let head = &bytes[..probe_len];
    let trimmed = head.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(head);
    let trimmed = skip_whitespace(trimmed);
    if trimmed.starts_with(b"<?xml") {
        return find_svg_open_tag(trimmed);
    }
    starts_with_svg_open_tag(trimmed)
}

fn starts_with_svg_open_tag(b: &[u8]) -> bool {
    if !b.starts_with(b"<svg") {
        return false;
    }
    // The byte after `<svg` must be whitespace, `>`, or `/` (a
    // self-closing tag) to count as the SVG root element.
    matches!(b.get(4), Some(b' ' | b'\t' | b'\r' | b'\n' | b'>' | b'/'))
}

fn find_svg_open_tag(haystack: &[u8]) -> bool {
    let needle = b"<svg";
    if haystack.len() < needle.len() {
        return false;
    }
    for i in 0..=haystack.len() - needle.len() {
        if &haystack[i..i + needle.len()] == needle {
            let suffix = &haystack[i..];
            if starts_with_svg_open_tag(suffix) {
                return true;
            }
        }
    }
    false
}

fn skip_whitespace(b: &[u8]) -> &[u8] {
    let mut i = 0;
    while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\r' | b'\n') {
        i += 1;
    }
    &b[i..]
}


/// Decode `bytes` into a single-frame [`Image`].
///
/// When `target` is `Some((w, h))` the SVG is rasterized at that
/// size; otherwise it uses the SVG's intrinsic size. Failure
/// surfaces as a string error.
pub(crate) fn decode(bytes: &[u8], target: Option<(u32, u32)>) -> Result<Image, String> {
    let opt = Options::default();
    let tree = Tree::from_data(bytes, &opt).map_err(|e| e.to_string())?;
    let svg_size = tree.size();
    let (out_w, out_h) = match target {
        Some((w, h)) if w > 0 && h > 0 => (w, h),
        _ => (
            svg_size.width().ceil().max(1.0) as u32,
            svg_size.height().ceil().max(1.0) as u32,
        ),
    };
    let mut pixmap = Pixmap::new(out_w, out_h).ok_or_else(|| {
        format!("failed to allocate {out_w}x{out_h} pixmap")
    })?;
    let transform = Transform::from_scale(
        out_w as f32 / svg_size.width(),
        out_h as f32 / svg_size.height(),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // tiny-skia hands back premultiplied RGBA8 by default.
    let stride = out_w
        .checked_mul(4)
        .ok_or_else(|| "stride overflow".to_string())?;
    let texture = Texture::from_parts(
        out_w,
        out_h,
        stride,
        MemoryFormat::R8g8b8a8Premultiplied,
        pixmap.data().to_vec().into_boxed_slice(),
    )
    .ok_or_else(|| "texture construction failed".to_string())?;
    let frame = Frame::new(texture, None);
    Ok(Image::from_parts("svg", out_w, out_h, vec![frame]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_xml_decl_then_svg() {
        let bytes = b"<?xml version=\"1.0\"?><svg xmlns=\"http://www.w3.org/2000/svg\"/>";
        assert!(looks_like_svg(bytes));
    }

    #[test]
    fn detects_bare_svg() {
        assert!(looks_like_svg(b"<svg width=\"10\" height=\"10\"/>"));
    }

    #[test]
    fn detects_svg_after_leading_whitespace() {
        assert!(looks_like_svg(b"  \n\t<svg/>"));
    }

    #[test]
    fn detects_svg_with_bom() {
        let mut buf = vec![0xEF, 0xBB, 0xBF];
        buf.extend_from_slice(b"<svg/>");
        assert!(looks_like_svg(&buf));
    }

    #[test]
    fn rejects_non_svg() {
        assert!(!looks_like_svg(b"\x89PNG\r\n\x1a\n"));
        assert!(!looks_like_svg(b"<?xml version=\"1.0\"?><html/>"));
        assert!(!looks_like_svg(b""));
        assert!(!looks_like_svg(b"<svg-but-not-quite"));
    }
}
