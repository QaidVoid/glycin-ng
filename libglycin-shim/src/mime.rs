//! Canonical IANA media types for the formats glycin-ng knows about.
//!
//! `glycin_ng_image_format_name` returns a short lowercase tag
//! (`"png"`, `"svg"`, ...). Upstream's `gly_image_get_mime_type`
//! reports the detected MIME type, so we map the tag back to the
//! canonical type. The chosen strings round-trip through
//! `KnownFormat::from_mime_type` on the engine side.

/// Map a `glycin_ng_image_format_name` tag to its canonical MIME
/// type. Returns `None` for tags we do not recognize.
pub(crate) fn from_format_name(name: &str) -> Option<&'static str> {
    Some(match name {
        "png" => "image/png",
        "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "tiff" => "image/tiff",
        "bmp" => "image/bmp",
        "ico" => "image/vnd.microsoft.icon",
        "tga" => "image/x-tga",
        "qoi" => "image/qoi",
        "exr" => "image/x-exr",
        "pnm" => "image/x-portable-anymap",
        "dds" => "image/x-dds",
        "jxl" => "image/jxl",
        "svg" => "image/svg+xml",
        _ => return None,
    })
}

/// Every MIME type the loader accepts, grouped by format. Used by
/// `gly_loader_get_mime_types`. This mirrors the alias set the
/// engine's `KnownFormat::from_mime_type` recognizes (see
/// `src/sniff.rs`), so the advertised list matches what the default
/// build actually decodes. A decoder being listed here does not
/// guarantee its Cargo feature is enabled in `libglycin_ng.so`,
/// matching upstream's behavior of advertising the format surface
/// rather than the build-time feature set.
pub(crate) const SUPPORTED_MIME_TYPES: &[&str] = &[
    "image/png",
    "image/apng",
    "image/jpeg",
    "image/pjpeg",
    "image/gif",
    "image/webp",
    "image/tiff",
    "image/bmp",
    "image/x-bmp",
    "image/x-ico",
    "image/x-icon",
    "image/vnd.microsoft.icon",
    "image/x-tga",
    "image/x-targa",
    "image/x-qoi",
    "image/qoi",
    "image/x-exr",
    "image/x-portable-anymap",
    "image/x-portable-bitmap",
    "image/x-portable-graymap",
    "image/x-portable-pixmap",
    "image/vnd-ms.dds",
    "image/x-dds",
    "image/jxl",
    "image/svg+xml",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_format_names_map_to_mime() {
        assert_eq!(from_format_name("png"), Some("image/png"));
        assert_eq!(from_format_name("jpeg"), Some("image/jpeg"));
        assert_eq!(from_format_name("svg"), Some("image/svg+xml"));
        assert_eq!(from_format_name("jxl"), Some("image/jxl"));
        assert_eq!(from_format_name("nonsense"), None);
    }

    #[test]
    fn every_format_name_mime_is_in_supported_list() {
        for name in [
            "png", "jpeg", "gif", "webp", "tiff", "bmp", "ico", "tga", "qoi", "exr", "pnm", "dds",
            "jxl", "svg",
        ] {
            let mime = from_format_name(name).expect("mapped");
            assert!(
                SUPPORTED_MIME_TYPES.contains(&mime),
                "{mime} missing from SUPPORTED_MIME_TYPES"
            );
        }
    }

    #[test]
    fn supported_list_has_no_duplicates() {
        let mut seen = SUPPORTED_MIME_TYPES.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), SUPPORTED_MIME_TYPES.len());
    }
}
