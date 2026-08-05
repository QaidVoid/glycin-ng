//! Subset of GLib / GObject / GIO FFI this shim needs.
//!
//! The shim links these libraries directly (see `build.rs`), the
//! same NEEDED set real librsvg carries, so consumers that dlopen
//! the library in a process without them still work. Test builds
//! use the stubs in [`test_stubs`] so the test binary does not link
//! `libgobject-2.0` etc.

#![allow(non_camel_case_types)]

use std::ffi::{c_char, c_double, c_int, c_uint, c_void};

pub type GType = usize;
pub type GQuark = u32;
pub type gboolean = c_int;
pub type gpointer = *mut c_void;
pub type GDestroyNotify = Option<unsafe extern "C" fn(data: gpointer)>;

/// `GClassInitFunc` for `g_type_register_static_simple`.
pub type GClassInitFunc = Option<unsafe extern "C" fn(class: gpointer, class_data: gpointer)>;
/// `GInstanceInitFunc` for `g_type_register_static_simple`.
pub type GInstanceInitFunc = Option<unsafe extern "C" fn(instance: gpointer, class: gpointer)>;

#[repr(C)]
pub struct GObject {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GParamSpec {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GFile {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GInputStream {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GCancellable {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GError {
    pub domain: GQuark,
    pub code: c_int,
    pub message: *mut c_char,
}

/// Layout of `GValue` (public GObject ABI: a `GType` plus two
/// pointer-sized data words).
#[repr(C)]
pub struct GValue {
    pub g_type: GType,
    pub data: [u64; 2],
}

/// Layout of `GTypeQuery`, filled by `g_type_query`.
#[repr(C)]
pub struct GTypeQuery {
    pub type_: GType,
    pub type_name: *const c_char,
    pub class_size: c_uint,
    pub instance_size: c_uint,
}

/// Layout of `GObjectClass` (public GObject ABI from `gobject.h`).
/// The shim writes `set_property`, `get_property`, and `finalize`
/// during `class_init` and reads the parent's `finalize` to chain up.
#[repr(C)]
pub struct GObjectClass {
    pub g_type_class: GType,
    pub construct_properties: gpointer,
    pub constructor: gpointer,
    pub set_property: Option<
        unsafe extern "C" fn(
            object: *mut GObject,
            property_id: c_uint,
            value: *const GValue,
            pspec: *mut GParamSpec,
        ),
    >,
    pub get_property: Option<
        unsafe extern "C" fn(
            object: *mut GObject,
            property_id: c_uint,
            value: *mut GValue,
            pspec: *mut GParamSpec,
        ),
    >,
    pub dispose: gpointer,
    pub finalize: Option<unsafe extern "C" fn(object: *mut GObject)>,
    pub dispatch_properties_changed: gpointer,
    pub notify: gpointer,
    pub constructed: gpointer,
    pub flags: usize,
    pub n_construct_properties: usize,
    pub pspecs: gpointer,
    pub n_pspecs: usize,
    pub pdummy: [gpointer; 3],
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

// GParamFlags subset used by the property specs.
pub const G_PARAM_READABLE: c_uint = 1 << 0;
pub const G_PARAM_READWRITE: c_uint = (1 << 0) | (1 << 1);
pub const G_PARAM_CONSTRUCT: c_uint = 1 << 2;
pub const G_PARAM_CONSTRUCT_ONLY: c_uint = 1 << 3;
pub const G_PARAM_STATIC_STRINGS: c_uint = (1 << 5) | (1 << 6) | (1 << 7);
pub const G_PARAM_DEPRECATED: c_uint = 1 << 31;

#[cfg(not(test))]
#[allow(dead_code)]
unsafe extern "C" {
    pub fn g_object_get_type() -> GType;
    pub fn g_object_new(object_type: GType, first_property_name: *const c_char) -> *mut GObject;
    pub fn g_object_ref(object: gpointer) -> gpointer;
    pub fn g_object_unref(object: gpointer);

    pub fn g_type_query(type_: GType, query: *mut GTypeQuery);
    pub fn g_type_register_static_simple(
        parent_type: GType,
        type_name: *const c_char,
        class_size: c_uint,
        class_init: GClassInitFunc,
        instance_size: c_uint,
        instance_init: GInstanceInitFunc,
        flags: c_uint,
    ) -> GType;
    pub fn g_type_class_peek_parent(g_class: gpointer) -> gpointer;
    pub fn g_enum_register_static(
        name: *const c_char,
        const_static_values: *const GEnumValue,
    ) -> GType;
    pub fn g_flags_register_static(
        name: *const c_char,
        const_static_values: *const GFlagsValue,
    ) -> GType;

    pub fn g_object_class_install_property(
        oclass: *mut GObjectClass,
        property_id: c_uint,
        pspec: *mut GParamSpec,
    );
    pub fn g_param_spec_double(
        name: *const c_char,
        nick: *const c_char,
        blurb: *const c_char,
        minimum: c_double,
        maximum: c_double,
        default_value: c_double,
        flags: c_uint,
    ) -> *mut GParamSpec;
    pub fn g_param_spec_int(
        name: *const c_char,
        nick: *const c_char,
        blurb: *const c_char,
        minimum: c_int,
        maximum: c_int,
        default_value: c_int,
        flags: c_uint,
    ) -> *mut GParamSpec;
    pub fn g_param_spec_string(
        name: *const c_char,
        nick: *const c_char,
        blurb: *const c_char,
        default_value: *const c_char,
        flags: c_uint,
    ) -> *mut GParamSpec;
    pub fn g_param_spec_flags(
        name: *const c_char,
        nick: *const c_char,
        blurb: *const c_char,
        flags_type: GType,
        default_value: c_uint,
        flags: c_uint,
    ) -> *mut GParamSpec;

    pub fn g_value_get_double(value: *const GValue) -> c_double;
    pub fn g_value_set_double(value: *mut GValue, v: c_double);
    pub fn g_value_set_int(value: *mut GValue, v: c_int);
    pub fn g_value_get_string(value: *const GValue) -> *const c_char;
    pub fn g_value_set_string(value: *mut GValue, v: *const c_char);
    pub fn g_value_get_flags(value: *const GValue) -> c_uint;
    pub fn g_value_set_flags(value: *mut GValue, v: c_uint);

    pub fn g_quark_from_static_string(string: *const c_char) -> GQuark;
    pub fn g_set_error_literal(
        err: *mut *mut GError,
        domain: GQuark,
        code: c_int,
        message: *const c_char,
    );
    pub fn g_propagate_error(dest: *mut *mut GError, src: *mut GError);
    pub fn g_error_free(error: *mut GError);

    pub fn g_strdup(str: *const c_char) -> *mut c_char;
    pub fn g_free(ptr: gpointer);
    pub fn g_malloc(n_bytes: usize) -> gpointer;

    pub fn g_file_new_for_commandline_arg(arg: *const c_char) -> *mut GFile;
    pub fn g_file_get_path(file: *mut GFile) -> *mut c_char;
    pub fn g_file_get_uri(file: *mut GFile) -> *mut c_char;
    pub fn g_file_read(
        file: *mut GFile,
        cancellable: *mut GCancellable,
        error: *mut *mut GError,
    ) -> *mut GInputStream;

    pub fn g_input_stream_read(
        stream: *mut GInputStream,
        buffer: gpointer,
        count: usize,
        cancellable: *mut GCancellable,
        error: *mut *mut GError,
    ) -> isize;
}

#[cfg(test)]
pub use test_stubs::*;

/// No-op / null-returning stand-ins so unit tests run without
/// linking GLib. Tests exercising real GObject behavior live in the
/// C harness, not here.
#[cfg(test)]
#[allow(dead_code)]
mod test_stubs {
    use super::*;

