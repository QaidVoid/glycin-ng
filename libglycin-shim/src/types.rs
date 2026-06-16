//! Internal handle types attached to the GObjects we hand out.
//!
//! These wrap opaque pointers returned by `libglycin_ng.so`. The
//! `Drop` impls call the matching `glycin_ng_*_free` so freeing the
//! GObject also tears down the underlying handle.

use std::ffi::CString;
use std::sync::{Arc, Mutex};

use crate::ngapi::{self, GlycinNgEncodedImage, GlycinNgEncoder, GlycinNgImage, GlycinNgLoader};

/// Shadow copy of glycin-ng's [`Limits`] knobs. Each field defaults
/// to `None` (use glycin-ng's built-in default). The loader applies
/// the fields lazily so callers can populate them through the
/// builder and have the values forwarded to glycin-ng at the
/// appropriate point.
#[derive(Copy, Clone, Default)]
pub(crate) struct LoaderLimits {
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub max_pixels: Option<u64>,
    pub max_frames: Option<u32>,
    pub max_animation_seconds: Option<u64>,
    pub decode_memory_mib: Option<u64>,
    pub decode_cpu_seconds: Option<u64>,
}

impl LoaderLimits {
    /// Push the configured values onto the underlying `GlycinNgLoader`.
    /// Each setter on the C ABI is a no-op when given a "don't care"
    /// caller value, but we still skip the call when the shadow has
    /// `None` to avoid clobbering glycin-ng's default behaviour.
    pub(crate) fn apply(&self, loader: *mut GlycinNgLoader) {
        if let Some(v) = self.max_width {
            unsafe { ngapi::glycin_ng_loader_set_max_width(loader, v) };
        }
        if let Some(v) = self.max_height {
            unsafe { ngapi::glycin_ng_loader_set_max_height(loader, v) };
        }
        if let Some(v) = self.max_pixels {
            unsafe { ngapi::glycin_ng_loader_set_max_pixels(loader, v) };
        }
        if let Some(v) = self.max_frames {
            unsafe { ngapi::glycin_ng_loader_set_max_frames(loader, v) };
        }
        if let Some(v) = self.max_animation_seconds {
            unsafe { ngapi::glycin_ng_loader_set_max_animation_seconds(loader, v) };
        }
        if let Some(v) = self.decode_memory_mib {
            unsafe { ngapi::glycin_ng_loader_set_decode_memory_mib(loader, v) };
        }
        if let Some(v) = self.decode_cpu_seconds {
            unsafe { ngapi::glycin_ng_loader_set_decode_cpu_seconds(loader, v) };
        }
    }
}

/// State backing a `GlyLoader`. The inner pointer is consumed on
/// `gly_loader_load`, so it lives behind an `Option`.
pub(crate) struct LoaderState {
    pub(crate) inner: Mutex<Option<*mut GlycinNgLoader>>,
    pub(crate) apply_transformations: Mutex<bool>,
    pub(crate) accepted_memory_formats: Mutex<u32>,
    pub(crate) limits: Mutex<LoaderLimits>,
    /// Original source bytes, retained so resolution-independent
    /// formats (SVG) can be re-decoded at a different size when the
    /// consumer calls `gly_frame_request_set_scale` after
    /// `gly_loader_load`.
    pub(crate) source_bytes: Option<Arc<[u8]>>,
}

// Raw pointers are !Send/!Sync by default. The shim's Mutex guards
// the only mutation paths and glycin-ng documents handles as
// thread-compatible. Sending across threads is safe in this usage.
unsafe impl Send for LoaderState {}
unsafe impl Sync for LoaderState {}

impl LoaderState {
    pub(crate) fn new(loader: *mut GlycinNgLoader, source_bytes: Option<Arc<[u8]>>) -> Self {
        Self {
            inner: Mutex::new(Some(loader)),
            apply_transformations: Mutex::new(true),
            accepted_memory_formats: Mutex::new(0),
            limits: Mutex::new(LoaderLimits::default()),
            source_bytes,
        }
    }
}

