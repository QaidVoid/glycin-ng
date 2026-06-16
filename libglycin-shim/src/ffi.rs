//! Subset of GLib / GObject / GIO FFI this shim needs.
//!
//! Symbols are resolved by the host process at `dlopen` time (Arch's
//! `libgdk_pixbuf-2.0.so.0` already links these). Test builds use
//! the stubs in [`test_stubs`] so the test binary does not try to
//! load `libgobject-2.0` etc.

#![allow(non_camel_case_types)]

use std::ffi::{c_char, c_int, c_uint, c_void};

pub type GType = usize;
pub type GQuark = u32;
pub type gboolean = c_int;
pub type GDestroyNotify = Option<unsafe extern "C" fn(data: *mut c_void)>;
pub type GStrv = *mut *mut c_char;
pub type gpointer = *mut c_void;

/// `GAsyncReadyCallback`: invoked when an async operation completes.
pub type GAsyncReadyCallback =
    Option<unsafe extern "C" fn(source: *mut GObject, res: *mut GAsyncResult, data: gpointer)>;

/// `GTaskThreadFunc`: runs the blocking body of a `GTask` on a pool
/// thread.
pub type GTaskThreadFunc = Option<
    unsafe extern "C" fn(
        task: *mut GTask,
        source: *mut GObject,
        task_data: gpointer,
        cancellable: *mut GCancellable,
    ),
>;

/// `GBoxedCopyFunc` for `g_boxed_type_register_static`.
pub type GBoxedCopyFunc = unsafe extern "C" fn(boxed: gpointer) -> gpointer;
/// `GBoxedFreeFunc` for `g_boxed_type_register_static`.
pub type GBoxedFreeFunc = unsafe extern "C" fn(boxed: gpointer);

#[repr(C)]
pub struct GTask {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GAsyncResult {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GCancellable {
    _private: [u8; 0],
}

/// Layout of `GTypeQuery`, filled by `g_type_query`.
#[repr(C)]
pub struct GTypeQuery {
    pub type_: GType,
    pub type_name: *const c_char,
    pub class_size: c_uint,
    pub instance_size: c_uint,
}

/// Layout of `GEnumValue` for `g_enum_register_static`.
#[repr(C)]
pub struct GEnumValue {
    pub value: c_int,
    pub value_name: *const c_char,
    pub value_nick: *const c_char,
}

/// Layout of `GFlagsValue` for `g_flags_register_static`.
#[repr(C)]
pub struct GFlagsValue {
    pub value: c_uint,
    pub value_name: *const c_char,
    pub value_nick: *const c_char,
}

// SAFETY: the value tables are built from `'static` data and GLib
// only reads them, so sharing the pointers across threads is sound.
unsafe impl Sync for GEnumValue {}
unsafe impl Sync for GFlagsValue {}

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
    pub fn g_strfreev(str_array: GStrv);
    pub fn g_strdup(str: *const c_char) -> *mut c_char;
    pub fn g_free(ptr: *mut c_void);
    pub fn g_malloc0(n_bytes: usize) -> *mut c_void;
    pub fn g_malloc(n_bytes: usize) -> *mut c_void;

    pub fn g_task_new(
        source_object: gpointer,
        cancellable: *mut GCancellable,
        callback: GAsyncReadyCallback,
        callback_data: gpointer,
    ) -> *mut GTask;
    pub fn g_task_set_task_data(
        task: *mut GTask,
        task_data: gpointer,
        task_data_destroy: GDestroyNotify,
    );
    pub fn g_task_get_task_data(task: *mut GTask) -> gpointer;
    pub fn g_task_return_pointer(
        task: *mut GTask,
        result: gpointer,
        result_destroy: GDestroyNotify,
    );
    pub fn g_task_return_error(task: *mut GTask, error: *mut GError);
    pub fn g_task_propagate_pointer(task: *mut GTask, error: *mut *mut GError) -> gpointer;
    pub fn g_task_run_in_thread(task: *mut GTask, task_func: GTaskThreadFunc);

    pub fn g_type_query(type_: GType, query: *mut GTypeQuery);
    pub fn g_type_register_static_simple(
        parent_type: GType,
        type_name: *const c_char,
        class_size: c_uint,
        class_init: gpointer,
        instance_size: c_uint,
        instance_init: gpointer,
        flags: c_uint,
    ) -> GType;
    pub fn g_enum_register_static(name: *const c_char, const_static_values: *const GEnumValue)
    -> GType;
    pub fn g_flags_register_static(
        name: *const c_char,
        const_static_values: *const GFlagsValue,
    ) -> GType;
    pub fn g_boxed_type_register_static(
        name: *const c_char,
        boxed_copy: GBoxedCopyFunc,
        boxed_free: GBoxedFreeFunc,
    ) -> GType;
}

#[cfg(test)]
pub use test_stubs::*;

#[cfg(test)]
#[allow(dead_code)]
mod test_stubs {
    use super::{
        GAsyncReadyCallback, GBoxedCopyFunc, GBoxedFreeFunc, GBytes, GCancellable, GDestroyNotify,
        GEnumValue, GError, GFile, GFlagsValue, GInputStream, GObject, GQuark, GStrv, GTask,
        GTaskThreadFunc, GType, GTypeQuery, gpointer,
    };
    use std::ffi::{c_char, c_int, c_uint, c_void};

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
    pub unsafe extern "C" fn g_strfreev(_: GStrv) {}
    pub unsafe extern "C" fn g_strdup(_: *const c_char) -> *mut c_char {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn g_free(_: *mut c_void) {}
    pub unsafe extern "C" fn g_malloc0(_: usize) -> *mut c_void {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn g_malloc(_: usize) -> *mut c_void {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn g_task_new(
        _: gpointer,
        _: *mut GCancellable,
        _: GAsyncReadyCallback,
        _: gpointer,
    ) -> *mut GTask {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn g_task_set_task_data(_: *mut GTask, _: gpointer, _: GDestroyNotify) {}
    pub unsafe extern "C" fn g_task_get_task_data(_: *mut GTask) -> gpointer {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn g_task_return_pointer(_: *mut GTask, _: gpointer, _: GDestroyNotify) {}
    pub unsafe extern "C" fn g_task_return_error(_: *mut GTask, _: *mut GError) {}
    pub unsafe extern "C" fn g_task_propagate_pointer(
        _: *mut GTask,
        _: *mut *mut GError,
    ) -> gpointer {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn g_task_run_in_thread(_: *mut GTask, _: GTaskThreadFunc) {}
    pub unsafe extern "C" fn g_type_query(_: GType, _: *mut GTypeQuery) {}
    pub unsafe extern "C" fn g_type_register_static_simple(
        _: GType,
        _: *const c_char,
        _: c_uint,
        _: gpointer,
        _: c_uint,
        _: gpointer,
        _: c_uint,
    ) -> GType {
        0
    }
    pub unsafe extern "C" fn g_enum_register_static(
        _: *const c_char,
        _: *const GEnumValue,
    ) -> GType {
        0
    }
    pub unsafe extern "C" fn g_flags_register_static(
        _: *const c_char,
        _: *const GFlagsValue,
    ) -> GType {
        0
    }
    pub unsafe extern "C" fn g_boxed_type_register_static(
        _: *const c_char,
        _: GBoxedCopyFunc,
        _: GBoxedFreeFunc,
    ) -> GType {
        0
    }
}
