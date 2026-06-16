//! `GType` registration for the public glycin enums, flags, boxed,
//! and object types.
//!
//! Each `gly_*_get_type` registers its GLib type once and caches the
//! result. The enum and flags value tables are transcribed from
//! `glycin.h` so the registered types carry the same values, names,
//! and nicks an app compiled against upstream expects. The object
//! types are registered as trivial `GObject` subclasses sized from
//! the parent through `g_type_query`, matching the base-`GObject`
//! handles this shim already hands out.

use std::ffi::{CStr, c_char};
use std::ptr;
use std::sync::OnceLock;

use crate::cicp::{GlyCicp, gly_cicp_copy, gly_cicp_free};
use crate::ffi::{self, GEnumValue, GFlagsValue, GType, GTypeQuery, gpointer};

macro_rules! enum_table {
    ($name:ident; $($val:expr => $sym:literal / $nick:literal),+ $(,)?) => {
        static $name: &[GEnumValue] = &[
            $(GEnumValue {
                value: $val,
                value_name: concat!($sym, "\0").as_ptr() as *const c_char,
                value_nick: concat!($nick, "\0").as_ptr() as *const c_char,
            }),+,
            GEnumValue { value: 0, value_name: ptr::null(), value_nick: ptr::null() },
        ];
    };
}

macro_rules! flags_table {
    ($name:ident; $($val:expr => $sym:literal / $nick:literal),+ $(,)?) => {
        static $name: &[GFlagsValue] = &[
            $(GFlagsValue {
                value: $val,
                value_name: concat!($sym, "\0").as_ptr() as *const c_char,
                value_nick: concat!($nick, "\0").as_ptr() as *const c_char,
            }),+,
            GFlagsValue { value: 0, value_name: ptr::null(), value_nick: ptr::null() },
        ];
    };
}

enum_table!(MEMORY_FORMAT_VALUES;
    0 => "B8g8r8a8Premultiplied" / "b8g8r8a8-premultiplied",
    1 => "A8r8g8b8Premultiplied" / "a8r8g8b8-premultiplied",
    2 => "R8g8b8a8Premultiplied" / "r8g8b8a8-premultiplied",
    3 => "B8g8r8a8" / "b8g8r8a8",
    4 => "A8r8g8b8" / "a8r8g8b8",
    5 => "R8g8b8a8" / "r8g8b8a8",
    6 => "A8b8g8r8" / "a8b8g8r8",
    7 => "R8g8b8" / "r8g8b8",
    8 => "B8g8r8" / "b8g8r8",
    9 => "R16g16b16" / "r16g16b16",
    10 => "R16g16b16a16Premultiplied" / "r16g16b16a16-premultiplied",
    11 => "R16g16b16a16" / "r16g16b16a16",
    12 => "R16g16b16Float" / "r16g16b16-float",
    13 => "R16g16b16a16Float" / "r16g16b16a16-float",
    14 => "R32g32b32Float" / "r32g32b32-float",
    15 => "R32g32b32a32FloatPremultiplied" / "r32g32b32a32-float-premultiplied",
    16 => "R32g32b32a32Float" / "r32g32b32a32-float",
    17 => "G8a8Premultiplied" / "g8a8-premultiplied",
    18 => "G8a8" / "g8a8",
    19 => "G8" / "g8",
    20 => "G16a16Premultiplied" / "g16a16-premultiplied",
    21 => "G16a16" / "g16a16",
    22 => "G16" / "g16",
);

enum_table!(SANDBOX_SELECTOR_VALUES;
    0 => "Auto" / "auto",
    1 => "Bwrap" / "bwrap",
    2 => "FlatpakSpawn" / "flatpak-spawn",
    3 => "NotSandboxed" / "not-sandboxed",
);

enum_table!(LOADER_ERROR_VALUES;
    0 => "Failed" / "failed",
    1 => "UnknownImageFormat" / "unknown-image-format",
    2 => "NoMoreFrames" / "no-more-frames",
);

