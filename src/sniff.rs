//! Format sniffing from leading bytes or filename extension.

/// Image formats the loader knows about.
///
/// A variant in this enum does not imply the matching decoder is
/// enabled; the decoder is only available when its Cargo feature is
/// on.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
#[non_exhaustive]
pub enum KnownFormat {
    /// Portable Network Graphics, including APNG.
    Png,
    /// JPEG.
    Jpeg,
    /// Graphics Interchange Format.
    Gif,
    /// WebP, static or animated.
    WebP,
    /// Tagged Image File Format.
    Tiff,
    /// Windows Bitmap.
    Bmp,
    /// Windows icon or cursor.
    Ico,
    /// Truevision Targa.
    Tga,
    /// Quite OK Image.
    Qoi,
    /// OpenEXR.
    Exr,
    /// Netpbm family (PBM, PGM, PPM, PAM).
    Pnm,
    /// DirectDraw surface.
    Dds,
    /// JPEG XL, codestream or container.
    Jxl,
    /// Scalable Vector Graphics.
    Svg,
}

impl KnownFormat {
    /// Short lowercase format-name string for diagnostics.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Gif => "gif",
            Self::WebP => "webp",
            Self::Tiff => "tiff",
            Self::Bmp => "bmp",
            Self::Ico => "ico",
            Self::Tga => "tga",
            Self::Qoi => "qoi",
            Self::Exr => "exr",
            Self::Pnm => "pnm",
            Self::Dds => "dds",
            Self::Jxl => "jxl",
            Self::Svg => "svg",
        }
    }

    /// Match a lowercase filename extension to a known format.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "png" | "apng" => Some(Self::Png),
            "jpg" | "jpeg" | "jpe" | "jfif" => Some(Self::Jpeg),
            "gif" => Some(Self::Gif),
            "webp" => Some(Self::WebP),
            "tif" | "tiff" => Some(Self::Tiff),
            "bmp" | "dib" => Some(Self::Bmp),
            "ico" | "cur" => Some(Self::Ico),
            "tga" | "icb" | "vda" | "vst" => Some(Self::Tga),
            "qoi" => Some(Self::Qoi),
            "exr" => Some(Self::Exr),
            "pbm" | "pgm" | "ppm" | "pnm" | "pam" => Some(Self::Pnm),
            "dds" => Some(Self::Dds),
            "jxl" => Some(Self::Jxl),
            "svg" | "svgz" => Some(Self::Svg),
            _ => None,
        }
    }

    /// Match an IANA media type (e.g. `image/png`) to a known format.
    /// Accepts the common `image/x-*` and `image/vnd.*` aliases.
    pub fn from_mime_type(mime: &str) -> Option<Self> {
        match mime.to_ascii_lowercase().as_str() {
            "image/png" | "image/apng" => Some(Self::Png),
            "image/jpeg" | "image/pjpeg" => Some(Self::Jpeg),
            "image/gif" => Some(Self::Gif),
            "image/webp" => Some(Self::WebP),
            "image/tiff" => Some(Self::Tiff),
            "image/bmp" | "image/x-bmp" => Some(Self::Bmp),
            "image/x-ico" | "image/x-icon" | "image/vnd.microsoft.icon" => Some(Self::Ico),
            "image/x-tga" | "image/x-targa" => Some(Self::Tga),
            "image/x-qoi" | "image/qoi" => Some(Self::Qoi),
            "image/x-exr" => Some(Self::Exr),
            "image/x-portable-anymap"
            | "image/x-portable-bitmap"
            | "image/x-portable-graymap"
            | "image/x-portable-pixmap" => Some(Self::Pnm),
            "image/vnd.ms-dds" | "image/vnd-ms.dds" | "image/x-dds" => Some(Self::Dds),
            "image/jxl" => Some(Self::Jxl),
            "image/svg+xml" => Some(Self::Svg),
            _ => None,
        }
    }
}

