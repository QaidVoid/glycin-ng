//! GType registration for `RsvgHandle`, `RsvgError`, and
//! `RsvgHandleFlags`, plus the property plumbing.
//!
//! Unlike the base-GObject trick in `libglycin-shim`, `RsvgHandle`
//! must be a real registered subtype: its instance and class struct
//! sizes are public ABI (`_abi_padding` in rsvg.h) and it carries 11
//! properties. The instance is laid out as
//! `GObject parent; gpointer _abi_padding[16]`, and the Rust state
//! pointer lives in the first padding slot, written by
//! `instance_init` and dropped by an overridden `finalize` that
//! chains up to GObject.

use std::ffi::{c_int, c_uint};
use std::ptr;
use std::sync::OnceLock;

use crate::ffi::{self, GEnumValue, GFlagsValue, GObject, GObjectClass, GParamSpec, GType, GValue};
use crate::handle;
use crate::state::HandleState;

/// Opaque instance type for the public `RsvgHandle`.
#[repr(C)]
pub struct RsvgHandle {
    _private: [u8; 0],
}

const PADDING_SLOTS_INSTANCE: usize = 16;
const PADDING_SLOTS_CLASS: usize = 15;

// Property ids, in installation order.
const PROP_DPI_X: c_uint = 1;
const PROP_DPI_Y: c_uint = 2;
const PROP_FLAGS: c_uint = 3;
const PROP_BASE_URI: c_uint = 4;
const PROP_WIDTH: c_uint = 5;
const PROP_HEIGHT: c_uint = 6;
const PROP_EM: c_uint = 7;
const PROP_EX: c_uint = 8;
const PROP_TITLE: c_uint = 9;
const PROP_DESC: c_uint = 10;
const PROP_METADATA: c_uint = 11;

static PARENT: OnceLock<ParentInfo> = OnceLock::new();

struct ParentInfo {
    instance_size: usize,
    finalize: Option<unsafe extern "C" fn(*mut GObject)>,
}

// SAFETY: written once during single-threaded type registration,
// read-only afterwards.
unsafe impl Sync for ParentInfo {}
unsafe impl Send for ParentInfo {}

fn parent_instance_size() -> usize {
    PARENT
        .get()
        .map(|p| p.instance_size)
        .expect("rsvg_handle_get_type not called yet")
}

/// Pointer to the state slot inside an instance (the first
/// `_abi_padding` entry, right after the `GObject` parent).
///
/// # Safety
///
/// `obj` must be a live `RsvgHandle` instance.
unsafe fn state_slot(obj: *mut GObject) -> *mut *mut HandleState {
    unsafe { obj.cast::<u8>().add(parent_instance_size()).cast() }
}

/// Borrow the Rust state of a handle. Returns `None` for NULL
/// pointers, GObjects that are not `RsvgHandle`s, or instances whose
/// init did not run, mirroring upstream's `RSVG_IS_HANDLE` guards.
///
/// # Safety
///
/// `handle` must be NULL or a live GObject instance.
pub unsafe fn state_of<'a>(handle: *mut RsvgHandle) -> Option<&'a mut HandleState> {
    if handle.is_null() || PARENT.get().is_none() {
        return None;
    }
    if unsafe { ffi::g_type_check_instance_is_a(handle.cast(), rsvg_handle_get_type()) } == 0 {
        return None;
    }
    let slot = unsafe { state_slot(handle.cast()) };
    unsafe { (*slot).as_mut() }
}

unsafe extern "C" fn instance_init(instance: ffi::gpointer, _class: ffi::gpointer) {
    let state = Box::into_raw(Box::new(HandleState::new()));
    unsafe { *state_slot(instance.cast()) = state };
}

unsafe extern "C" fn finalize(object: *mut GObject) {
    let slot = unsafe { state_slot(object) };
    let state = unsafe { *slot };
    if !state.is_null() {
        unsafe {
            *slot = ptr::null_mut();
            drop(Box::from_raw(state));
        }
    }
    if let Some(parent_finalize) = PARENT.get().and_then(|p| p.finalize) {
        unsafe { parent_finalize(object) };
    }
}

