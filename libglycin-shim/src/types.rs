//! Internal handle types attached to the GObjects we hand out.

use std::sync::Mutex;

use glycin_ng::{Frame, Image, Limits, Loader, SandboxSelector};

/// State backing a `GlyLoader`. The inner [`Loader`] is consumed on
/// `gly_loader_load`, so it lives behind an `Option` we can `take()`.
pub(crate) struct LoaderState {
    pub(crate) inner: Mutex<Option<Loader>>,
    pub(crate) apply_transformations: Mutex<bool>,
    pub(crate) sandbox: Mutex<SandboxSelector>,
    pub(crate) limits: Mutex<Limits>,
}

impl LoaderState {
    pub(crate) fn new(loader: Loader) -> Self {
        Self {
            inner: Mutex::new(Some(loader)),
            apply_transformations: Mutex::new(true),
            sandbox: Mutex::new(SandboxSelector::default()),
            limits: Mutex::new(Limits::default()),
        }
    }
}

/// State backing a `GlyImage`. Holds the decoded frames and an
/// iterator cursor that advances each time the consumer asks for
/// `gly_image_get_specific_frame` or `gly_image_next_frame`.
pub(crate) struct ImageState {
    pub(crate) image: Image,
    pub(crate) cursor: Mutex<usize>,
}

impl ImageState {
    pub(crate) fn new(image: Image) -> Self {
        Self {
            image,
            cursor: Mutex::new(0),
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
