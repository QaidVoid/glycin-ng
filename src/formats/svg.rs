//! SVG decoder via `resvg` and `usvg`.
//!
//! Besides the one-shot [`decode`] entry point used by the loader,
//! this module exposes the parse/render/geometry helpers behind the
//! persistent SVG document C API (`glycin_ng_svg_*`), which the
//! librsvg compatibility shim builds on.

mod xinclude;

use std::path::PathBuf;
use std::sync::{Arc, LazyLock};

use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{Options, Tree, fontdb};

use crate::{Error, Frame, Image, Limits, MemoryFormat, Result, Texture};

use super::DecodeOptions;

/// Parse-time options for [`parse_tree`].
#[derive(Clone, Debug, Default)]
pub(crate) struct SvgOptions {
    /// Extra user-origin CSS injected into the document.
    pub stylesheet: Option<String>,
    /// Dots per inch for physical-unit conversion. Values `<= 0`
    /// fall back to the usvg default of 96.
    pub dpi: f32,
    /// Directory for resolving relative external references
    /// (`<image href="...">`). `None` keeps external file loading
    /// disabled.
    pub resources_dir: Option<PathBuf>,
    /// Use the system font database instead of the bundled single
    /// fallback font.
    pub system_fonts: bool,
}

/// Parse `bytes` into a usvg tree, running the XInclude pass first.
pub(crate) fn parse_tree(bytes: &[u8], opts: &SvgOptions) -> Result<Tree> {
    let parse_opt = Options {
        fontdb: if opts.system_fonts {
            system_fontdb()
        } else {
            BUNDLED_FONTDB.clone()
        },
        dpi: if opts.dpi > 0.0 { opts.dpi } else { 96.0 },
        style_sheet: opts.stylesheet.clone(),
        resources_dir: opts.resources_dir.clone(),
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
    Tree::from_data(svg_bytes, &parse_opt).map_err(|e| Error::Malformed(e.to_string()))
}

pub(crate) fn decode(bytes: &[u8], opts: &DecodeOptions) -> Result<Image> {
    let tree = parse_tree(bytes, &SvgOptions::default())?;
    let svg_size = tree.size();
    let intrinsic_w = svg_size.width().ceil().max(1.0) as u32;
    let intrinsic_h = svg_size.height().ceil().max(1.0) as u32;
    let (width, height) = opts.render_size_hint.unwrap_or((intrinsic_w, intrinsic_h));
    let width = width.max(1);
    let height = height.max(1);

    let sx = width as f64 / intrinsic_w as f64;
    let sy = height as f64 / intrinsic_h as f64;
    let rgba = render_tree(
        &tree,
        SvgTarget::Document,
        width,
        height,
        [sx, 0.0, 0.0, sy, 0.0, 0.0],
        true,
        &opts.limits,
    )?;

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

/// What part of the document a render or geometry query targets.
///
/// Element IDs are bare (no leading `#`).
#[derive(Clone, Copy, Debug)]
pub(crate) enum SvgTarget<'a> {
    /// The whole document.
    Document,
    /// A single element rendered in its in-document position
    /// (librsvg "layer" semantics).
    Layer(&'a str),
    /// A single element extracted by itself, normalized to the
    /// origin (librsvg "element" semantics).
    Element(&'a str),
}

/// Render `tree` into a `width`x`height` RGBA8 buffer under a 2x3
/// affine `transform` given in cairo order `[xx, yx, xy, yy, x0, y0]`.
///
/// With `unpremultiply` the buffer is converted to straight alpha
/// (what `GdkPixbuf` expects); otherwise it stays premultiplied
/// (what cairo `ARGB32` expects, modulo channel order).
pub(crate) fn render_tree(
    tree: &Tree,
    target: SvgTarget<'_>,
    width: u32,
    height: u32,
    transform: [f64; 6],
    unpremultiply: bool,
    limits: &Limits,
) -> Result<Vec<u8>> {
    limits.check_dimensions(width.max(1), height.max(1), 1)?;
    if width == 0 || height == 0 {
        return Err(Error::Decoder {
            format: "svg",
            message: "zero-sized render target".into(),
        });
    }
    let mut pixmap = Pixmap::new(width, height).ok_or_else(|| Error::Decoder {
        format: "svg",
        message: format!("failed to allocate {width}x{height} pixmap"),
    })?;
    let ts = Transform::from_row(
        transform[0] as f32,
        transform[1] as f32,
        transform[2] as f32,
        transform[3] as f32,
        transform[4] as f32,
        transform[5] as f32,
    );

    match target {
        SvgTarget::Document => resvg::render(tree, ts, &mut pixmap.as_mut()),
        SvgTarget::Layer(id) => {
            let node = node_by_id(tree, id)?;
            // `render_node` normalizes to the node's layer bounding
            // box origin; pre-translate by that origin so the node
            // lands at its in-document position instead.
            if let Some(bbox) = node.abs_layer_bounding_box() {
                let ts = ts.pre_translate(bbox.x(), bbox.y());
                resvg::render_node(node, ts, &mut pixmap.as_mut());
            }
        }
        SvgTarget::Element(id) => {
            let node = node_by_id(tree, id)?;
            resvg::render_node(node, ts, &mut pixmap.as_mut());
        }
    }

    if unpremultiply {
        // tiny_skia renders into a premultiplied buffer; see decode()
        // for why straight alpha is the interchange default.
        let mut rgba = Vec::with_capacity(pixmap.data().len());
        for px in pixmap.pixels() {
            let c = px.demultiply();
            rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
        }
        Ok(rgba)
    } else {
        Ok(pixmap.take())
    }
}

fn node_by_id<'a>(tree: &'a Tree, id: &str) -> Result<&'a resvg::usvg::Node> {
    tree.node_by_id(id)
        .ok_or_else(|| Error::Malformed(format!("no element with id \"{id}\"")))
}

/// Whether the document contains an element with the given bare id.
pub(crate) fn has_element(tree: &Tree, id: &str) -> bool {
    tree.node_by_id(id).is_some()
}

/// Ink (painted, stroke-inclusive) and logical (fill-only) bounding
/// rectangles as `[x, y, w, h]` in document coordinates.
///
/// `id: None` measures the whole document from the root. With
/// `element_mode` both rectangles are translated so the ink rect
/// starts at the origin, matching librsvg's
/// `rsvg_handle_get_geometry_for_element`.
pub(crate) fn geometry(
    tree: &Tree,
    id: Option<&str>,
    element_mode: bool,
) -> Result<([f64; 4], [f64; 4])> {
    let (ink, logical) = match id {
        None => {
            let root = tree.root();
            (
                rect(root.abs_stroke_bounding_box()),
                rect(root.abs_bounding_box()),
            )
        }
        Some(id) => {
            let node = node_by_id(tree, id)?;
            (
                rect(node.abs_stroke_bounding_box()),
                rect(node.abs_bounding_box()),
            )
        }
    };
    if element_mode {
        let (dx, dy) = (ink[0], ink[1]);
        Ok((
            [0.0, 0.0, ink[2], ink[3]],
            [logical[0] - dx, logical[1] - dy, logical[2], logical[3]],
        ))
    } else {
        Ok((ink, logical))
    }
}

fn rect(r: resvg::usvg::Rect) -> [f64; 4] {
    [
        r.x() as f64,
        r.y() as f64,
        r.width() as f64,
        r.height() as f64,
    ]
}

/// CSS length unit of a root `width`/`height` attribute. The
/// discriminants match librsvg's `RsvgUnit` so the C API can pass
/// them through unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub(crate) enum SvgUnit {
    /// Percentage, where `1.0` means 100%.
    Percent = 0,
    /// Pixels (also bare numbers).
    Px = 1,
    /// Current font size.
    Em = 2,
    /// x-height of the current font.
    Ex = 3,
    /// Inches.
    In = 4,
    /// Centimeters.
    Cm = 5,
    /// Millimeters.
    Mm = 6,
    /// Points (1/72 inch).
    Pt = 7,
    /// Picas (1/6 inch).
    Pc = 8,
    /// Advance measure of a `0` character.
    Ch = 9,
}

