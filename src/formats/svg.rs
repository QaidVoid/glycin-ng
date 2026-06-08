//! SVG decoder via `resvg` and `usvg`.

mod xinclude;

use std::sync::{Arc, LazyLock};

use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{Options, Tree, fontdb};

use crate::{Error, Frame, Image, MemoryFormat, Result, Texture};

use super::DecodeOptions;

pub(crate) fn decode(bytes: &[u8], opts: &DecodeOptions) -> Result<Image> {
    let parse_opt = Options {
        fontdb: BUNDLED_FONTDB.clone(),
        ..Default::default()
    };
    let owned;
    let svg_bytes: &[u8] = match xinclude::expand(bytes) {
        Some(expanded) => {
            owned = expanded;
            &owned
        }
        None => bytes,
    };
    let tree =
        Tree::from_data(svg_bytes, &parse_opt).map_err(|e| Error::Malformed(e.to_string()))?;
    let svg_size = tree.size();
    let intrinsic_w = svg_size.width().ceil().max(1.0) as u32;
    let intrinsic_h = svg_size.height().ceil().max(1.0) as u32;
    let (width, height) = opts.render_size_hint.unwrap_or((intrinsic_w, intrinsic_h));
    let width = width.max(1);
    let height = height.max(1);
    opts.limits.check_dimensions(width, height, 1)?;

    let mut pixmap = Pixmap::new(width, height).ok_or_else(|| Error::Decoder {
        format: "svg",
        message: format!("failed to allocate {width}x{height} pixmap"),
    })?;
    let sx = width as f32 / intrinsic_w as f32;
    let sy = height as f32 / intrinsic_h as f32;
    let transform = Transform::from_scale(sx, sy);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // tiny_skia renders into a premultiplied buffer; un-premultiply so
    // SVG matches the straight-alpha output of every other decoder.
    // Consumers that treat the buffer as straight alpha (the common
    // case) otherwise darken semi-transparent regions into invisibility.
    let mut rgba = Vec::with_capacity(pixmap.data().len());
    for px in pixmap.pixels() {
        let c = px.demultiply();
        rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }

    let stride = width.checked_mul(4).ok_or(Error::LimitExceeded("stride"))?;
    let texture = Texture::from_parts(
        width,
        height,
        stride,
        MemoryFormat::R8g8b8a8,
        rgba.into_boxed_slice(),
    )
    .ok_or_else(|| Error::Decoder {
        format: "svg",
        message: "texture construction failed".into(),
    })?;

    let _ = opts.apply_transformations;
    Ok(Image::from_parts(
        "svg",
        width,
        height,
        vec![Frame::new(texture, None)],
    ))
}

/// Font database with a single bundled font, shared across every decode.
///
/// Building the database parses the embedded font, so it is done once and
/// the resulting `Arc` is cloned per decode. All five generic families
/// (serif, sans-serif, monospace, cursive, fantasy) are mapped to the
/// bundled face: usvg's default font selector falls back to `Family::Serif`
/// when a requested family is missing, so this single font handles all text
/// rendering without any system font discovery, keeping the sandbox tight.
static BUNDLED_FONTDB: LazyLock<Arc<fontdb::Database>> = LazyLock::new(|| {
    let mut db = fontdb::Database::new();
    db.load_font_data(include_bytes!("../../assets/Cantarell-Regular.ttf").to_vec());
    db.set_serif_family("Cantarell");
    db.set_sans_serif_family("Cantarell");
    db.set_monospace_family("Cantarell");
    db.set_cursive_family("Cantarell");
    db.set_fantasy_family("Cantarell");
    Arc::new(db)
});

#[cfg(test)]
mod tests {
    use base64::Engine;

    use super::*;
    use crate::Limits;

    fn opts() -> DecodeOptions {
        DecodeOptions {
            limits: Limits::default(),
            apply_transformations: true,
            render_size_hint: None,
        }
    }

