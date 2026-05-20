//! Subset of GLib / GObject / GIO FFI this shim needs.
//!
//! Symbols are resolved by the host process at `dlopen` time (Arch's
//! `libgdk_pixbuf-2.0.so.0` already links these). Test builds use
//! the stubs in [`test_stubs`] so the test binary does not try to
//! load `libgobject-2.0` etc.

#![allow(non_camel_case_types)]

use std::ffi::{c_char, c_int, c_void};

pub type GType = usize;
pub type GQuark = u32;
pub type gboolean = c_int;
pub type GDestroyNotify = Option<unsafe extern "C" fn(data: *mut c_void)>;
pub type GStrv = *mut *mut c_char;
pub type gpointer = *mut c_void;

#[repr(C)]
pub struct GObject {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GFile {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GBytes {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GInputStream {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GError {
    pub domain: GQuark,
    pub code: c_int,
    pub message: *mut c_char,
}

#[cfg(not(test))]
#[allow(dead_code)]
unsafe extern "C" {
    pub fn g_object_get_type() -> GType;
    pub fn g_object_new(object_type: GType, first_property_name: *const c_char) -> *mut GObject;
    pub fn g_object_set_data_full(
        object: *mut GObject,
        key: *const c_char,
        data: gpointer,
        destroy: GDestroyNotify,
    );
    pub fn g_object_get_data(object: *mut GObject, key: *const c_char) -> gpointer;
    pub fn g_object_unref(object: *mut c_void);
    pub fn g_object_ref(object: *mut c_void) -> *mut c_void;

    pub fn g_file_get_path(file: *mut GFile) -> *mut c_char;
    pub fn g_file_get_uri(file: *mut GFile) -> *mut c_char;

    pub fn g_bytes_get_data(bytes: *mut GBytes, size: *mut usize) -> *const c_void;
    pub fn g_bytes_new(data: *const c_void, size: usize) -> *mut GBytes;
    pub fn g_bytes_unref(bytes: *mut GBytes);

    pub fn g_input_stream_read(
        stream: *mut GInputStream,
        buffer: *mut c_void,
        count: usize,
        cancellable: *mut c_void,
        error: *mut *mut GError,
    ) -> isize;

    pub fn g_quark_from_static_string(string: *const c_char) -> GQuark;
    pub fn g_set_error_literal(
        err: *mut *mut GError,
        domain: GQuark,
        code: c_int,
        message: *const c_char,
    );

    pub fn g_strv_length(str_array: GStrv) -> u32;
    pub fn g_strdup(str: *const c_char) -> *mut c_char;
    pub fn g_free(ptr: *mut c_void);
    pub fn g_malloc0(n_bytes: usize) -> *mut c_void;
}

#[cfg(test)]
pub use test_stubs::*;

#[cfg(test)]
#[allow(dead_code)]
mod test_stubs {
    use super::{GBytes, GError, GFile, GInputStream, GObject, GQuark, GStrv, GType, gpointer};
    use std::ffi::{c_char, c_int, c_void};

    pub unsafe extern "C" fn g_object_get_type() -> GType {
        80
    }
    pub unsafe extern "C" fn g_object_new(_: GType, _: *const c_char) -> *mut GObject {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn g_object_set_data_full(
        _: *mut GObject,
        _: *const c_char,
        _: gpointer,
        _: super::GDestroyNotify,
    ) {
    }
    pub unsafe extern "C" fn g_object_get_data(_: *mut GObject, _: *const c_char) -> gpointer {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn g_object_unref(_: *mut c_void) {}
    pub unsafe extern "C" fn g_object_ref(p: *mut c_void) -> *mut c_void {
        p
    }
    pub unsafe extern "C" fn g_file_get_path(_: *mut GFile) -> *mut c_char {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn g_file_get_uri(_: *mut GFile) -> *mut c_char {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn g_bytes_get_data(_: *mut GBytes, _: *mut usize) -> *const c_void {
        std::ptr::null()
    }
    pub unsafe extern "C" fn g_bytes_new(_: *const c_void, _: usize) -> *mut GBytes {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn g_bytes_unref(_: *mut GBytes) {}
    pub unsafe extern "C" fn g_input_stream_read(
        _: *mut GInputStream,
        _: *mut c_void,
        _: usize,
        _: *mut c_void,
        _: *mut *mut GError,
    ) -> isize {
        0
    }
    pub unsafe extern "C" fn g_quark_from_static_string(_: *const c_char) -> GQuark {
        1
    }
    pub unsafe extern "C" fn g_set_error_literal(
        _: *mut *mut GError,
        _: GQuark,
        _: c_int,
        _: *const c_char,
    ) {
    }
    pub unsafe extern "C" fn g_strv_length(_: GStrv) -> u32 {
        0
    }
    pub unsafe extern "C" fn g_strdup(_: *const c_char) -> *mut c_char {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn g_free(_: *mut c_void) {}
    pub unsafe extern "C" fn g_malloc0(_: usize) -> *mut c_void {
        std::ptr::null_mut()
    }
}
