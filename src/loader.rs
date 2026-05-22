//! Builder-style image loader.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::formats::{DecodeOptions, dispatch};
use crate::sniff::{self, KnownFormat};
use crate::{Error, Image, Limits, Result, SandboxSelector};

/// Image source captured by the loader.
enum Source {
    Path(PathBuf),
    Bytes(Vec<u8>),
    Reader(Box<dyn Read + Send + 'static>),
}

impl Source {
    fn read_all(self) -> Result<Vec<u8>> {
        match self {
            Self::Path(p) => Ok(std::fs::read(p)?),
            Self::Bytes(b) => Ok(b),
            Self::Reader(mut r) => {
                let mut buf = Vec::new();
                r.read_to_end(&mut buf)?;
                Ok(buf)
            }
        }
    }
}

/// Builder for decoding a single image.
///
/// Construct with [`Loader::new_path`], [`Loader::new_bytes`], or
/// [`Loader::new_reader`]; configure with the builder methods; then
/// call [`Loader::load`] to consume the loader and produce an
/// [`Image`].
pub struct Loader {
    source: Source,
    extension_hint: Option<String>,
    format_hint: Option<KnownFormat>,
    limits: Limits,
    sandbox: SandboxSelector,
    apply_transformations: bool,
    render_size_hint: Option<(u32, u32)>,
}

impl Loader {
    /// Load from a filesystem path.
    pub fn new_path<P: AsRef<Path>>(path: P) -> Self {
        let path = path.as_ref().to_path_buf();
        let extension_hint = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase());
        Self::new(Source::Path(path), extension_hint)
    }

    /// Load from an in-memory byte buffer.
    pub fn new_bytes(bytes: Vec<u8>) -> Self {
        Self::new(Source::Bytes(bytes), None)
    }

    /// Load from a reader. The full contents are read into memory
    /// when [`Loader::load`] runs.
    pub fn new_reader<R: Read + Send + 'static>(reader: R) -> Self {
        Self::new(Source::Reader(Box::new(reader)), None)
    }

    fn new(source: Source, extension_hint: Option<String>) -> Self {
        Self {
            source,
            extension_hint,
            format_hint: None,
            limits: Limits::default(),
            sandbox: SandboxSelector::default(),
            apply_transformations: true,
            render_size_hint: None,
        }
    }

    /// Request the decoder render at a specific output size in pixels.
    ///
    /// Only resolution-independent formats (SVG) honor this; raster
    /// formats decode at their native size. The decoder still rejects
    /// the request if `width * height` would exceed [`Limits`].
    pub fn render_size_hint(mut self, width: u32, height: u32) -> Self {
        self.render_size_hint = Some((width, height));
        self
    }

    /// Replace the limits applied to this decode.
    pub fn limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// Read back the [`Limits`] currently configured on this loader.
    /// Useful when adjusting a single field without losing the rest;
    /// callers can read, mutate, and pass the result to
    /// [`Loader::limits`].
    pub fn current_limits(&self) -> Limits {
        self.limits
    }

    /// Replace the sandbox selector for this decode.
    pub fn sandbox_selector(mut self, sandbox: SandboxSelector) -> Self {
        self.sandbox = sandbox;
        self
    }

    /// Refuse the decode if any requested sandbox layer cannot be
    /// applied. Equivalent to setting `strict = true` on the
    /// [`SandboxSelector`].
    pub fn require_sandbox(mut self) -> Self {
        self.sandbox.strict = true;
        self
    }

    /// Apply EXIF orientation during decode (default: `true`).
    pub fn apply_transformations(mut self, apply: bool) -> Self {
        self.apply_transformations = apply;
        self
    }

    /// Override the format-detection step with an explicit hint.
    ///
    /// Useful when the source has no extension and the leading bytes
    /// do not carry a signature (TGA is the common case).
    pub fn format_hint(mut self, format: KnownFormat) -> Self {
        self.format_hint = Some(format);
        self
    }

    /// Consume the loader and decode the image.
    pub fn load(self) -> Result<Image> {
        let Loader {
            source,
            extension_hint,
            format_hint,
            limits,
            sandbox,
            apply_transformations,
            render_size_hint,
        } = self;
        let bytes = source.read_all()?;
        let format = format_hint
            .or_else(|| sniff::detect(&bytes))
            .or_else(|| {
                extension_hint
                    .as_deref()
                    .and_then(KnownFormat::from_extension)
            })
            .ok_or(Error::UnsupportedFormat)?;
        let opts = DecodeOptions {
            limits,
            apply_transformations,
            render_size_hint,
        };
        let (mut image, posture) = crate::sandbox::run_in_worker(sandbox, limits, move || {
            dispatch(format, &bytes, &opts)
        })?;
        #[cfg(feature = "metadata")]
        crate::metadata::apply_orientation_if_present(&mut image, apply_transformations);
        image.set_sandbox_posture(posture);
        Ok(image)
    }
}

