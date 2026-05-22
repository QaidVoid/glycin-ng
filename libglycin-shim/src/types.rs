//! Internal handle types attached to the GObjects we hand out.

use std::sync::{Arc, Mutex};

use glycin_ng::{Frame, Image, Limits, Loader};

/// State backing a `GlyLoader`. The inner [`Loader`] is consumed on
/// `gly_loader_load`, so it lives behind an `Option` we can `take()`.
pub(crate) struct LoaderState {
    pub(crate) inner: Mutex<Option<Loader>>,
    pub(crate) apply_transformations: Mutex<bool>,
    pub(crate) limits: Mutex<Limits>,
    /// Bitmask of `GlyMemoryFormatSelection` values the caller said
    /// they accept. `0` means the call was never made; we then leave
    /// the decoded format alone.
    pub(crate) accepted_memory_formats: Mutex<u32>,
    /// Original source bytes, retained so resolution-independent
    /// formats (SVG) can be re-decoded at a different size when the
    /// consumer calls `gly_frame_request_set_scale` after
    /// `gly_loader_load`. `None` if reading the source failed at
    /// `gly_loader_new` time; the inner `Loader` may still succeed
    /// from its own source.
    pub(crate) source_bytes: Option<Arc<[u8]>>,
}

impl LoaderState {
    pub(crate) fn new(loader: Loader, source_bytes: Option<Arc<[u8]>>) -> Self {
        Self {
            inner: Mutex::new(Some(loader)),
            apply_transformations: Mutex::new(true),
            limits: Mutex::new(Limits::default()),
            accepted_memory_formats: Mutex::new(0),
            source_bytes,
        }
    }
}

/// State backing a `GlyImage`. Holds the decoded frames and an
/// iterator cursor that advances each time the consumer asks for
/// `gly_image_get_specific_frame` or `gly_image_next_frame`.
pub(crate) struct ImageState {
    pub(crate) image: Image,
    pub(crate) cursor: Mutex<usize>,
    /// Inputs the original decode used, kept so a later
    /// `gly_image_get_specific_frame` can re-decode at a caller-
    /// requested scale. Only populated for resolution-independent
    /// formats; raster decoders ignore the scale hint and gdk-pixbuf
    /// scales their bitmap output itself.
    pub(crate) rerender: Option<Rerender>,
}

/// Snapshot of the decode configuration needed to rebuild an SVG
/// (or similar vector format) at a different output size.
pub(crate) struct Rerender {
    pub(crate) source_bytes: Arc<[u8]>,
    pub(crate) limits: Limits,
    pub(crate) apply_transformations: bool,
    pub(crate) accepted_memory_formats: u32,
}

impl ImageState {
    pub(crate) fn new(image: Image, rerender: Option<Rerender>) -> Self {
        Self {
            image,
            cursor: Mutex::new(0),
            rerender,
        }
    }

    /// Returns the next frame index (clamped to the last frame on
    /// non-looping; wraps to 0 on looping).
    pub(crate) fn advance(&self, loop_animation: bool) -> Option<usize> {
        let total = self.image.frames().len();
        if total == 0 {
            return None;
        }
        let mut c = self.cursor.lock().unwrap();
        let idx = *c;
        if idx >= total {
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

/// State backing a `GlyFrame`. Holds the texture bytes shared with
/// the host via a [`glib::Bytes`](crate::ffi::GBytes).
pub(crate) struct FrameState {
    pub(crate) frame: Frame,
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

/// State backing a `GlyCreator`. Wraps a [`glycin_ng::Encoder`]; the
/// shim forwards every `gly_creator_*` C entry point to it. Lives
/// behind an `Option` so `gly_creator_create` can `take()` the
/// inner encoder (which consumes self on `encode`).
pub(crate) struct CreatorState {
    pub(crate) encoder: Mutex<Option<glycin_ng::Encoder>>,
}

/// State backing a `GlyEncodedImage`.
pub(crate) struct EncodedImageState {
    pub(crate) data: Vec<u8>,
}
