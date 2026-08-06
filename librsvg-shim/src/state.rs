//! Per-handle state and the pure math shared by the rendering and
//! dimension paths.

use std::ffi::{CString, c_int};

use crate::ffi::{GDestroyNotify, gpointer};
use crate::ngapi;

/// librsvg's historical default DPI.
pub const DEFAULT_DPI: f64 = 90.0;
/// Default font size used for em/ex conversion, matching the usvg
/// default the engine parses with.
pub const FONT_SIZE: f64 = 12.0;

/// `RsvgSizeFunc` from rsvg.h.
pub type RsvgSizeFunc =
    Option<unsafe extern "C" fn(width: *mut c_int, height: *mut c_int, user_data: gpointer)>;

/// Deprecated sizing-callback override installed with
/// `rsvg_handle_set_size_callback`.
pub struct SizeCallback {
    pub func: RsvgSizeFunc,
    pub data: gpointer,
    pub destroy: GDestroyNotify,
}

impl SizeCallback {
    pub const fn none() -> Self {
        Self {
            func: None,
            data: std::ptr::null_mut(),
            destroy: None,
        }
    }

    /// Run the callback over the natural integer size, if set.
    pub fn apply(&self, width: i32, height: i32) -> (i32, i32) {
        match self.func {
            None => (width, height),
            Some(f) => {
                let (mut w, mut h) = (width, height);
                unsafe { f(&mut w, &mut h, self.data) };
                (w, h)
            }
        }
    }
}

impl Drop for SizeCallback {
    fn drop(&mut self) {
        if let Some(destroy) = self.destroy
            && !self.data.is_null()
        {
            unsafe { destroy(self.data) };
        }
    }
}

/// Where the handle is in librsvg's documented loading lifecycle.
pub enum LoadState {
    /// Constructed, no data fed yet.
    Start,
    /// Accumulating bytes from `rsvg_handle_write`.
    Loading(Vec<u8>),
    /// Fully loaded: the engine document handle.
    Loaded(*mut ngapi::GlycinNgSvg),
    /// A load was attempted and failed.
    Failed,
}

/// The Rust state hung off every `RsvgHandle` instance.
pub struct HandleState {
    /// Raw property values; `<= 0` means "use [`DEFAULT_DPI`]".
    pub dpi_x: f64,
    pub dpi_y: f64,
    /// `RsvgHandleFlags` bits, accepted and carried but not acted on
    /// (the engine has no unlimited-XML or keep-image-data modes).
    pub flags: u32,
    pub base_uri: Option<CString>,
    pub stylesheet: Option<Vec<u8>>,
    pub size_callback: SizeCallback,
    /// Accepted via `rsvg_handle_set_cancellable_for_rendering`;
    /// renders currently run to completion.
    pub rendering_cancellable: gpointer,
    pub load: LoadState,
}

impl HandleState {
    pub fn new() -> Self {
        Self {
            dpi_x: 0.0,
            dpi_y: 0.0,
            flags: 0,
            base_uri: None,
            stylesheet: None,
            size_callback: SizeCallback::none(),
            rendering_cancellable: std::ptr::null_mut(),
            load: LoadState::Start,
        }
    }

    /// DPI handed to the engine, which carries a single value: the
    /// horizontal axis wins, the vertical one is used when only it
    /// was set, and [`DEFAULT_DPI`] covers neither being set.
    pub fn effective_dpi(&self) -> f64 {
        if self.dpi_x > 0.0 {
            self.dpi_x
        } else if self.dpi_y > 0.0 {
            self.dpi_y
        } else {
            DEFAULT_DPI
        }
    }

    pub fn document(&self) -> Option<*mut ngapi::GlycinNgSvg> {
        match self.load {
            LoadState::Loaded(doc) => Some(doc),
            _ => None,
        }
    }

    /// Directory used for resolving relative external references,
    /// derived from the base URI when it points into the
    /// filesystem. File URIs with a hostname are rejected.
    pub fn resources_dir(&self) -> Option<CString> {
        let uri = self.base_uri.as_ref()?.to_str().ok()?;
        let path: Vec<u8> = if let Some(rest) = uri.strip_prefix("file://") {
            if !rest.starts_with('/') {
                return None;
            }
            percent_decode(rest)?
        } else if uri.starts_with('/') {
            uri.as_bytes().to_vec()
        } else {
            return None;
        };
        use std::os::unix::ffi::OsStrExt;
        let path = std::path::Path::new(std::ffi::OsStr::from_bytes(&path));
        let dir = path.parent()?;
        CString::new(dir.as_os_str().as_encoded_bytes()).ok()
    }
}

impl Drop for HandleState {
    fn drop(&mut self) {
        if let LoadState::Loaded(doc) = self.load {
            unsafe { ngapi::glycin_ng_svg_free(doc) };
        }
    }
}

/// A rectangle in the librsvg public ABI layout.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RsvgRectangle {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// `RsvgDimensionData` from rsvg.h.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct RsvgDimensionData {
    pub width: c_int,
    pub height: c_int,
    pub em: f64,
    pub ex: f64,
}

/// `RsvgPositionData` from rsvg.h.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct RsvgPositionData {
    pub x: c_int,
    pub y: c_int,
}

/// `RsvgLength` from rsvg.h. The unit field uses librsvg's
/// `RsvgUnit` numbering, which the engine mirrors.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct RsvgLength {
    pub length: f64,
    pub unit: u32,
}

/// Decode `%XX` escapes in a URI path segment into raw bytes.
fn percent_decode(s: &str) -> Option<Vec<u8>> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hi = (*bytes.get(i + 1)? as char).to_digit(16)?;
            let lo = (*bytes.get(i + 2)? as char).to_digit(16)?;
            out.push((hi * 16 + lo) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Some(out)
}