/// Detect a format from the leading bytes of an image.
///
/// Returns `None` when no known magic matches. TGA has no signature
/// and is never detected here; callers should fall back to
/// [`KnownFormat::from_extension`].
pub fn detect(buf: &[u8]) -> Option<KnownFormat> {
    if starts_with(buf, b"\x89PNG\r\n\x1a\n") {
        return Some(KnownFormat::Png);
    }
    if starts_with(buf, b"\xff\xd8\xff") {
        return Some(KnownFormat::Jpeg);
    }
    if starts_with(buf, b"GIF87a") || starts_with(buf, b"GIF89a") {
        return Some(KnownFormat::Gif);
    }
    if buf.len() >= 12 && &buf[0..4] == b"RIFF" && &buf[8..12] == b"WEBP" {
        return Some(KnownFormat::WebP);
    }
    if starts_with(buf, b"II*\0") || starts_with(buf, b"MM\0*") {
        return Some(KnownFormat::Tiff);
    }
    if starts_with(buf, b"BM") && buf.len() >= 14 {
        return Some(KnownFormat::Bmp);
    }
    if starts_with(buf, b"\0\0\x01\0") || starts_with(buf, b"\0\0\x02\0") {
        return Some(KnownFormat::Ico);
    }
    if starts_with(buf, b"qoif") {
        return Some(KnownFormat::Qoi);
    }
    if starts_with(buf, b"\x76\x2f\x31\x01") {
        return Some(KnownFormat::Exr);
    }
    if is_pnm(buf) {
        return Some(KnownFormat::Pnm);
    }
    if starts_with(buf, b"DDS ") {
        return Some(KnownFormat::Dds);
    }
    if starts_with(buf, b"\xff\x0a") || starts_with(buf, b"\0\0\0\x0cJXL \r\n\x87\n") {
        return Some(KnownFormat::Jxl);
    }
    if looks_like_svg(buf) {
        return Some(KnownFormat::Svg);
    }
    None
}

fn looks_like_svg(buf: &[u8]) -> bool {
    let probe = &buf[..buf.len().min(512)];
    let probe = probe.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(probe);
    let probe = strip_leading_whitespace(probe);
    if probe.starts_with(b"<?xml") {
        return find_svg_open_tag(probe);
    }
    starts_with_svg_open_tag(probe)
}

fn strip_leading_whitespace(b: &[u8]) -> &[u8] {
    let mut i = 0;
    while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\r' | b'\n') {
        i += 1;
    }
    &b[i..]
}

fn starts_with_svg_open_tag(b: &[u8]) -> bool {
    if !b.starts_with(b"<svg") {
        return false;
    }
    matches!(b.get(4), Some(b' ' | b'\t' | b'\r' | b'\n' | b'>' | b'/'))
}

fn find_svg_open_tag(haystack: &[u8]) -> bool {
    let needle = b"<svg";
    if haystack.len() < needle.len() {
        return false;
    }
    for i in 0..=haystack.len() - needle.len() {
        if &haystack[i..i + needle.len()] == needle && starts_with_svg_open_tag(&haystack[i..]) {
            return true;
        }
    }
    false
}

fn starts_with(buf: &[u8], prefix: &[u8]) -> bool {
    buf.len() >= prefix.len() && &buf[..prefix.len()] == prefix
}

