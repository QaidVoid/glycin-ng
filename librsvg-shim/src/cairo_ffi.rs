//! Subset of cairo FFI needed to paint rendered pixmaps onto a
//! caller's `cairo_t`.

#![allow(non_camel_case_types)]

use std::ffi::{c_double, c_int, c_uchar};

#[repr(C)]
pub struct cairo_t {
    _private: [u8; 0],
}

#[repr(C)]
pub struct cairo_surface_t {
    _private: [u8; 0],
}

/// Layout of `cairo_matrix_t` (public cairo ABI).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct cairo_matrix_t {
    pub xx: c_double,
    pub yx: c_double,
    pub xy: c_double,
    pub yy: c_double,
    pub x0: c_double,
    pub y0: c_double,
}

pub const CAIRO_STATUS_SUCCESS: c_int = 0;
pub const CAIRO_FORMAT_ARGB32: c_int = 0;

#[cfg(not(test))]
unsafe extern "C" {
    pub fn cairo_status(cr: *mut cairo_t) -> c_int;
    pub fn cairo_get_matrix(cr: *mut cairo_t, matrix: *mut cairo_matrix_t);
    pub fn cairo_save(cr: *mut cairo_t);
    pub fn cairo_restore(cr: *mut cairo_t);
    pub fn cairo_translate(cr: *mut cairo_t, tx: c_double, ty: c_double);
    pub fn cairo_scale(cr: *mut cairo_t, sx: c_double, sy: c_double);
    pub fn cairo_set_source_surface(
        cr: *mut cairo_t,
        surface: *mut cairo_surface_t,
        x: c_double,
        y: c_double,
    );
    pub fn cairo_paint(cr: *mut cairo_t);

    pub fn cairo_image_surface_create(
        format: c_int,
        width: c_int,
        height: c_int,
    ) -> *mut cairo_surface_t;
    pub fn cairo_surface_status(surface: *mut cairo_surface_t) -> c_int;
    pub fn cairo_surface_flush(surface: *mut cairo_surface_t);
    pub fn cairo_surface_mark_dirty(surface: *mut cairo_surface_t);
    pub fn cairo_surface_destroy(surface: *mut cairo_surface_t);
    pub fn cairo_image_surface_get_data(surface: *mut cairo_surface_t) -> *mut c_uchar;
    pub fn cairo_image_surface_get_stride(surface: *mut cairo_surface_t) -> c_int;
}

#[cfg(test)]
pub use test_stubs::*;

#[cfg(test)]
#[allow(dead_code)]
mod test_stubs {
    use super::*;

    pub unsafe extern "C" fn cairo_status(_: *mut cairo_t) -> c_int {
        CAIRO_STATUS_SUCCESS
    }
    pub unsafe extern "C" fn cairo_get_matrix(_: *mut cairo_t, m: *mut cairo_matrix_t) {
        unsafe {
            *m = cairo_matrix_t {
                xx: 1.0,
                yy: 1.0,
                ..Default::default()
            };
        }
    }
    pub unsafe extern "C" fn cairo_save(_: *mut cairo_t) {}
    pub unsafe extern "C" fn cairo_restore(_: *mut cairo_t) {}
    pub unsafe extern "C" fn cairo_translate(_: *mut cairo_t, _: c_double, _: c_double) {}
    pub unsafe extern "C" fn cairo_scale(_: *mut cairo_t, _: c_double, _: c_double) {}
    pub unsafe extern "C" fn cairo_set_source_surface(
        _: *mut cairo_t,
        _: *mut cairo_surface_t,
        _: c_double,
        _: c_double,
    ) {
    }
    pub unsafe extern "C" fn cairo_paint(_: *mut cairo_t) {}
    pub unsafe extern "C" fn cairo_image_surface_create(
        _: c_int,
        _: c_int,
        _: c_int,
    ) -> *mut cairo_surface_t {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn cairo_surface_status(_: *mut cairo_surface_t) -> c_int {
        // NULL stub surface: report failure so paint paths bail out.
        11
    }
    pub unsafe extern "C" fn cairo_surface_flush(_: *mut cairo_surface_t) {}
    pub unsafe extern "C" fn cairo_surface_mark_dirty(_: *mut cairo_surface_t) {}
    pub unsafe extern "C" fn cairo_surface_destroy(_: *mut cairo_surface_t) {}
    pub unsafe extern "C" fn cairo_image_surface_get_data(_: *mut cairo_surface_t) -> *mut c_uchar {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn cairo_image_surface_get_stride(_: *mut cairo_surface_t) -> c_int {
        0
    }
}