unsafe extern "C" fn set_property(
    object: *mut GObject,
    property_id: c_uint,
    value: *const GValue,
    _pspec: *mut GParamSpec,
) {
    let Some(state) = (unsafe { state_of(object.cast()) }) else {
        return;
    };
    match property_id {
        PROP_DPI_X => {
            let dpi = unsafe { ffi::g_value_get_double(value) };
            handle::set_dpi_values(state, Some(dpi), None);
        }
        PROP_DPI_Y => {
            let dpi = unsafe { ffi::g_value_get_double(value) };
            handle::set_dpi_values(state, None, Some(dpi));
        }
        PROP_FLAGS => {
            state.flags = unsafe { ffi::g_value_get_flags(value) };
        }
        PROP_BASE_URI => {
            // A NULL value is ignored rather than clearing, matching
            // upstream's property setter and the C entry point.
            let s = unsafe { ffi::g_value_get_string(value) };
            if !s.is_null() {
                state.base_uri = Some(unsafe { std::ffi::CStr::from_ptr(s) }.to_owned());
            }
        }
        _ => {}
    }
}

unsafe extern "C" fn get_property(
    object: *mut GObject,
    property_id: c_uint,
    value: *mut GValue,
    _pspec: *mut GParamSpec,
) {
    let Some(state) = (unsafe { state_of(object.cast()) }) else {
        return;
    };
    match property_id {
        PROP_DPI_X => unsafe { ffi::g_value_set_double(value, state.dpi_x) },
        PROP_DPI_Y => unsafe { ffi::g_value_set_double(value, state.dpi_y) },
        PROP_FLAGS => unsafe { ffi::g_value_set_flags(value, state.flags) },
        PROP_BASE_URI => {
            let s = state
                .base_uri
                .as_ref()
                .map(|s| s.as_ptr())
                .unwrap_or(ptr::null());
            unsafe { ffi::g_value_set_string(value, s) };
        }
        PROP_WIDTH => {
            let d = handle::natural_dimensions(state);
            unsafe { ffi::g_value_set_int(value, d.width) };
        }
        PROP_HEIGHT => {
            let d = handle::natural_dimensions(state);
            unsafe { ffi::g_value_set_int(value, d.height) };
        }
        PROP_EM => {
            let d = handle::natural_dimensions(state);
            unsafe { ffi::g_value_set_double(value, d.em) };
        }
        PROP_EX => {
            let d = handle::natural_dimensions(state);
            unsafe { ffi::g_value_set_double(value, d.ex) };
        }
        PROP_TITLE | PROP_DESC | PROP_METADATA => unsafe {
            ffi::g_value_set_string(value, ptr::null());
        },
        _ => {}
    }
}

