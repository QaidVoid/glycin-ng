//! End-to-end decode round-trips for every wired format.

use glycin_ng::Loader;
#[cfg(any(feature = "png", feature = "gif"))]
use glycin_ng::MemoryFormat;

#[test]
#[cfg(feature = "png")]
fn end_to_end_png() {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, 4, 4);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&[0; 64]).unwrap();
    }
    let image = Loader::new_bytes(out).load().unwrap();
    assert_eq!(image.width(), 4);
    assert_eq!(image.format_name(), "png");
    assert_eq!(
        image.first_frame().unwrap().texture().format(),
        MemoryFormat::R8g8b8a8
    );
}

#[test]
#[cfg(feature = "qoi")]
fn end_to_end_qoi() {
    let pixels = vec![0x40u8; 4 * 4 * 4];
    let encoded = qoi::encode_to_vec(&pixels, 4, 4).unwrap();
    let image = Loader::new_bytes(encoded).load().unwrap();
    assert_eq!(image.width(), 4);
    assert_eq!(image.format_name(), "qoi");
}

#[test]
#[cfg(feature = "gif")]
fn end_to_end_gif() {
    let mut out = Vec::new();
    let palette: &[u8] = &[0, 0, 0, 255, 255, 255];
    let frame = gif::Frame {
        width: 2,
        height: 2,
        buffer: std::borrow::Cow::Owned(vec![0, 1, 1, 0]),
        ..gif::Frame::default()
    };
    let mut encoder = gif::Encoder::new(&mut out, 2, 2, palette).unwrap();
    encoder.write_frame(&frame).unwrap();
    drop(encoder);
    let image = Loader::new_bytes(out).load().unwrap();
    assert_eq!(image.width(), 2);
    assert_eq!(image.format_name(), "gif");
    assert_eq!(
        image.first_frame().unwrap().texture().format(),
        MemoryFormat::R8g8b8a8
    );
}

#[test]
#[cfg(feature = "pnm")]
fn end_to_end_pnm_ppm() {
    let mut bytes = b"P6\n3 2\n255\n".to_vec();
    bytes.extend_from_slice(&[
        255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0, 0, 255, 255, 255, 0, 255,
    ]);
    let image = Loader::new_bytes(bytes).load().unwrap();
    assert_eq!(image.width(), 3);
    assert_eq!(image.height(), 2);
    assert_eq!(image.format_name(), "pnm");
}

#[test]
fn unknown_bytes_return_unsupported_format() {
    let err = Loader::new_bytes(b"This is plainly not an image".to_vec())
        .load()
        .unwrap_err();
    assert!(matches!(err, glycin_ng::Error::UnsupportedFormat));
}

#[test]
#[cfg(feature = "png")]
fn loader_decodes_png_with_strict_sandbox_on_linux() {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, 2, 2);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&[0; 16]).unwrap();
    }
    let result = Loader::new_bytes(out).require_sandbox().load();
    if cfg!(target_os = "linux") {
        // On Linux 5.13+ with seccomp support this must succeed.
        result.unwrap();
    } else {
        let err = result.unwrap_err();
        assert!(matches!(err, glycin_ng::Error::SandboxUnavailable(_)));
    }
}