/// A CSS length: numeric value plus unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SvgLength {
    /// Numeric part.
    pub value: f64,
    /// Unit part.
    pub unit: SvgUnit,
}

/// Raw `width`/`height`/`viewBox` attributes of the root `<svg>`
/// element, before usvg resolves them to pixels.
#[derive(Clone, Copy, Debug)]
pub(crate) struct IntrinsicDimensions {
    /// Root `width`, defaulting to `100%` when absent (SVG2).
    pub width: SvgLength,
    /// Root `height`, defaulting to `100%` when absent (SVG2).
    pub height: SvgLength,
    /// Root `viewBox` as `[x, y, w, h]`, if present.
    pub viewbox: Option<[f64; 4]>,
}

/// Scan the root `<svg>` start tag for its sizing attributes.
///
/// usvg resolves the root sizing to pixels during parsing; librsvg's
/// `get_intrinsic_dimensions` needs the pre-resolution values with
/// units, so they are read from the raw XML here.
pub(crate) fn intrinsic_dimensions(bytes: &[u8]) -> IntrinsicDimensions {
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;

    let default = SvgLength {
        value: 1.0,
        unit: SvgUnit::Percent,
    };
    let mut dims = IntrinsicDimensions {
        width: default,
        height: default,
        viewbox: None,
    };

    let mut reader = Reader::from_reader(bytes);
    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                if e.local_name().as_ref() == b"svg" {
                    for attr in e.attributes().with_checks(false).flatten() {
                        let value = attr.value.as_ref();
                        match attr.key.as_ref() {
                            b"width" => {
                                if let Some(l) = parse_length(value) {
                                    dims.width = l;
                                }
                            }
                            b"height" => {
                                if let Some(l) = parse_length(value) {
                                    dims.height = l;
                                }
                            }
                            b"viewBox" => dims.viewbox = parse_viewbox(value),
                            _ => {}
                        }
                    }
                }
                // Only the document element carries the intrinsic
                // sizing, whether or not it turned out to be <svg>.
                break;
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    dims
}

