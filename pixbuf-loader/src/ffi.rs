//! Minimal FFI surface for the gdk-pixbuf loader plugin ABI.
//!
//! Layouts mirror `gdk-pixbuf/gdk-pixbuf-io.h` exactly; the framework
//! treats `GdkPixbufModule` as part of its ABI so the field order
//! must match.

#![allow(non_camel_case_types)]

use std::ffi::{c_char, c_int, c_uint, c_void};

/// Opaque GLib types passed through this loader.
#[repr(C)]
pub struct GdkPixbuf {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GdkPixbufSimpleAnim {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GBytes {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GError {
    pub domain: u32,
    pub code: c_int,
    pub message: *mut c_char,
}

#[repr(C)]
pub struct GdkPixbufModulePattern {
    pub prefix: *const c_char,
    pub mask: *const c_char,
    pub relevance: c_int,
}

#[repr(C)]
pub struct GdkPixbufFormat {
    pub name: *const c_char,
    pub signature: *mut GdkPixbufModulePattern,
    pub domain: *const c_char,
    pub description: *const c_char,
    pub mime_types: *mut *const c_char,
    pub extensions: *mut *const c_char,
    pub flags: u32,
    pub disabled: c_int,
    pub license: *const c_char,
}

pub type GdkPixbufModuleSizeFunc =
    unsafe extern "C" fn(width: *mut c_int, height: *mut c_int, data: *mut c_void);
pub type GdkPixbufModulePreparedFunc = unsafe extern "C" fn(
    pixbuf: *mut GdkPixbuf,
    anim: *mut c_void,
    data: *mut c_void,
);
pub type GdkPixbufModuleUpdatedFunc = unsafe extern "C" fn(
    pixbuf: *mut GdkPixbuf,
    x: c_int,
    y: c_int,
    w: c_int,
    h: c_int,
    data: *mut c_void,
);

pub type LoadFn = unsafe extern "C" fn(
    f: *mut libc::FILE,
    error: *mut *mut GError,
) -> *mut GdkPixbuf;

pub type LoadXpmDataFn =
    unsafe extern "C" fn(data: *mut *const c_char) -> *mut GdkPixbuf;

pub type BeginLoadFn = unsafe extern "C" fn(
    size_func: Option<GdkPixbufModuleSizeFunc>,
    prepared_func: Option<GdkPixbufModulePreparedFunc>,
    updated_func: Option<GdkPixbufModuleUpdatedFunc>,
    user_data: *mut c_void,
    error: *mut *mut GError,
) -> *mut c_void;

pub type StopLoadFn = unsafe extern "C" fn(
    context: *mut c_void,
    error: *mut *mut GError,
) -> c_int;

pub type LoadIncrementFn = unsafe extern "C" fn(
    context: *mut c_void,
    buf: *const u8,
    size: c_uint,
    error: *mut *mut GError,
) -> c_int;

pub type LoadAnimationFn = unsafe extern "C" fn(
    f: *mut libc::FILE,
    error: *mut *mut GError,
) -> *mut c_void;

#[repr(C)]
pub struct GdkPixbufModule {
    pub module_name: *mut c_char,
    pub module_path: *mut c_char,
    pub module: *mut c_void,
    pub info: *mut GdkPixbufFormat,

    pub load: Option<LoadFn>,
    pub load_xpm_data: Option<LoadXpmDataFn>,

    pub begin_load: Option<BeginLoadFn>,
    pub stop_load: Option<StopLoadFn>,
    pub load_increment: Option<LoadIncrementFn>,

    pub load_animation: Option<LoadAnimationFn>,

    pub save: Option<extern "C" fn() -> c_int>,
    pub save_to_callback: Option<extern "C" fn() -> c_int>,

    pub is_save_option_supported: Option<extern "C" fn(*const c_char) -> c_int>,

