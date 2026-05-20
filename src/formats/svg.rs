//! SVG decoder via `resvg` and `usvg`.

use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{Options, Tree};

use crate::{Error, Frame, Image, MemoryFormat, Result, Texture};

use super::DecodeOptions;

pub(crate) fn decode(bytes: &[u8], opts: &DecodeOptions) -> Result<Image> {
    let parse_opt = Options::default();
    let tree = Tree::from_data(bytes, &parse_opt).map_err(|e| Error::Malformed(e.to_string()))?;
    let svg_size = tree.size();
    let width = svg_size.width().ceil().max(1.0) as u32;
    let height = svg_size.height().ceil().max(1.0) as u32;
    opts.limits.check_dimensions(width, height, 1)?;

    let mut pixmap = Pixmap::new(width, height).ok_or_else(|| Error::Decoder {
        format: "svg",
        message: format!("failed to allocate {width}x{height} pixmap"),
    })?;
    let transform = Transform::identity();
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let stride = width.checked_mul(4).ok_or(Error::LimitExceeded("stride"))?;
    let texture = Texture::from_parts(
        width,
        height,
        stride,
        MemoryFormat::R8g8b8a8Premultiplied,
        pixmap.data().to_vec().into_boxed_slice(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Limits;

    fn opts() -> DecodeOptions {
        DecodeOptions {
            limits: Limits::default(),
            apply_transformations: true,
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
        assert_eq!(
            frame.texture().format(),
            MemoryFormat::R8g8b8a8Premultiplied
        );
        assert_eq!(frame.texture().data().len(), 4 * 4 * 4);
        // First pixel should be red (255, 0, 0, 255) premultiplied -> (255, 0, 0, 255).
        let data = frame.texture().data();
        assert_eq!(&data[0..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn rejects_garbage() {
        let err = decode(b"<svg garbage>not really svg", &opts()).unwrap_err();
        assert!(matches!(err, Error::Malformed(_) | Error::Decoder { .. }));
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
            },
        )
        .unwrap_err();
        assert!(matches!(err, Error::LimitExceeded("max_width")));
    }
}