fn parse_length(raw: &[u8]) -> Option<SvgLength> {
    let s = std::str::from_utf8(raw).ok()?.trim();
    const UNITS: &[(&str, SvgUnit)] = &[
        ("%", SvgUnit::Percent),
        ("px", SvgUnit::Px),
        ("em", SvgUnit::Em),
        ("ex", SvgUnit::Ex),
        ("in", SvgUnit::In),
        ("cm", SvgUnit::Cm),
        ("mm", SvgUnit::Mm),
        ("pt", SvgUnit::Pt),
        ("pc", SvgUnit::Pc),
        ("ch", SvgUnit::Ch),
    ];
    for (suffix, unit) in UNITS {
        if let Some(num) = s.strip_suffix(suffix) {
            let mut value: f64 = num.trim_end().parse().ok()?;
            if *unit == SvgUnit::Percent {
                value /= 100.0;
            }
            return Some(SvgLength { value, unit: *unit });
        }
    }
    let value: f64 = s.parse().ok()?;
    Some(SvgLength {
        value,
        unit: SvgUnit::Px,
    })
}

fn parse_viewbox(raw: &[u8]) -> Option<[f64; 4]> {
    let s = std::str::from_utf8(raw).ok()?;
    let mut parts = s
        .split(|c: char| c.is_ascii_whitespace() || c == ',')
        .filter(|p| !p.is_empty());
    let mut vb = [0.0; 4];
    for slot in &mut vb {
        *slot = parts.next()?.parse().ok()?;
    }
    parts.next().is_none().then_some(vb)
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
    set_generic_families(&mut db);
    Arc::new(db)
});

fn set_generic_families(db: &mut fontdb::Database) {
    db.set_serif_family("Cantarell");
    db.set_sans_serif_family("Cantarell");
    db.set_monospace_family("Cantarell");
    db.set_cursive_family("Cantarell");
    db.set_fantasy_family("Cantarell");
}

/// System font database plus the bundled fallback face, shared
/// process-wide.
///
/// Every file-backed face is converted to a shared memory map up
/// front: parsing may later run on a landlocked worker thread that
/// cannot open font files, and established mappings keep working
/// there. Generic families stay mapped to the bundled Cantarell so
/// output is deterministic; explicitly named families resolve
/// against the system set.
pub(crate) fn system_fontdb() -> Arc<fontdb::Database> {
    static DB: LazyLock<Arc<fontdb::Database>> = LazyLock::new(|| {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        db.load_font_data(include_bytes!("../../assets/Cantarell-Regular.ttf").to_vec());
        set_generic_families(&mut db);
        let ids: Vec<fontdb::ID> = db.faces().map(|f| f.id).collect();
        for id in ids {
            // SAFETY: maps the face's font file read-only; the usual
            // mmap caveat applies (the file must not be truncated
            // while mapped), the same trade every font stack makes.
            let _ = unsafe { db.make_shared_face_data(id) };
        }
        Arc::new(db)
    });
    DB.clone()
}

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

    const TWO_RECTS: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10">
  <rect id="left" x="0" y="0" width="10" height="10" fill="red"/>
  <rect id="right" x="10" y="0" width="10" height="10" fill="blue"/>