/// Convert an `RsvgLength` to pixels, `None` for percentages (which
/// need a viewport to resolve).
pub fn length_to_pixels(length: f64, unit: u32, dpi: f64) -> Option<f64> {
    match unit {
        1 => Some(length),                   // px
        2 => Some(length * FONT_SIZE),       // em
        3 => Some(length * FONT_SIZE * 0.5), // ex
        4 => Some(length * dpi),             // in
        5 => Some(length * dpi / 2.54),      // cm
        6 => Some(length * dpi / 25.4),      // mm
        7 => Some(length * dpi / 72.0),      // pt
        8 => Some(length * dpi / 6.0),       // pc
        9 => Some(length * FONT_SIZE * 0.5), // ch, approximated
        _ => None,                           // percent
    }
}

/// Uniform "meet" fit of a `doc_w` x `doc_h` document into a
/// viewport, centered on both axes (the dominant
/// `preserveAspectRatio="xMidYMid meet"` case).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    pub scale: f64,
    pub offset_x: f64,
    pub offset_y: f64,
}

pub fn fit_document(doc_w: f64, doc_h: f64, viewport: &RsvgRectangle) -> Option<Fit> {
    if doc_w <= 0.0 || doc_h <= 0.0 || viewport.width <= 0.0 || viewport.height <= 0.0 {
        return None;
    }
    let scale = (viewport.width / doc_w).min(viewport.height / doc_h);
    Some(Fit {
        scale,
        offset_x: viewport.x + (viewport.width - doc_w * scale) / 2.0,
        offset_y: viewport.y + (viewport.height - doc_h * scale) / 2.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_units_convert_at_90dpi() {
        let dpi = 90.0;
        assert_eq!(length_to_pixels(2.0, 1, dpi), Some(2.0));
        assert_eq!(length_to_pixels(2.0, 4, dpi), Some(180.0));
        assert_eq!(length_to_pixels(2.54, 5, dpi), Some(90.0));
        assert_eq!(length_to_pixels(25.4, 6, dpi), Some(90.0));
        assert_eq!(length_to_pixels(72.0, 7, dpi), Some(90.0));
        assert_eq!(length_to_pixels(6.0, 8, dpi), Some(90.0));
        assert_eq!(length_to_pixels(1.0, 2, dpi), Some(FONT_SIZE));
        assert_eq!(length_to_pixels(0.5, 0, dpi), None);
    }

    #[test]
    fn fit_is_uniform_and_centered() {
        // 20x10 document into a 40x40 viewport at (5, 5): scale 2,
        // centered vertically.
        let vp = RsvgRectangle {
            x: 5.0,
            y: 5.0,
            width: 40.0,
            height: 40.0,
        };
        let fit = fit_document(20.0, 10.0, &vp).unwrap();
        assert_eq!(fit.scale, 2.0);
        assert_eq!(fit.offset_x, 5.0);
        assert_eq!(fit.offset_y, 15.0);

        assert!(fit_document(0.0, 10.0, &vp).is_none());
        assert!(
            fit_document(
                20.0,
                10.0,
                &RsvgRectangle {
                    width: 0.0,
                    ..Default::default()
                }
            )
            .is_none()
        );
    }

    #[test]
    fn size_callback_applies_and_defaults() {
        let cb = SizeCallback::none();
        assert_eq!(cb.apply(64, 32), (64, 32));

        unsafe extern "C" fn double_it(w: *mut c_int, h: *mut c_int, _: gpointer) {
            unsafe {
                *w *= 2;
                *h *= 2;
            }
        }
        let cb = SizeCallback {
            func: Some(double_it),
            data: std::ptr::null_mut(),
            destroy: None,
        };
        assert_eq!(cb.apply(64, 32), (128, 64));
    }

    #[test]
    fn resources_dir_from_base_uri() {
        let mut state = HandleState::new();
        assert!(state.resources_dir().is_none());

        state.base_uri = Some(CString::new("file:///tmp/icons/foo.svg").unwrap());
        assert_eq!(
            state.resources_dir().unwrap().to_str().unwrap(),
            "/tmp/icons"
        );

        state.base_uri = Some(CString::new("/tmp/icons/foo.svg").unwrap());
        assert_eq!(
            state.resources_dir().unwrap().to_str().unwrap(),
            "/tmp/icons"
        );

        state.base_uri = Some(CString::new("https://example.com/a.svg").unwrap());
        assert!(state.resources_dir().is_none());

        // Percent-encoded UTF-8 and spaces decode fully.
        state.base_uri = Some(CString::new("file:///tmp/My%20Bilder%C3%A4/x.svg").unwrap());
        assert_eq!(
            state.resources_dir().unwrap().as_bytes(),
            "/tmp/My Bilder\u{e4}".as_bytes()
        );

        // Truncated escapes are rejected rather than mangled.
        state.base_uri = Some(CString::new("file:///tmp/bad%2/x.svg").unwrap());
        assert!(state.resources_dir().is_none());
    }

    #[test]
    fn effective_dpi_defaults_to_90() {
        let mut state = HandleState::new();
        assert_eq!(state.effective_dpi(), DEFAULT_DPI);
        state.dpi_x = 96.0;
        assert_eq!(state.effective_dpi(), 96.0);
        state.dpi_x = -5.0;
        assert_eq!(state.effective_dpi(), DEFAULT_DPI);
        // Only dpi-y set: it stands in for the single engine DPI.
        state.dpi_y = 120.0;
        assert_eq!(state.effective_dpi(), 120.0);
        state.dpi_x = 96.0;
        assert_eq!(state.effective_dpi(), 96.0);
    }
}