fn is_pnm(buf: &[u8]) -> bool {
    buf.len() >= 3
        && buf[0] == b'P'
        && matches!(buf[1], b'1'..=b'7')
        && matches!(buf[2], b' ' | b'\t' | b'\r' | b'\n')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_png() {
        let bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
        assert_eq!(detect(bytes), Some(KnownFormat::Png));
    }

    #[test]
    fn detects_jpeg() {
        assert_eq!(detect(b"\xff\xd8\xff\xe0"), Some(KnownFormat::Jpeg));
        assert_eq!(detect(b"\xff\xd8\xff\xe1"), Some(KnownFormat::Jpeg));
    }

    #[test]
    fn detects_gif_both_versions() {
        assert_eq!(detect(b"GIF87a..."), Some(KnownFormat::Gif));
        assert_eq!(detect(b"GIF89a..."), Some(KnownFormat::Gif));
    }

    #[test]
    fn detects_webp() {
        let bytes = b"RIFF\0\0\0\0WEBPVP8L";
        assert_eq!(detect(bytes), Some(KnownFormat::WebP));
    }

    #[test]
    fn rejects_riff_without_webp_marker() {
        let bytes = b"RIFF\0\0\0\0WAVEfmt ";
        assert_eq!(detect(bytes), None);
    }

    #[test]
    fn detects_tiff_both_byte_orders() {
        assert_eq!(detect(b"II*\0\x08\0\0\0"), Some(KnownFormat::Tiff));
        assert_eq!(detect(b"MM\0*\0\0\0\x08"), Some(KnownFormat::Tiff));
    }

    #[test]
    fn detects_bmp() {
        let bytes = b"BM\0\0\0\0\0\0\0\0\0\0\0\0";
        assert_eq!(detect(bytes), Some(KnownFormat::Bmp));
    }

    #[test]
    fn detects_ico_and_cur() {
        assert_eq!(detect(b"\0\0\x01\0..."), Some(KnownFormat::Ico));
        assert_eq!(detect(b"\0\0\x02\0..."), Some(KnownFormat::Ico));
    }

    #[test]
    fn detects_qoi() {
        assert_eq!(detect(b"qoif..."), Some(KnownFormat::Qoi));
    }

    #[test]
    fn detects_exr() {
        assert_eq!(detect(b"\x76\x2f\x31\x01..."), Some(KnownFormat::Exr));
    }

    #[test]
    fn detects_pnm_family() {
        for kind in [
            b"P1\n", b"P2\n", b"P3\n", b"P4\n", b"P5\n", b"P6\n", b"P7\n",
        ] {
            assert_eq!(detect(kind), Some(KnownFormat::Pnm));
        }
        assert_eq!(detect(b"P1 "), Some(KnownFormat::Pnm));
        assert_eq!(detect(b"P0 "), None);
        assert_eq!(detect(b"P8 "), None);
    }

    #[test]
    fn detects_dds() {
        assert_eq!(detect(b"DDS \0\0\0\0..."), Some(KnownFormat::Dds));
    }

    #[test]
    fn dds_mime_aliases_resolve() {
        for mime in ["image/vnd.ms-dds", "image/vnd-ms.dds", "image/x-dds"] {
            assert_eq!(
                KnownFormat::from_mime_type(mime),
                Some(KnownFormat::Dds),
                "{mime} should map to Dds"
            );
        }
    }

    #[test]
    fn detects_jxl_codestream_and_container() {
        assert_eq!(detect(b"\xff\x0a..."), Some(KnownFormat::Jxl));
        let container = b"\0\0\0\x0cJXL \r\n\x87\n";
        assert_eq!(detect(container), Some(KnownFormat::Jxl));
    }

    #[test]
    fn detects_svg() {
        assert_eq!(detect(b"<svg/>"), Some(KnownFormat::Svg));
        assert_eq!(detect(b"<svg "), Some(KnownFormat::Svg));
        assert_eq!(detect(b"  \n<svg width=\"10\"/>"), Some(KnownFormat::Svg));
        assert_eq!(
            detect(b"<?xml version=\"1.0\"?><svg/>"),
            Some(KnownFormat::Svg)
        );
        let mut bom = vec![0xEF, 0xBB, 0xBF];
        bom.extend_from_slice(b"<svg/>");
        assert_eq!(detect(&bom), Some(KnownFormat::Svg));
    }

    #[test]
    fn rejects_non_svg_xml() {
        assert_eq!(detect(b"<?xml version=\"1.0\"?><html/>"), None);
        assert_eq!(detect(b"<svgno"), None);
    }

    #[test]
    fn unknown_bytes_return_none() {
        assert_eq!(detect(b""), None);
        assert_eq!(detect(b"hello"), None);
        assert_eq!(detect(b"\0\0\0\0"), None);
    }

    #[test]
    fn tga_is_never_sniffed() {
        let plausible_tga = b"\0\0\x02\0\0\0\0\0\0\0\0\0";
        assert_ne!(detect(plausible_tga), Some(KnownFormat::Tga));
    }

    #[test]
    fn extension_maps_format() {
        assert_eq!(KnownFormat::from_extension("png"), Some(KnownFormat::Png));
        assert_eq!(KnownFormat::from_extension("PNG"), Some(KnownFormat::Png));
        assert_eq!(KnownFormat::from_extension("jpg"), Some(KnownFormat::Jpeg));
        assert_eq!(KnownFormat::from_extension("tga"), Some(KnownFormat::Tga));
        assert_eq!(KnownFormat::from_extension("xyz"), None);
        assert_eq!(KnownFormat::from_extension(""), None);
    }

    #[test]
    fn format_name_strings_are_lowercase() {
        for f in [
            KnownFormat::Png,
            KnownFormat::Jpeg,
            KnownFormat::Gif,
            KnownFormat::WebP,
            KnownFormat::Tiff,
            KnownFormat::Bmp,
            KnownFormat::Ico,
            KnownFormat::Tga,
            KnownFormat::Qoi,
            KnownFormat::Exr,
            KnownFormat::Pnm,
            KnownFormat::Dds,
            KnownFormat::Jxl,
        ] {
            let n = f.name();
            assert!(!n.is_empty());
            assert!(n.chars().all(|c| c.is_ascii_lowercase()));
        }
    }
}
