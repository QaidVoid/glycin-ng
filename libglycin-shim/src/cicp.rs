//! `GlyCicp` boxed type and the frame color accessor.
//!
//! `GlyCicp` carries ITU-T H.273 coding-independent code points. The
//! engine does not yet surface CICP, so `gly_frame_get_color_cicp`
//! reports `NULL`, which upstream defines as "no CICP used". The
//! boxed copy and free entry points are implemented fully so callers
//! and `g_boxed_*` machinery behave correctly once CICP data exists.

use std::ffi::c_void;
use std::ptr;

use crate::ffi::{self, GObject};

/// ITU-T H.273 coding-independent code points for a frame's color.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct GlyCicp {
    pub color_primaries: u8,
    pub transfer_characteristics: u8,
    pub matrix_coefficients: u8,
    pub video_full_range_flag: u8,
}

/// Duplicate a `GlyCicp` into a fresh `g_malloc` allocation.
///
/// # Safety
/// `cicp` must be a valid `GlyCicp` pointer or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_cicp_copy(cicp: *mut GlyCicp) -> *mut GlyCicp {
    if cicp.is_null() {
        return ptr::null_mut();
    }
    let new = unsafe { ffi::g_malloc0(size_of::<GlyCicp>()) } as *mut GlyCicp;
    if new.is_null() {
        return ptr::null_mut();
    }
    unsafe { *new = *cicp };
    new
}

/// Free a `GlyCicp` allocated by [`gly_cicp_copy`] or returned from
/// [`gly_frame_get_color_cicp`].
///
/// # Safety
/// `cicp` must come from this library's allocator or be NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_cicp_free(cicp: *mut GlyCicp) {
    if !cicp.is_null() {
        unsafe { ffi::g_free(cicp as *mut c_void) };
    }
}

/// Return the frame's CICP, or `NULL` when no CICP applies.
///
/// # Safety
/// `frame` must be valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_frame_get_color_cicp(_frame: *mut GObject) -> *mut GlyCicp {
    ptr::null_mut()
}
