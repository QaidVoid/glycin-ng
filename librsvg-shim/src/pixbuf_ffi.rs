//! Subset of gdk-pixbuf FFI needed by the `rsvg_*pixbuf*` functions.

#![allow(non_camel_case_types)]

use crate::ffi::gpointer;
use std::ffi::c_int;

#[repr(C)]
pub struct GdkPixbuf {
    _private: [u8; 0],
}

/// `GdkPixbufDestroyNotify`: frees the pixel buffer once the pixbuf
/// is finalized.
pub type GdkPixbufDestroyNotify = Option<unsafe extern "C" fn(pixels: *mut u8, data: gpointer)>;

pub const GDK_COLORSPACE_RGB: c_int = 0;

#[cfg(not(test))]
unsafe extern "C" {
    pub fn gdk_pixbuf_new_from_data(
        data: *const u8,
        colorspace: c_int,
        has_alpha: c_int,
        bits_per_sample: c_int,
        width: c_int,
        height: c_int,
        rowstride: c_int,
        destroy_fn: GdkPixbufDestroyNotify,
        destroy_fn_data: gpointer,
    ) -> *mut GdkPixbuf;
}

#[cfg(test)]
pub unsafe extern "C" fn gdk_pixbuf_new_from_data(
    _data: *const u8,
    _colorspace: c_int,
    _has_alpha: c_int,
    _bits_per_sample: c_int,
    _width: c_int,
    _height: c_int,
    _rowstride: c_int,
    _destroy_fn: GdkPixbufDestroyNotify,
    _destroy_fn_data: gpointer,
) -> *mut GdkPixbuf {
    std::ptr::null_mut()
}