impl Drop for LoaderState {
    fn drop(&mut self) {
        if let Some(p) = self.inner.get_mut().ok().and_then(|o| o.take())
            && !p.is_null()
        {
            unsafe { ngapi::glycin_ng_loader_free(p) };
        }
    }
}

/// State backing a `GlyImage`. The cursor advances each time the
/// consumer asks for the next frame.
pub(crate) struct ImageState {
    pub(crate) inner: *mut GlycinNgImage,
    pub(crate) cursor: Mutex<usize>,
    pub(crate) frame_count: usize,
    pub(crate) rerender: Option<Rerender>,
    /// Detected MIME type, cached so `gly_image_get_mime_type` can
    /// hand back a stable `const char*` owned by the image. `None`
    /// when the format has no MIME mapping.
    pub(crate) mime: Option<CString>,
}

unsafe impl Send for ImageState {}
unsafe impl Sync for ImageState {}

impl ImageState {
    pub(crate) fn new(
        image: *mut GlycinNgImage,
        frame_count: usize,
        rerender: Option<Rerender>,
        mime: Option<CString>,
    ) -> Self {
        Self {
            inner: image,
            cursor: Mutex::new(0),
            frame_count,
            rerender,
            mime,
        }
    }

    /// Returns the next frame index (clamped to the last frame on
    /// non-looping; wraps to 0 on looping).
    pub(crate) fn advance(&self, loop_animation: bool) -> Option<usize> {
        if self.frame_count == 0 {
            return None;
        }
        let mut c = self.cursor.lock().unwrap();
        let idx = *c;
        if idx >= self.frame_count {
            if loop_animation {
                *c = 1;
                return Some(0);
            }
            return None;
        }
        *c = idx + 1;
        Some(idx)
    }
}

impl Drop for ImageState {
    fn drop(&mut self) {
        if !self.inner.is_null() {
            unsafe { ngapi::glycin_ng_image_free(self.inner) };
        }
    }
}

/// Snapshot of the decode configuration needed to rebuild an SVG
/// (or similar vector format) at a different output size.
pub(crate) struct Rerender {
    pub(crate) source_bytes: Arc<[u8]>,
    pub(crate) limits: LoaderLimits,
    pub(crate) apply_transformations: bool,
    pub(crate) accepted_memory_formats: u32,
}

/// State backing a `GlyFrame`. Owns its pixel data so the frame
/// outlives the originating image handle.
pub(crate) struct FrameState {
    pub(crate) frame: crate::convert::RawFrame,
}

/// State backing a `GlyFrameRequest`.
pub(crate) struct FrameRequestState {
    pub(crate) scale: Mutex<Option<(u32, u32)>>,
    pub(crate) loop_animation: Mutex<bool>,
}

impl FrameRequestState {
    pub(crate) fn new() -> Self {
        Self {
            scale: Mutex::new(None),
            loop_animation: Mutex::new(true),
        }
    }
}

/// State backing a `GlyCreator`. Wraps a `GlycinNgEncoder`.
pub(crate) struct CreatorState {
    pub(crate) encoder: Mutex<*mut GlycinNgEncoder>,
}

unsafe impl Send for CreatorState {}
unsafe impl Sync for CreatorState {}

impl Drop for CreatorState {
    fn drop(&mut self) {
        if let Ok(p) = self.encoder.get_mut()
            && !p.is_null()
        {
            unsafe { ngapi::glycin_ng_encoder_free(*p) };
        }
    }
}

/// State backing a `GlyEncodedImage`.
pub(crate) struct EncodedImageState {
    pub(crate) inner: *mut GlycinNgEncodedImage,
}

unsafe impl Send for EncodedImageState {}
unsafe impl Sync for EncodedImageState {}

impl Drop for EncodedImageState {
    fn drop(&mut self) {
        if !self.inner.is_null() {
            unsafe { ngapi::glycin_ng_encoded_image_free(self.inner) };
        }
    }
}