    pub _reserved1: Option<extern "C" fn()>,
    pub _reserved2: Option<extern "C" fn()>,
    pub _reserved3: Option<extern "C" fn()>,
    pub _reserved4: Option<extern "C" fn()>,
}

pub const GDK_COLORSPACE_RGB: c_int = 0;

pub const GDK_PIXBUF_FORMAT_THREADSAFE: u32 = 1 << 2;

// Production build: the cdylib leaves these symbols undefined and
// the dynamic linker resolves them against libgdk_pixbuf-2.0 /
// libgobject-2.0 / libglib-2.0 in the host process at dlopen time.
#[cfg(not(test))]
unsafe extern "C" {
    pub fn gdk_pixbuf_new_from_bytes(
        data: *mut GBytes,
        colorspace: c_int,
        has_alpha: c_int,
        bits_per_sample: c_int,
        width: c_int,
        height: c_int,
        rowstride: c_int,
    ) -> *mut GdkPixbuf;

    pub fn gdk_pixbuf_get_width(pixbuf: *mut GdkPixbuf) -> c_int;
    pub fn gdk_pixbuf_get_height(pixbuf: *mut GdkPixbuf) -> c_int;

    pub fn gdk_pixbuf_simple_anim_new(
        width: c_int,
        height: c_int,
        rate: f32,
    ) -> *mut GdkPixbufSimpleAnim;
    pub fn gdk_pixbuf_simple_anim_add_frame(
        animation: *mut GdkPixbufSimpleAnim,
        pixbuf: *mut GdkPixbuf,
    );
    pub fn gdk_pixbuf_simple_anim_set_loop(
        animation: *mut GdkPixbufSimpleAnim,
        loop_: c_int,
    );

    pub fn g_bytes_new(data: *const c_void, size: usize) -> *mut GBytes;
    pub fn g_bytes_unref(bytes: *mut GBytes);

    pub fn g_object_unref(object: *mut c_void);

    pub fn g_set_error_literal(
        err: *mut *mut GError,
        domain: u32,
        code: c_int,
        message: *const c_char,
    );
    pub fn g_quark_from_static_string(string: *const c_char) -> u32;
}

// Test build: provide stub implementations so the test binary links
// and runs without libgdk_pixbuf-2.0 on the system. None of our
// unit tests actually invoke decode paths that reach into these,
// they just need to be present at link time.
#[cfg(test)]
pub use test_stubs::*;

#[cfg(test)]
mod test_stubs {
    use super::{GBytes, GError, GdkPixbuf, GdkPixbufSimpleAnim};
    use std::ffi::{c_char, c_int, c_void};

    /// # Safety
    /// Stub; returns NULL.
    pub unsafe extern "C" fn gdk_pixbuf_simple_anim_new(
        _: c_int,
        _: c_int,
        _: f32,
    ) -> *mut GdkPixbufSimpleAnim {
        std::ptr::null_mut()
    }

    /// # Safety
    /// Stub; no-op.
    pub unsafe extern "C" fn gdk_pixbuf_simple_anim_add_frame(
        _: *mut GdkPixbufSimpleAnim,
        _: *mut GdkPixbuf,
    ) {
    }

    /// # Safety
    /// Stub; no-op.
    pub unsafe extern "C" fn gdk_pixbuf_simple_anim_set_loop(
        _: *mut GdkPixbufSimpleAnim,
        _: c_int,
    ) {
    }

    /// # Safety
    /// Pixbuf creation is stubbed; always returns NULL in tests.
    pub unsafe extern "C" fn gdk_pixbuf_new_from_bytes(
        _: *mut GBytes,
        _: c_int,
        _: c_int,
        _: c_int,
        _: c_int,
        _: c_int,
        _: c_int,
    ) -> *mut GdkPixbuf {
        std::ptr::null_mut()
    }

    /// # Safety
    /// Stub; returns 0.
    pub unsafe extern "C" fn gdk_pixbuf_get_width(_: *mut GdkPixbuf) -> c_int {
        0
    }

    /// # Safety
    /// Stub; returns 0.
    pub unsafe extern "C" fn gdk_pixbuf_get_height(_: *mut GdkPixbuf) -> c_int {
        0
    }

    /// # Safety
    /// Stub; returns NULL.
    pub unsafe extern "C" fn g_bytes_new(_: *const c_void, _: usize) -> *mut GBytes {
        std::ptr::null_mut()
    }

    /// # Safety
    /// Stub; no-op.
    pub unsafe extern "C" fn g_bytes_unref(_: *mut GBytes) {}

    /// # Safety
    /// Stub; no-op.
    pub unsafe extern "C" fn g_object_unref(_: *mut c_void) {}

    /// # Safety
    /// Stub; no-op.
    pub unsafe extern "C" fn g_set_error_literal(
        _: *mut *mut GError,
        _: u32,
        _: c_int,
        _: *const c_char,
    ) {
    }

    /// # Safety
    /// Stub; returns a fixed nonzero quark.
    pub unsafe extern "C" fn g_quark_from_static_string(_: *const c_char) -> u32 {
        1
    }
}
