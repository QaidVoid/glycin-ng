//! glycin 2.2 color-mode entry points.
//!
//! `gly_frame_get_color_mode` tells the caller which accessor holds
//! the frame's color information. The engine reports CICP code
//! points but does not hand ICC profiles to the shim, so a frame is
//! `CICP` when it has code points and `SRGB` otherwise. That is the
//! same assumption callers of the pre-2.2 API already make.

use std::ffi::c_int;
use std::ptr;

use crate::ffi::{GBytes, GObject, gboolean};
use crate::state_ref;
use crate::types::FrameState;

pub(crate) const GLY_COLOR_MODE_SRGB: c_int = 1;
pub(crate) const GLY_COLOR_MODE_CICP: c_int = 2;

/// # Safety
/// `frame` must be valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_frame_get_color_mode(frame: *mut GObject) -> c_int {
    match unsafe { state_ref::<FrameState>(frame) }.and_then(|s| s.cicp) {
        Some(_) => GLY_COLOR_MODE_CICP,
        None => GLY_COLOR_MODE_SRGB,
    }
}

/// Always `NULL`: no frame reports `GLY_COLOR_MODE_ICC_PROFILE`, and
/// upstream returns `NULL` whenever the color mode is not ICC.
///
/// # Safety
/// Always safe; `frame` is not read.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_frame_get_color_icc_profile(_frame: *mut GObject) -> *mut GBytes {
    ptr::null_mut()
}

/// Accepted for ABI compatibility. The engine never converts ICC
/// profiles, so there is nothing to switch.
///
/// # Safety
/// Always safe; nothing is read or written.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_loader_set_color_convert_icc_srgb(
    _loader: *mut GObject,
    _convert: gboolean,
) {
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