unsafe extern "C" fn class_init(class: ffi::gpointer, _class_data: ffi::gpointer) {
    let oclass = class.cast::<GObjectClass>();

    let parent_class = unsafe { ffi::g_type_class_peek_parent(class) }.cast::<GObjectClass>();
    let parent_finalize = if parent_class.is_null() {
        None
    } else {
        unsafe { (*parent_class).finalize }
    };
    let mut query = ffi::GTypeQuery {
        type_: 0,
        type_name: ptr::null(),
        class_size: 0,
        instance_size: 0,
    };
    unsafe { ffi::g_type_query(ffi::g_object_get_type(), &mut query) };
    let _ = PARENT.set(ParentInfo {
        instance_size: query.instance_size as usize,
        finalize: parent_finalize,
    });

    unsafe {
        (*oclass).set_property = Some(set_property);
        (*oclass).get_property = Some(get_property);
        (*oclass).finalize = Some(finalize);
    }

    let rw_construct =
        ffi::G_PARAM_READWRITE | ffi::G_PARAM_CONSTRUCT | ffi::G_PARAM_STATIC_STRINGS;
    let read_only = ffi::G_PARAM_READABLE | ffi::G_PARAM_STATIC_STRINGS;
    let deprecated = read_only | ffi::G_PARAM_DEPRECATED;

    unsafe {
        ffi::g_object_class_install_property(
            oclass,
            PROP_DPI_X,
            ffi::g_param_spec_double(
                c"dpi-x".as_ptr(),
                c"dpi-x".as_ptr(),
                c"Horizontal resolution in dots per inch".as_ptr(),
                f64::MIN,
                f64::MAX,
                0.0,
                rw_construct,
            ),
        );
        ffi::g_object_class_install_property(
            oclass,
            PROP_DPI_Y,
            ffi::g_param_spec_double(
                c"dpi-y".as_ptr(),
                c"dpi-y".as_ptr(),
                c"Vertical resolution in dots per inch".as_ptr(),
                f64::MIN,
                f64::MAX,
                0.0,
                rw_construct,
            ),
        );
        ffi::g_object_class_install_property(
            oclass,
            PROP_FLAGS,
            ffi::g_param_spec_flags(
                c"flags".as_ptr(),
                c"flags".as_ptr(),
                c"Loading flags".as_ptr(),
                rsvg_handle_flags_get_type(),
                0,
                ffi::G_PARAM_READWRITE | ffi::G_PARAM_CONSTRUCT_ONLY | ffi::G_PARAM_STATIC_STRINGS,
            ),
        );
        ffi::g_object_class_install_property(
            oclass,
            PROP_BASE_URI,
            ffi::g_param_spec_string(
                c"base-uri".as_ptr(),
                c"base-uri".as_ptr(),
                c"Base URI for resolving relative references".as_ptr(),
                ptr::null(),
                rw_construct,
            ),
        );
        ffi::g_object_class_install_property(
            oclass,
            PROP_WIDTH,
            ffi::g_param_spec_int(
                c"width".as_ptr(),
                c"width".as_ptr(),
                c"Image width".as_ptr(),
                c_int::MIN,
                c_int::MAX,
                0,
                read_only,
            ),
        );
        ffi::g_object_class_install_property(
            oclass,
            PROP_HEIGHT,
            ffi::g_param_spec_int(
                c"height".as_ptr(),
                c"height".as_ptr(),
                c"Image height".as_ptr(),
                c_int::MIN,
                c_int::MAX,
                0,
                read_only,
            ),
        );
        ffi::g_object_class_install_property(
            oclass,
            PROP_EM,
            ffi::g_param_spec_double(
                c"em".as_ptr(),
                c"em".as_ptr(),
                c"Exact width".as_ptr(),
                f64::MIN,
                f64::MAX,
                0.0,
                read_only,
            ),
        );
        ffi::g_object_class_install_property(
            oclass,
            PROP_EX,
            ffi::g_param_spec_double(
                c"ex".as_ptr(),
                c"ex".as_ptr(),
                c"Exact height".as_ptr(),
                f64::MIN,
                f64::MAX,
                0.0,
                read_only,
            ),
        );
        ffi::g_object_class_install_property(
            oclass,
            PROP_TITLE,
            ffi::g_param_spec_string(
                c"title".as_ptr(),
                c"title".as_ptr(),
                c"SVG's title".as_ptr(),
                ptr::null(),
                deprecated,
            ),
        );
        ffi::g_object_class_install_property(
            oclass,
            PROP_DESC,
            ffi::g_param_spec_string(
                c"desc".as_ptr(),
                c"desc".as_ptr(),
                c"SVG's description".as_ptr(),
                ptr::null(),
                deprecated,
            ),
        );
        ffi::g_object_class_install_property(
            oclass,
            PROP_METADATA,
            ffi::g_param_spec_string(
                c"metadata".as_ptr(),
                c"metadata".as_ptr(),
                c"SVG's metadata".as_ptr(),
                ptr::null(),
                deprecated,
            ),
        );
    }
}

/// Returns the `RsvgHandle` GType, registering it on first call
/// with the ABI-mandated instance and class sizes.
#[unsafe(no_mangle)]
pub extern "C" fn rsvg_handle_get_type() -> GType {
    static CELL: OnceLock<GType> = OnceLock::new();
    *CELL.get_or_init(|| {
        let parent = unsafe { ffi::g_object_get_type() };
        let mut query = ffi::GTypeQuery {
            type_: 0,
            type_name: ptr::null(),
            class_size: 0,
            instance_size: 0,
        };
        unsafe { ffi::g_type_query(parent, &mut query) };
        let ptr_size = size_of::<*mut ()>();
        let instance_size = query.instance_size as usize + PADDING_SLOTS_INSTANCE * ptr_size;
        let class_size = query.class_size as usize + PADDING_SLOTS_CLASS * ptr_size;
        unsafe {
            ffi::g_type_register_static_simple(
                parent,
                c"RsvgHandle".as_ptr(),
                class_size as c_uint,
                Some(class_init),
                instance_size as c_uint,
                Some(instance_init),
                0,
            )
        }
    })
}

static ERROR_VALUES: &[GEnumValue] = &[
    GEnumValue {
        value: 0,
        value_name: c"RSVG_ERROR_FAILED".as_ptr(),
        value_nick: c"failed".as_ptr(),
    },
    GEnumValue {
        value: 0,
        value_name: ptr::null(),
        value_nick: ptr::null(),
    },
];