    #[test]
    fn decodes_minimal_svg() {
        let bytes =
            b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"4\" height=\"4\"><rect width=\"4\" height=\"4\" fill=\"red\"/></svg>";
        let image = decode(bytes, &opts()).unwrap();
        assert_eq!(image.width(), 4);
        assert_eq!(image.height(), 4);
        let frame = image.first_frame().unwrap();
        assert_eq!(frame.texture().format(), MemoryFormat::R8g8b8a8);
        assert_eq!(frame.texture().data().len(), 4 * 4 * 4);
        // First pixel should be opaque red (255, 0, 0, 255).
        let data = frame.texture().data();
        assert_eq!(&data[0..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn rejects_garbage() {
        let err = decode(b"<svg garbage>not really svg", &opts()).unwrap_err();
        assert!(matches!(err, Error::Malformed(_) | Error::Decoder { .. }));
    }

    #[test]
    fn decodes_gtk_symbolic_recolor_wrapper() {
        // The outer wrapper GTK builds when loading a symbolic icon:
        // outer <svg> with a recolor <style>, then an <xi:include>
        // pulling the original 4x4 red SVG as a base64 data URI.
        // Without the xinclude pass this renders as a fully
        // transparent 16x16 image, which is what made the toolbar
        // icons disappear in Ristretto.
        let inner = b"<?xml version=\"1.0\"?><svg xmlns=\"http://www.w3.org/2000/svg\" width=\"4\" height=\"4\"><rect width=\"4\" height=\"4\" fill=\"#2e3436\"/></svg>";
        let inner_b64 = base64::engine::general_purpose::STANDARD.encode(inner);
        let wrapper = format!(
            r#"<?xml version="1.0"?>
<svg version="1.1" xmlns="http://www.w3.org/2000/svg" xmlns:xi="http://www.w3.org/2001/XInclude" width="4" height="4">
  <style>rect, path {{ fill: rgb(255, 0, 0) !important; }}</style>
  <g><xi:include href="data:text/xml;base64,{inner_b64}"/></g>
</svg>"#
        );

        let image = decode(wrapper.as_bytes(), &opts()).unwrap();
        let data = image.first_frame().unwrap().texture().data();
        let alpha_set = data.chunks_exact(4).filter(|p| p[3] != 0).count();
        assert!(
            alpha_set > 0,
            "expected non-transparent output after xi:include expansion"
        );
    }

    #[test]
    fn render_size_hint_scales_output() {
        // 4x4 SVG rendered at 32x32 via the hint. The vector grid
        // should fill the full pixmap, every pixel opaque red.
        let bytes = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"4\" height=\"4\"><rect width=\"4\" height=\"4\" fill=\"red\"/></svg>";
        let image = decode(
            bytes,
            &DecodeOptions {
                render_size_hint: Some((32, 32)),
                ..DecodeOptions::default()
            },
        )
        .unwrap();
        assert_eq!(image.width(), 32);
        assert_eq!(image.height(), 32);
        let data = image.first_frame().unwrap().texture().data();
        assert_eq!(data.len(), 32 * 32 * 4);
        // Top-left pixel is fully-opaque red even though the source
        // SVG was 4x4: vector scale, not bitmap stretch.
        assert_eq!(&data[0..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn enforces_max_dimensions() {
        let bytes = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"/>"#;
        let limits = Limits {
            max_width: 50,
            ..Limits::default()
        };
        let err = decode(
            bytes,
            &DecodeOptions {
                limits,
                apply_transformations: true,
                render_size_hint: None,
            },
        )
        .unwrap_err();
        assert!(matches!(err, Error::LimitExceeded("max_width")));
    }

    #[test]
    fn renders_text_elements() {
        let bytes = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"128\" height=\"32\"><rect width=\"128\" height=\"32\" fill=\"white\"/><text x=\"10\" y=\"24\" font-family=\"sans-serif\" font-size=\"20\" fill=\"black\">18:30</text></svg>";
        let image = decode(bytes, &opts()).unwrap();
        let data = image.first_frame().unwrap().texture().data();
        let non_white = data
            .chunks_exact(4)
            .filter(|p| p[0] < 240 || p[3] < 240)
            .count();
        assert!(non_white > 0, "text element '18:30' was not rendered");
    }
}