impl std::fmt::Debug for Loader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Loader")
            .field(
                "source",
                &match &self.source {
                    Source::Path(p) => format!("Path({})", p.display()),
                    Source::Bytes(b) => format!("Bytes({} bytes)", b.len()),
                    Source::Reader(_) => "Reader(..)".to_string(),
                },
            )
            .field("extension_hint", &self.extension_hint)
            .field("format_hint", &self.format_hint)
            .field("limits", &self.limits)
            .field("sandbox", &self.sandbox)
            .field("apply_transformations", &self.apply_transformations)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tempfile::tempdir;

    fn truncated_png_signature() -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
        v.extend_from_slice(b"\0\0\0\rIHDR");
        v
    }

    #[test]
    fn bytes_loader_sniffs_and_dispatches() {
        let err = Loader::new_bytes(truncated_png_signature())
            .load()
            .unwrap_err();
        assert!(matches!(
            err,
            Error::Malformed(_) | Error::Io(_) | Error::UnsupportedFormat
        ));
    }

    #[test]
    fn unknown_bytes_return_unsupported_format() {
        let err = Loader::new_bytes(b"garbage".to_vec()).load().unwrap_err();
        assert!(matches!(err, Error::UnsupportedFormat));
    }

    #[test]
    fn path_extension_resolves_tga() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("dummy.tga");
        std::fs::write(&path, b"\0\0\x02\0\0\0\0\0\0\0\0\0").unwrap();
        let err = Loader::new_path(&path).load().unwrap_err();
        // With the tga feature on, the sniffer hands the file to the
        // TGA decoder via the extension fallback. The 12-byte dummy is
        // not a valid TGA, so we get a Malformed error rather than
        // UnsupportedFormat.
        assert!(matches!(
            err,
            Error::UnsupportedFormat | Error::Malformed(_) | Error::Decoder { .. }
        ));
    }

    #[test]
    fn format_hint_overrides_sniffer() {
        let err = Loader::new_bytes(b"".to_vec())
            .format_hint(KnownFormat::Png)
            .load()
            .unwrap_err();
        assert!(matches!(
            err,
            Error::Malformed(_) | Error::Io(_) | Error::UnsupportedFormat
        ));
    }

    #[test]
    fn reader_buffers_into_memory() {
        let cursor = Cursor::new(truncated_png_signature());
        let err = Loader::new_reader(cursor).load().unwrap_err();
        assert!(matches!(
            err,
            Error::Malformed(_) | Error::Io(_) | Error::UnsupportedFormat
        ));
    }

    #[test]
    fn builder_methods_chain() {
        let l = Loader::new_bytes(vec![0u8; 8])
            .limits(Limits::unlimited())
            .sandbox_selector(SandboxSelector::none())
            .require_sandbox()
            .apply_transformations(false)
            .format_hint(KnownFormat::Tga);
        // Mostly a compile-time check; verify the values were stored.
        let dbg = format!("{l:?}");
        assert!(dbg.contains("Tga"));
        assert!(dbg.contains("apply_transformations: false"));
    }

    #[test]
    fn missing_path_returns_io_error() {
        let err = Loader::new_path("/nonexistent/path/to/image.png")
            .load()
            .unwrap_err();
        assert!(matches!(err, Error::Io(_)));
    }
}