    pub unsafe extern "C" fn g_object_get_type() -> GType {
        80
    }
    pub unsafe extern "C" fn g_object_new(_: GType, _: *const c_char) -> *mut GObject {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn g_object_ref(p: gpointer) -> gpointer {
        p
    }
    pub unsafe extern "C" fn g_object_unref(_: gpointer) {}
    pub unsafe extern "C" fn g_type_query(_: GType, q: *mut GTypeQuery) {
        unsafe {
            (*q).class_size = 136;
            (*q).instance_size = 24;
        }
    }
    pub unsafe extern "C" fn g_type_register_static_simple(
        _: GType,
        _: *const c_char,
        _: c_uint,
        _: GClassInitFunc,
        _: c_uint,
        _: GInstanceInitFunc,
        _: c_uint,
    ) -> GType {
        1
    }
    pub unsafe extern "C" fn g_type_class_peek_parent(_: gpointer) -> gpointer {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn g_enum_register_static(
        _: *const c_char,
        _: *const GEnumValue,
    ) -> GType {
        2
    }
    pub unsafe extern "C" fn g_flags_register_static(
        _: *const c_char,
        _: *const GFlagsValue,
    ) -> GType {
        3
    }
    pub unsafe extern "C" fn g_object_class_install_property(
        _: *mut GObjectClass,
        _: c_uint,
        _: *mut GParamSpec,
    ) {
    }
    pub unsafe extern "C" fn g_param_spec_double(
        _: *const c_char,
        _: *const c_char,
        _: *const c_char,
        _: c_double,
        _: c_double,
        _: c_double,
        _: c_uint,
    ) -> *mut GParamSpec {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn g_param_spec_int(
        _: *const c_char,
        _: *const c_char,
        _: *const c_char,
        _: c_int,
        _: c_int,
        _: c_int,
        _: c_uint,
    ) -> *mut GParamSpec {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn g_param_spec_string(
        _: *const c_char,
        _: *const c_char,
        _: *const c_char,
        _: *const c_char,
        _: c_uint,
    ) -> *mut GParamSpec {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn g_param_spec_flags(
        _: *const c_char,
        _: *const c_char,
        _: *const c_char,
        _: GType,
        _: c_uint,
        _: c_uint,
    ) -> *mut GParamSpec {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn g_value_get_double(_: *const GValue) -> c_double {
        0.0
    }
    pub unsafe extern "C" fn g_value_set_double(_: *mut GValue, _: c_double) {}
    pub unsafe extern "C" fn g_value_set_int(_: *mut GValue, _: c_int) {}
    pub unsafe extern "C" fn g_value_get_string(_: *const GValue) -> *const c_char {
        std::ptr::null()
    }
    pub unsafe extern "C" fn g_value_set_string(_: *mut GValue, _: *const c_char) {}
    pub unsafe extern "C" fn g_value_get_flags(_: *const GValue) -> c_uint {
        0
    }
    pub unsafe extern "C" fn g_value_set_flags(_: *mut GValue, _: c_uint) {}
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
    pub unsafe extern "C" fn g_propagate_error(_: *mut *mut GError, _: *mut GError) {}
    pub unsafe extern "C" fn g_error_free(_: *mut GError) {}
    pub unsafe extern "C" fn g_strdup(_: *const c_char) -> *mut c_char {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn g_free(_: gpointer) {}
    pub unsafe extern "C" fn g_malloc(_: usize) -> gpointer {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn g_file_new_for_commandline_arg(_: *const c_char) -> *mut GFile {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn g_file_get_path(_: *mut GFile) -> *mut c_char {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn g_file_get_uri(_: *mut GFile) -> *mut c_char {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn g_file_read(
        _: *mut GFile,
        _: *mut GCancellable,
        _: *mut *mut GError,
    ) -> *mut GInputStream {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn g_input_stream_read(
        _: *mut GInputStream,
        _: gpointer,
        _: usize,
        _: *mut GCancellable,
        _: *mut *mut GError,
    ) -> isize {
        0
    }
}
