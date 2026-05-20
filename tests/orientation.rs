//! End-to-end orientation handling via a synthetic EXIF-tagged PNG.

#![cfg(all(feature = "png", feature = "metadata"))]

use glycin_ng::{Loader, Orientation};

/// Build an EXIF blob (without the "Exif\0\0" prefix used by JPEG)
/// containing a single Orientation tag in little-endian TIFF format.
fn build_exif_orientation(value: u16) -> Vec<u8> {
    let mut blob = Vec::new();
    blob.extend_from_slice(b"II*\0");
    blob.extend_from_slice(&8_u32.to_le_bytes());
    blob.extend_from_slice(&1_u16.to_le_bytes()); // num entries
    blob.extend_from_slice(&0x0112_u16.to_le_bytes()); // tag
    blob.extend_from_slice(&3_u16.to_le_bytes()); // type SHORT
    blob.extend_from_slice(&1_u32.to_le_bytes()); // count
    blob.extend_from_slice(&value.to_le_bytes());
    blob.extend_from_slice(&0_u16.to_le_bytes()); // padding
    blob
}

fn encode_png_with_exif(width: u32, height: u32, exif: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.add_text_chunk("Note".into(), "test".into()).unwrap();
        let mut writer = enc.write_header().unwrap();
        writer
            .write_chunk(png::chunk::ChunkType(*b"eXIf"), exif)
            .unwrap();
        let pixels = vec![0x80; (width * height * 4) as usize];
        writer.write_image_data(&pixels).unwrap();
    }
    out
}

#[test]
fn orientation_is_applied_when_transformations_are_on() {
    let exif = build_exif_orientation(6); // rotate 90 CW
    let bytes = encode_png_with_exif(2, 3, &exif);
    let image = Loader::new_bytes(bytes)
        .apply_transformations(true)
        .load()
        .unwrap();
    // After baking, width and height are swapped and the reported
    // orientation is Normal.
    assert_eq!(image.width(), 3);
    assert_eq!(image.height(), 2);
    assert_eq!(image.orientation(), Orientation::Normal);
}

#[test]
fn orientation_is_reported_but_not_baked_when_off() {
    let exif = build_exif_orientation(6);
    let bytes = encode_png_with_exif(2, 3, &exif);
    let image = Loader::new_bytes(bytes)
        .apply_transformations(false)
        .load()
        .unwrap();
    // Pixels untouched; orientation reflects the EXIF tag.
    assert_eq!(image.width(), 2);
    assert_eq!(image.height(), 3);
    assert_eq!(image.orientation(), Orientation::Rotate90);
}

#[test]
fn every_exif_orientation_value_round_trips() {
    use glycin_ng::Orientation;
    for (raw, expected) in [
        (1, Orientation::Normal),
        (2, Orientation::FlipHorizontal),
        (3, Orientation::Rotate180),
        (4, Orientation::FlipVertical),
        (5, Orientation::Transpose),
        (6, Orientation::Rotate90),
        (7, Orientation::Transverse),
        (8, Orientation::Rotate270),
    ] {
        let exif = build_exif_orientation(raw);
        let bytes = encode_png_with_exif(2, 3, &exif);
        let image = Loader::new_bytes(bytes)
            .apply_transformations(false)
            .load()
            .unwrap();
        assert_eq!(
            image.orientation(),
            expected,
            "raw value {raw} should map to {expected:?}"
        );
    }
}

#[test]
fn baked_orientation_results_in_normal() {
    for raw in [2, 3, 4, 5, 6, 7, 8] {
        let exif = build_exif_orientation(raw);
        let bytes = encode_png_with_exif(2, 3, &exif);
        let image = Loader::new_bytes(bytes)
            .apply_transformations(true)
            .load()
            .unwrap();
        assert_eq!(
            image.orientation(),
            Orientation::Normal,
            "baked image should report Normal regardless of input ({raw})"
        );
    }
}

#[test]
fn missing_exif_keeps_normal_orientation() {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, 2, 2);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().unwrap();
        writer.write_image_data(&[0; 16]).unwrap();
    }
    let image = Loader::new_bytes(out).load().unwrap();
    assert_eq!(image.orientation(), Orientation::Normal);
}