flags_table!(MEMORY_FORMAT_SELECTION_VALUES;
    1 << 0 => "B8g8r8a8Premultiplied" / "b8g8r8a8-premultiplied",
    1 << 1 => "A8r8g8b8Premultiplied" / "a8r8g8b8-premultiplied",
    1 << 2 => "R8g8b8a8Premultiplied" / "r8g8b8a8-premultiplied",
    1 << 3 => "B8g8r8a8" / "b8g8r8a8",
    1 << 4 => "A8r8g8b8" / "a8r8g8b8",
    1 << 5 => "R8g8b8a8" / "r8g8b8a8",
    1 << 6 => "A8b8g8r8" / "a8b8g8r8",
    1 << 7 => "R8g8b8" / "r8g8b8",
    1 << 8 => "B8g8r8" / "b8g8r8",
    1 << 9 => "R16g16b16" / "r16g16b16",
    1 << 10 => "R16g16b16a16Premultiplied" / "r16g16b16a16-premultiplied",
    1 << 11 => "R16g16b16a16" / "r16g16b16a16",
    1 << 12 => "R16g16b16Float" / "r16g16b16-float",
    1 << 13 => "R16g16b16a16Float" / "r16g16b16a16-float",
    1 << 14 => "R32g32b32Float" / "r32g32b32-float",
    1 << 15 => "R32g32b32a32FloatPremultiplied" / "r32g32b32a32-float-premultiplied",
    1 << 16 => "R32g32b32a32Float" / "r32g32b32a32-float",
    1 << 17 => "G8a8Premultiplied" / "g8a8-premultiplied",
    1 << 18 => "G8a8" / "g8a8",
    1 << 19 => "G8" / "g8",
    1 << 20 => "G16a16Premultiplied" / "g16a16-premultiplied",
    1 << 21 => "G16a16" / "g16a16",
    1 << 22 => "G16" / "g16",
);

fn register_enum(name: &CStr, values: &'static [GEnumValue]) -> GType {
    unsafe { ffi::g_enum_register_static(name.as_ptr(), values.as_ptr()) }
}

fn register_flags(name: &CStr, values: &'static [GFlagsValue]) -> GType {
    unsafe { ffi::g_flags_register_static(name.as_ptr(), values.as_ptr()) }
}

fn register_gobject(name: &CStr) -> GType {
    let parent = unsafe { ffi::g_object_get_type() };
    let mut query = GTypeQuery {
        type_: 0,
        type_name: ptr::null(),
        class_size: 0,
        instance_size: 0,
    };
    unsafe { ffi::g_type_query(parent, &mut query) };
    unsafe {
        ffi::g_type_register_static_simple(
            parent,
            name.as_ptr(),
            query.class_size,
            ptr::null_mut(),
            query.instance_size,
            ptr::null_mut(),
            0,
        )
    }
}

unsafe extern "C" fn cicp_boxed_copy(boxed: gpointer) -> gpointer {
    unsafe { gly_cicp_copy(boxed as *mut GlyCicp) as gpointer }
}

unsafe extern "C" fn cicp_boxed_free(boxed: gpointer) {
    unsafe { gly_cicp_free(boxed as *mut GlyCicp) };
}

macro_rules! get_type_fn {
    ($fn_name:ident, $register:expr) => {
        /// # Safety
        /// Always safe. Registers the type on first call and caches it.
        #[unsafe(no_mangle)]
        pub extern "C" fn $fn_name() -> GType {
            static CELL: OnceLock<GType> = OnceLock::new();
            *CELL.get_or_init(|| $register)
        }
    };
}

get_type_fn!(gly_memory_format_get_type, register_enum(c"GlyMemoryFormat", MEMORY_FORMAT_VALUES));
get_type_fn!(gly_sandbox_selector_get_type, register_enum(c"GlySandboxSelector", SANDBOX_SELECTOR_VALUES));
get_type_fn!(gly_loader_error_get_type, register_enum(c"GlyLoaderError", LOADER_ERROR_VALUES));
get_type_fn!(gly_memory_format_selection_get_type, register_flags(c"GlyMemoryFormatSelection", MEMORY_FORMAT_SELECTION_VALUES));
get_type_fn!(gly_loader_get_type, register_gobject(c"GlyLoader"));
get_type_fn!(gly_image_get_type, register_gobject(c"GlyImage"));
get_type_fn!(gly_frame_get_type, register_gobject(c"GlyFrame"));
get_type_fn!(gly_frame_request_get_type, register_gobject(c"GlyFrameRequest"));
get_type_fn!(gly_creator_get_type, register_gobject(c"GlyCreator"));
get_type_fn!(gly_encoded_image_get_type, register_gobject(c"GlyEncodedImage"));
get_type_fn!(gly_new_frame_get_type, register_gobject(c"GlyNewFrame"));