static HANDLE_FLAGS_VALUES: &[GFlagsValue] = &[
    GFlagsValue {
        value: 0,
        value_name: c"RSVG_HANDLE_FLAGS_NONE".as_ptr(),
        value_nick: c"flags-none".as_ptr(),
    },
    GFlagsValue {
        value: 1,
        value_name: c"RSVG_HANDLE_FLAG_UNLIMITED".as_ptr(),
        value_nick: c"flag-unlimited".as_ptr(),
    },
    GFlagsValue {
        value: 2,
        value_name: c"RSVG_HANDLE_FLAG_KEEP_IMAGE_DATA".as_ptr(),
        value_nick: c"flag-keep-image-data".as_ptr(),
    },
    GFlagsValue {
        value: 0,
        value_name: ptr::null(),
        value_nick: ptr::null(),
    },
];

/// Returns the `RsvgError` enum GType.
#[unsafe(no_mangle)]
pub extern "C" fn rsvg_error_get_type() -> GType {
    static CELL: OnceLock<GType> = OnceLock::new();
    *CELL.get_or_init(|| unsafe {
        ffi::g_enum_register_static(c"RsvgError".as_ptr(), ERROR_VALUES.as_ptr())
    })
}

/// Returns the `RsvgHandleFlags` flags GType.
#[unsafe(no_mangle)]
pub extern "C" fn rsvg_handle_flags_get_type() -> GType {
    static CELL: OnceLock<GType> = OnceLock::new();
    *CELL.get_or_init(|| unsafe {
        ffi::g_flags_register_static(c"RsvgHandleFlags".as_ptr(), HANDLE_FLAGS_VALUES.as_ptr())
    })
}

/// Returns the librsvg error domain quark.
#[unsafe(no_mangle)]
pub extern "C" fn rsvg_error_quark() -> ffi::GQuark {
    static CELL: OnceLock<ffi::GQuark> = OnceLock::new();
    *CELL.get_or_init(|| unsafe { ffi::g_quark_from_static_string(c"rsvg-error-quark".as_ptr()) })
}

/// Set a `GError` in the librsvg error domain.
pub fn set_gerror(error: *mut *mut ffi::GError, message: &str) {
    if error.is_null() {
        return;
    }
    let msg = std::ffi::CString::new(message).unwrap_or_else(|_| c"rendering error".to_owned());
    unsafe { ffi::g_set_error_literal(error, rsvg_error_quark(), 0, msg.as_ptr()) };
}

/// Set a `GError` from the engine's thread-local last error.
pub fn set_gerror_from_engine(error: *mut *mut ffi::GError) {
    let msg = unsafe { crate::ngapi::glycin_ng_last_error() };
    if msg.is_null() {
        set_gerror(error, "unknown error");
    } else {
        unsafe { ffi::g_set_error_literal(error, rsvg_error_quark(), 0, msg) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::{CStr, c_char};

    fn name(p: *const c_char) -> &'static str {
        unsafe { CStr::from_ptr(p) }.to_str().unwrap()
    }

    #[test]
    fn flags_table_matches_librsvg() {
        assert_eq!(HANDLE_FLAGS_VALUES.len(), 4);
        assert_eq!(HANDLE_FLAGS_VALUES[0].value, 0);
        assert_eq!(
            name(HANDLE_FLAGS_VALUES[0].value_name),
            "RSVG_HANDLE_FLAGS_NONE"
        );
        assert_eq!(name(HANDLE_FLAGS_VALUES[0].value_nick), "flags-none");
        assert_eq!(HANDLE_FLAGS_VALUES[1].value, 1);
        assert_eq!(name(HANDLE_FLAGS_VALUES[1].value_nick), "flag-unlimited");
        assert_eq!(HANDLE_FLAGS_VALUES[2].value, 2);
        assert_eq!(
            name(HANDLE_FLAGS_VALUES[2].value_name),
            "RSVG_HANDLE_FLAG_KEEP_IMAGE_DATA"
        );
        assert!(HANDLE_FLAGS_VALUES[3].value_name.is_null());
    }

    #[test]
    fn error_table_matches_librsvg() {
        assert_eq!(ERROR_VALUES.len(), 2);
        assert_eq!(ERROR_VALUES[0].value, 0);
        assert_eq!(name(ERROR_VALUES[0].value_name), "RSVG_ERROR_FAILED");
        assert_eq!(name(ERROR_VALUES[0].value_nick), "failed");
        assert!(ERROR_VALUES[1].value_name.is_null());
    }
}
