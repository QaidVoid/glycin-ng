//! End-to-end tests for metadata extraction: load PNGs carrying a
//! `tEXt` chunk and a `cICP` chunk through the public `Loader` API and
//! verify the key/value map and CICP code points.

#![cfg(all(feature = "png", feature = "metadata"))]

use glycin_ng::Loader;

const TEXT_METADATA_PNG: &[u8] = include_bytes!("data/text-metadata.png");
const CICP_PNG: &[u8] = include_bytes!("data/cicp.png");

#[test]
fn text_chunk_becomes_key_value() {
    let image = Loader::new_bytes(TEXT_METADATA_PNG.to_vec()).load().unwrap();
    let map = image
        .metadata_key_value()
        .expect("png tEXt chunk should yield key/value metadata");
    assert_eq!(map.get("Comment").map(String::as_str), Some("hello"));
}

#[test]
fn image_without_text_has_no_key_value() {
    let image = Loader::new_bytes(CICP_PNG.to_vec()).load().unwrap();
    assert!(image.metadata_key_value().is_none());
}

#[test]
fn cicp_chunk_is_extracted() {
    let image = Loader::new_bytes(CICP_PNG.to_vec()).load().unwrap();
    assert_eq!(image.cicp(), Some([9, 16, 0, 1]));
}

#[test]
fn image_without_cicp_reports_none() {
    let image = Loader::new_bytes(TEXT_METADATA_PNG.to_vec()).load().unwrap();
    assert!(image.cicp().is_none());
}
