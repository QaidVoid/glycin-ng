//! glycin 2.2 color-mode entry points.
//!
//! `gly_frame_get_color_mode` tells the caller which accessor holds
//! the frame's color information: ICC profile when present, CICP
//! when only code points are present, otherwise sRGB.

use std::ffi::c_int;
use std::ptr;

use crate::ffi::{GBytes, GObject, gboolean};
use crate::state_ref;
use crate::types::FrameState;

pub(crate) const GLY_COLOR_MODE_SRGB: c_int = 1;
pub(crate) const GLY_COLOR_MODE_CICP: c_int = 2;
pub(crate) const GLY_COLOR_MODE_ICC: c_int = 3;

/// # Safety
/// `frame` must be valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_frame_get_color_mode(frame: *mut GObject) -> c_int {
    let Some(state) = (unsafe { state_ref::<FrameState>(frame) }) else {
        return GLY_COLOR_MODE_SRGB;
    };
    if state.icc_profile.is_some() {
        GLY_COLOR_MODE_ICC
    } else if state.cicp.is_some() {
        GLY_COLOR_MODE_CICP
    } else {
        GLY_COLOR_MODE_SRGB
    }
}

/// Return the frame ICC profile as a fresh `GBytes`, or NULL when
/// absent.
///
/// # Safety
/// `frame` must be valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_frame_get_color_icc_profile(frame: *mut GObject) -> *mut GBytes {
    let Some(state) = (unsafe { state_ref::<FrameState>(frame) }) else {
        return ptr::null_mut();
    };
    match &state.icc_profile {
        Some(bytes) if !bytes.is_empty() => unsafe {
            crate::ffi::g_bytes_new(bytes.as_ptr() as *const std::ffi::c_void, bytes.len())
        },
        _ => ptr::null_mut(),
    }
}

/// Whether to convert ICC-profiled textures to sRGB. Stored for ABI
/// compatibility; the engine decodes without color conversion so the
/// flag has no effect.
///
/// # Safety
/// `loader` must be valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_loader_set_color_convert_icc_srgb(
    loader: *mut GObject,
    convert: gboolean,
) {
    let Some(state) = (unsafe { state_ref::<crate::types::LoaderState>(loader) }) else {
        return;
    };
    *state.color_convert_icc_srgb.lock().unwrap() = convert != 0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_frame_reports_srgb() {
        assert_eq!(
            unsafe { gly_frame_get_color_mode(ptr::null_mut()) },
            GLY_COLOR_MODE_SRGB
        );
    }

    #[test]
    fn icc_profile_is_always_null() {
        assert!(unsafe { gly_frame_get_color_icc_profile(ptr::null_mut()) }.is_null());
    }

    #[test]
    fn convert_icc_srgb_accepts_any_arguments() {
        unsafe {
            gly_loader_set_color_convert_icc_srgb(ptr::null_mut(), 0);
            gly_loader_set_color_convert_icc_srgb(ptr::null_mut(), 1);
        }
    }
}