</svg>"#;

    #[test]
    fn stylesheet_overrides_fill() {
        let tree = parse_tree(
            TWO_RECTS,
            &SvgOptions {
                stylesheet: Some("rect { fill: #00ff00 !important; }".into()),
                ..SvgOptions::default()
            },
        )
        .unwrap();
        let data = render_tree(
            &tree,
            SvgTarget::Document,
            20,
            10,
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            true,
            &Limits::default(),
        )
        .unwrap();
        assert_eq!(&data[0..4], &[0, 255, 0, 255]);
    }

    #[test]
    fn layer_render_keeps_document_position() {
        let tree = parse_tree(TWO_RECTS, &SvgOptions::default()).unwrap();
        let data = render_tree(
            &tree,
            SvgTarget::Layer("right"),
            20,
            10,
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            true,
            &Limits::default(),
        )
        .unwrap();
        // Left half untouched (transparent), right half blue.
        assert_eq!(&data[0..4], &[0, 0, 0, 0]);
        let px = |x: usize, y: usize| &data[(y * 20 + x) * 4..(y * 20 + x) * 4 + 4];
        assert_eq!(px(15, 5), &[0, 0, 255, 255]);
    }

    #[test]
    fn element_render_normalizes_to_origin() {
        let tree = parse_tree(TWO_RECTS, &SvgOptions::default()).unwrap();
        let data = render_tree(
            &tree,
            SvgTarget::Element("right"),
            10,
            10,
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            true,
            &Limits::default(),
        )
        .unwrap();
        // The blue rect fills the pixmap from the origin.
        assert_eq!(&data[0..4], &[0, 0, 255, 255]);
    }

    #[test]
    fn geometry_layer_and_element_modes() {
        let tree = parse_tree(TWO_RECTS, &SvgOptions::default()).unwrap();
        let (ink, logical) = geometry(&tree, Some("right"), false).unwrap();
        assert_eq!(ink, [10.0, 0.0, 10.0, 10.0]);
        assert_eq!(logical, [10.0, 0.0, 10.0, 10.0]);
        let (ink, _logical) = geometry(&tree, Some("right"), true).unwrap();
        assert_eq!(ink, [0.0, 0.0, 10.0, 10.0]);
        assert!(geometry(&tree, Some("missing"), false).is_err());
        assert!(has_element(&tree, "left"));
        assert!(!has_element(&tree, "missing"));
    }

    #[test]
    fn intrinsic_dimensions_units_and_viewbox() {
        let d = intrinsic_dimensions(
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="210mm" height="297mm" viewBox="0 0 210 297"/>"#,
        );
        assert_eq!(
            d.width,
            SvgLength {
                value: 210.0,
                unit: SvgUnit::Mm
            }
        );
        assert_eq!(
            d.height,
            SvgLength {
                value: 297.0,
                unit: SvgUnit::Mm
            }
        );
        assert_eq!(d.viewbox, Some([0.0, 0.0, 210.0, 297.0]));

        let d = intrinsic_dimensions(br#"<svg viewBox="0 0 100 200"/>"#);
        assert_eq!(
            d.width,
            SvgLength {
                value: 1.0,
                unit: SvgUnit::Percent
            }
        );
        assert_eq!(
            d.height,
            SvgLength {
                value: 1.0,
                unit: SvgUnit::Percent
            }
        );
        assert_eq!(d.viewbox, Some([0.0, 0.0, 100.0, 200.0]));

        let d = intrinsic_dimensions(br#"<svg width="50%" height="4in"/>"#);
        assert_eq!(
            d.width,
            SvgLength {
                value: 0.5,
                unit: SvgUnit::Percent
            }
        );
        assert_eq!(
            d.height,
            SvgLength {
                value: 4.0,
                unit: SvgUnit::In
            }
        );

        let d = intrinsic_dimensions(br#"<svg width="64" height="32"/>"#);
        assert_eq!(
            d.width,
            SvgLength {
                value: 64.0,
                unit: SvgUnit::Px
            }
        );
        assert_eq!(
            d.height,
            SvgLength {
                value: 32.0,
                unit: SvgUnit::Px
            }
        );
        assert_eq!(d.viewbox, None);
    }
}