/// # Safety
/// Always safe. Registers the boxed type on first call and caches it.
#[unsafe(no_mangle)]
pub extern "C" fn gly_cicp_get_type() -> GType {
    static CELL: OnceLock<GType> = OnceLock::new();
    *CELL.get_or_init(|| unsafe {
        ffi::g_boxed_type_register_static(c"GlyCicp".as_ptr(), cicp_boxed_copy, cicp_boxed_free)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drop the trailing zeroed terminator GLib requires.
    fn enum_payload(values: &'static [GEnumValue]) -> &'static [GEnumValue] {
        &values[..values.len() - 1]
    }

    #[test]
    fn enum_values_are_sequential_from_zero() {
        for table in [
            MEMORY_FORMAT_VALUES,
            SANDBOX_SELECTOR_VALUES,
            LOADER_ERROR_VALUES,
        ] {
            for (i, entry) in enum_payload(table).iter().enumerate() {
                assert_eq!(entry.value, i as i32);
                assert!(!entry.value_name.is_null());
                assert!(!entry.value_nick.is_null());
            }
        }
    }

    #[test]
    fn enum_tables_have_expected_lengths() {
        assert_eq!(MEMORY_FORMAT_VALUES.len(), 23 + 1);
        assert_eq!(SANDBOX_SELECTOR_VALUES.len(), 4 + 1);
        assert_eq!(LOADER_ERROR_VALUES.len(), 3 + 1);
    }

    #[test]
    fn flags_values_are_distinct_single_bits() {
        let payload = &MEMORY_FORMAT_SELECTION_VALUES[..MEMORY_FORMAT_SELECTION_VALUES.len() - 1];
        assert_eq!(payload.len(), 23);
        for (i, entry) in payload.iter().enumerate() {
            assert_eq!(entry.value, 1u32 << i);
            assert!(!entry.value_name.is_null());
        }
    }

    #[test]
    fn value_names_and_nicks_match_glib_derive() {
        // glib's Enum and flags derives register value_name as the
        // UpperCamelCase variant identifier and value_nick as its
        // kebab-case form. Lock in a few entries so the tables stay
        // aligned with upstream's `#[derive(glib::Enum)]` output
        // rather than the C macro names.
        let name = |p: *const std::ffi::c_char| unsafe {
            std::ffi::CStr::from_ptr(p).to_str().unwrap().to_owned()
        };
        assert_eq!(name(LOADER_ERROR_VALUES[2].value_name), "NoMoreFrames");
        assert_eq!(name(LOADER_ERROR_VALUES[2].value_nick), "no-more-frames");
        assert_eq!(name(SANDBOX_SELECTOR_VALUES[2].value_name), "FlatpakSpawn");
        assert_eq!(name(SANDBOX_SELECTOR_VALUES[2].value_nick), "flatpak-spawn");
        assert_eq!(name(MEMORY_FORMAT_VALUES[0].value_name), "B8g8r8a8Premultiplied");
        assert_eq!(name(MEMORY_FORMAT_VALUES[0].value_nick), "b8g8r8a8-premultiplied");
        assert_eq!(name(MEMORY_FORMAT_VALUES[15].value_name), "R32g32b32a32FloatPremultiplied");
        assert_eq!(
            name(MEMORY_FORMAT_SELECTION_VALUES[12].value_name),
            "R16g16b16Float"
        );
    }

    #[test]
    fn tables_are_nul_terminated() {
        let last = MEMORY_FORMAT_VALUES.last().unwrap();
        assert!(last.value_name.is_null());
        let last = MEMORY_FORMAT_SELECTION_VALUES.last().unwrap();
        assert!(last.value_name.is_null());
    }
}
