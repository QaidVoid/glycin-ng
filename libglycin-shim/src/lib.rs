//! `libglycin-2.so.0` compat shim that forwards to `glycin-ng`.
//!
//! Arch's `gdk-pixbuf2` package is built with the
//! glycin-backed loader compiled in, so its
//! `libgdk_pixbuf-2.0.so.0` has a hard `NEEDED libglycin-2.so.0`
//! and calls the `gly_*` C API directly. This crate produces a
//! shared object with the same SONAME that re-exports those
//! symbols, routes the LOAD path through
//! [`glycin_ng::Loader`], and stubs the encode path (we do not
//! yet ship an encoder).
//!
//! The opaque `GlyLoader`, `GlyImage`, `GlyFrame`, and
//! `GlyFrameRequest` types are returned as base `GObject`s with
//! our Rust-side state attached via `g_object_set_data_full`. This
//! sidesteps full GType registration and is sufficient for
//! everything Arch's gdk-pixbuf does with these handles.

mod ffi;
mod memformat;
mod types;

use std::ffi::{CStr, CString, c_char, c_int, c_uint, c_void};
use std::path::PathBuf;
use std::ptr;
use std::slice;
use std::sync::OnceLock;

use glycin_ng::{Loader, SandboxSelector};

use crate::ffi::{
    GBytes, GError, GFile, GInputStream, GObject, GQuark, GStrv, GType, gboolean, gpointer,
};
use crate::types::{FrameRequestState, FrameState, ImageState, LoaderState};

const STATE_KEY: &CStr = c"glycin_ng_state";

fn state_key() -> *const c_char {
    STATE_KEY.as_ptr()
}

fn gobject_type() -> GType {
    static CELL: OnceLock<GType> = OnceLock::new();
    *CELL.get_or_init(|| unsafe { ffi::g_object_get_type() })
}

unsafe fn attach_state<T: 'static>(state: T) -> *mut GObject {
    let obj = unsafe { ffi::g_object_new(gobject_type(), ptr::null()) };
    if obj.is_null() {
        return ptr::null_mut();
    }
    let raw = Box::into_raw(Box::new(state)) as gpointer;
    unsafe {
        ffi::g_object_set_data_full(obj, state_key(), raw, Some(state_drop::<T>));
    }
    obj
}

unsafe extern "C" fn state_drop<T>(data: gpointer) {
    if !data.is_null() {
        drop(unsafe { Box::from_raw(data as *mut T) });
    }
}

unsafe fn state_ref<'a, T>(obj: *mut GObject) -> Option<&'a T> {
    if obj.is_null() {
        return None;
    }
    let raw = unsafe { ffi::g_object_get_data(obj, state_key()) };
    if raw.is_null() {
        return None;
    }
    Some(unsafe { &*(raw as *const T) })
}

unsafe fn set_error(error: *mut *mut GError, code: c_int, msg: &str) {
    if error.is_null() {
        return;
    }
    let cmsg = match CString::new(msg) {
        Ok(c) => c,
        Err(_) => CString::new("error").unwrap(),
    };
    let domain = gly_loader_error_quark();
    unsafe {
        ffi::g_set_error_literal(error, domain, code, cmsg.as_ptr());
    }
}

// ----- gly_loader_error_quark -----

/// # Safety
/// Always safe; returns a process-global quark.
#[unsafe(no_mangle)]
pub extern "C" fn gly_loader_error_quark() -> GQuark {
    unsafe { ffi::g_quark_from_static_string(c"gly-loader-error-quark".as_ptr()) }
}

// ----- gly_loader_new family -----

/// # Safety
/// `file` must be a valid GFile or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_loader_new(file: *mut GFile) -> *mut GObject {
    if file.is_null() {
        return ptr::null_mut();
    }
    let path_ptr = unsafe { ffi::g_file_get_path(file) };
    if path_ptr.is_null() {
        // Non-native GFile (e.g. URI not on disk). We don't yet
        // resolve those; the caller will get a NULL handle.
        return ptr::null_mut();
    }
    let path_owned = unsafe { CStr::from_ptr(path_ptr) }.to_string_lossy().into_owned();
    unsafe { ffi::g_free(path_ptr as *mut c_void) };
    let loader = Loader::new_path(PathBuf::from(path_owned));
    unsafe { attach_state(LoaderState::new(loader)) }
}

/// # Safety
/// `bytes` must be a valid GBytes or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_loader_new_for_bytes(bytes: *mut GBytes) -> *mut GObject {
    if bytes.is_null() {
        return ptr::null_mut();
    }
    let mut size: usize = 0;
    let data = unsafe { ffi::g_bytes_get_data(bytes, &mut size) };
    if data.is_null() || size == 0 {
        return ptr::null_mut();
    }
    let vec = unsafe { slice::from_raw_parts(data as *const u8, size) }.to_vec();
    let loader = Loader::new_bytes(vec);
    unsafe { attach_state(LoaderState::new(loader)) }
}

/// # Safety
/// `stream` must be a valid GInputStream or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_loader_new_for_stream(stream: *mut GInputStream) -> *mut GObject {
    if stream.is_null() {
        return ptr::null_mut();
    }
    let mut buf = Vec::<u8>::new();
    let mut chunk = vec![0u8; 65536];
    loop {
        let mut err: *mut GError = ptr::null_mut();
        let n = unsafe {
            ffi::g_input_stream_read(
                stream,
                chunk.as_mut_ptr() as *mut c_void,
                chunk.len(),
                ptr::null_mut(),
                &mut err,
            )
        };
        if n < 0 {
            // Read failure. We swallow the GError pointer rather
            // than propagating it (no error channel on this
            // constructor), and return a NULL handle.
            return ptr::null_mut();
        }
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n as usize]);
    }
    let loader = Loader::new_bytes(buf);
    unsafe { attach_state(LoaderState::new(loader)) }
}

// ----- gly_loader_set_* -----

/// # Safety
/// `loader` must be a Loader handle returned from `gly_loader_new*`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_loader_set_sandbox_selector(
    loader: *mut GObject,
    selector: c_int,
) {
    let Some(state) = (unsafe { state_ref::<LoaderState>(loader) }) else {
        return;
    };
    let mapped = match selector {
        // GLY_SANDBOX_SELECTOR_NOT_SANDBOXED
        3 => SandboxSelector::none(),
        _ => SandboxSelector::default(),
    };
    *state.sandbox.lock().unwrap() = mapped;
}

/// # Safety
/// `loader` must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_loader_set_apply_transformations(
    loader: *mut GObject,
    apply: gboolean,
) {
    let Some(state) = (unsafe { state_ref::<LoaderState>(loader) }) else {
        return;
    };
    *state.apply_transformations.lock().unwrap() = apply != 0;
}

/// # Safety
/// `loader` must be valid. The selection bitmask is currently
/// ignored - glycin-ng decoders pick the native format.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_loader_set_accepted_memory_formats(
    _loader: *mut GObject,
    _selection: c_uint,
) {
    // Not yet wired: we always hand back the decoder's native
    // memory format and let the host convert.
}

// ----- gly_loader_load -----

/// # Safety
/// `loader` must be valid; `error` may be NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_loader_load(
    loader: *mut GObject,
    error: *mut *mut GError,
) -> *mut GObject {
    let Some(state) = (unsafe { state_ref::<LoaderState>(loader) }) else {
        unsafe { set_error(error, 0, "null loader") };
        return ptr::null_mut();
    };

    let inner = match state.inner.lock().unwrap().take() {
        Some(l) => l,
        None => {
            unsafe { set_error(error, 0, "loader already consumed") };
            return ptr::null_mut();
        }
    };

    let sandbox = *state.sandbox.lock().unwrap();
    let apply = *state.apply_transformations.lock().unwrap();
    let limits = *state.limits.lock().unwrap();
    let configured = inner
        .sandbox_selector(sandbox)
        .apply_transformations(apply)
        .limits(limits);

    match configured.load() {
        Ok(image) => unsafe { attach_state(ImageState::new(image)) },
        Err(e) => {
            unsafe { set_error(error, 0, &e.to_string()) };
            ptr::null_mut()
        }
    }
}

// ----- gly_image_get_* -----

/// # Safety
/// `image` must be valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_image_get_width(image: *mut GObject) -> u32 {
    unsafe { state_ref::<ImageState>(image) }
        .map(|s| s.image.width())
        .unwrap_or(0)
}

/// # Safety
/// `image` must be valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_image_get_height(image: *mut GObject) -> u32 {
    unsafe { state_ref::<ImageState>(image) }
        .map(|s| s.image.height())
        .unwrap_or(0)
}

/// # Safety
/// `image` must be valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_image_get_transformation_orientation(
    image: *mut GObject,
) -> u16 {
    unsafe { state_ref::<ImageState>(image) }
        .map(|s| s.image.orientation().exif_value())
        .unwrap_or(1)
}

/// # Safety
/// `image` must be valid or NULL. Currently returns NULL: no
/// per-key metadata is exposed yet.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_image_get_metadata_keys(_image: *mut GObject) -> GStrv {
    ptr::null_mut()
}

/// # Safety
/// `image` and `key` must be valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_image_get_metadata_key_value(
    _image: *mut GObject,
    _key: *const c_char,
) -> *mut c_char {
    ptr::null_mut()
}

// ----- gly_image_get_specific_frame -----

/// # Safety
/// `image` must be valid; `frame_request` and `error` may be NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_image_get_specific_frame(
    image: *mut GObject,
    frame_request: *mut GObject,
    error: *mut *mut GError,
) -> *mut GObject {
    let Some(img_state) = (unsafe { state_ref::<ImageState>(image) }) else {
        unsafe { set_error(error, 0, "null image") };
        return ptr::null_mut();
    };
    let loop_animation = unsafe { state_ref::<FrameRequestState>(frame_request) }
        .map(|s| *s.loop_animation.lock().unwrap())
        .unwrap_or(true);
    let idx = match img_state.advance(loop_animation) {
        Some(i) => i,
        None => {
            unsafe { set_error(error, 2, "no more frames") };
            return ptr::null_mut();
        }
    };
    let frame = img_state.image.frames()[idx].clone();
    unsafe { attach_state(FrameState { frame }) }
}

// ----- gly_frame_request_* -----

/// # Safety
/// Always safe.
#[unsafe(no_mangle)]
pub extern "C" fn gly_frame_request_new() -> *mut GObject {
    unsafe { attach_state(FrameRequestState::new()) }
}

/// # Safety
/// `request` must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_frame_request_set_scale(
    request: *mut GObject,
    width: u32,
    height: u32,
) {
    let Some(state) = (unsafe { state_ref::<FrameRequestState>(request) }) else {
        return;
    };
    *state.scale.lock().unwrap() = Some((width, height));
}

/// # Safety
/// `request` must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_frame_request_set_loop_animation(
    request: *mut GObject,
    loop_animation: gboolean,
) {
    let Some(state) = (unsafe { state_ref::<FrameRequestState>(request) }) else {
        return;
    };
    *state.loop_animation.lock().unwrap() = loop_animation != 0;
}

// ----- gly_frame_get_* -----

/// # Safety
/// `frame` must be valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_frame_get_width(frame: *mut GObject) -> u32 {
    unsafe { state_ref::<FrameState>(frame) }
        .map(|s| s.frame.width())
        .unwrap_or(0)
}

/// # Safety
/// `frame` must be valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_frame_get_height(frame: *mut GObject) -> u32 {
    unsafe { state_ref::<FrameState>(frame) }
        .map(|s| s.frame.height())
        .unwrap_or(0)
}

/// # Safety
/// `frame` must be valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_frame_get_stride(frame: *mut GObject) -> u32 {
    unsafe { state_ref::<FrameState>(frame) }
        .map(|s| s.frame.texture().stride())
        .unwrap_or(0)
}

/// # Safety
/// `frame` must be valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_frame_get_memory_format(frame: *mut GObject) -> c_int {
    unsafe { state_ref::<FrameState>(frame) }
        .map(|s| memformat::to_gly(s.frame.texture().format()))
        .unwrap_or(memformat::GLY_MEMORY_R8G8B8A8)
}

/// # Safety
/// `frame` must be valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_frame_get_delay(frame: *mut GObject) -> i64 {
    unsafe { state_ref::<FrameState>(frame) }
        .and_then(|s| s.frame.delay())
        .map(|d| d.as_micros() as i64)
        .unwrap_or(-1)
}

/// # Safety
/// `frame` must be valid or NULL. The returned `GBytes` is a fresh
/// reference; the caller frees it with `g_bytes_unref`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_frame_get_buf_bytes(frame: *mut GObject) -> *mut GBytes {
    let Some(state) = (unsafe { state_ref::<FrameState>(frame) }) else {
        return ptr::null_mut();
    };
    let data = state.frame.texture().data();
    unsafe { ffi::g_bytes_new(data.as_ptr() as *const c_void, data.len()) }
}

// ----- gly_memory_format helpers -----

/// # Safety
/// Always safe; pure value.
#[unsafe(no_mangle)]
pub extern "C" fn gly_memory_format_has_alpha(format: c_int) -> gboolean {
    memformat::has_alpha_for_gly(format)
}

/// # Safety
/// Always safe; pure value.
#[unsafe(no_mangle)]
pub extern "C" fn gly_memory_format_is_premultiplied(format: c_int) -> gboolean {
    memformat::is_premultiplied_for_gly(format)
}

// ----- Encode path (stubs - glycin-ng has no encoder yet) -----

/// # Safety
/// All encode-path entry points return NULL/FALSE. The caller is
/// expected to handle that as "encoding unsupported".
#[unsafe(no_mangle)]
pub extern "C" fn gly_creator_new(
    _mime_type: *const c_char,
    _error: *mut *mut GError,
) -> *mut GObject {
    ptr::null_mut()
}

/// # Safety
/// Stub.
#[unsafe(no_mangle)]
pub extern "C" fn gly_creator_add_frame(
    _creator: *mut GObject,
    _width: u32,
    _height: u32,
    _memory_format: c_int,
    _texture_bytes: *mut GBytes,
    _error: *mut *mut GError,
) -> *mut GObject {
    ptr::null_mut()
}

/// # Safety
/// Stub.
#[unsafe(no_mangle)]
pub extern "C" fn gly_creator_add_frame_with_stride(
    _creator: *mut GObject,
    _width: u32,
    _height: u32,
    _stride: u32,
    _memory_format: c_int,
    _texture_bytes: *mut GBytes,
    _error: *mut *mut GError,
) -> *mut GObject {
    ptr::null_mut()
}

/// # Safety
/// Stub.
#[unsafe(no_mangle)]
pub extern "C" fn gly_creator_add_metadata_key_value(
    _creator: *mut GObject,
    _key: *const c_char,
    _value: *const c_char,
    _error: *mut *mut GError,
) -> gboolean {
    0
}

/// # Safety
/// Stub.
#[unsafe(no_mangle)]
pub extern "C" fn gly_creator_set_encoding_quality(
    _creator: *mut GObject,
    _quality: u8,
    _error: *mut *mut GError,
) -> gboolean {
    0
}

/// # Safety
/// Stub.
#[unsafe(no_mangle)]
pub extern "C" fn gly_creator_set_encoding_compression(
    _creator: *mut GObject,
    _compression: u8,
    _error: *mut *mut GError,
) -> gboolean {
    0
}

/// # Safety
/// Stub.
#[unsafe(no_mangle)]
pub extern "C" fn gly_creator_set_sandbox_selector(
    _creator: *mut GObject,
    _selector: c_int,
) -> gboolean {
    0
}

/// # Safety
/// Stub.
#[unsafe(no_mangle)]
pub extern "C" fn gly_creator_create(
    _creator: *mut GObject,
    _error: *mut *mut GError,
) -> *mut GObject {
    ptr::null_mut()
}

/// # Safety
/// Stub.
#[unsafe(no_mangle)]
pub extern "C" fn gly_new_frame_set_color_icc_profile(
    _new_frame: *mut GObject,
    _icc_profile: *mut GBytes,
) -> gboolean {
    0
}

/// # Safety
/// Stub.
#[unsafe(no_mangle)]
pub extern "C" fn gly_encoded_image_get_data(_encoded: *mut GObject) -> *mut GBytes {
    ptr::null_mut()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loader_error_quark_is_nonzero() {
        let q = gly_loader_error_quark();
        assert!(q != 0);
    }

    #[test]
    fn memory_format_helpers_classify_correctly() {
        assert_eq!(
            gly_memory_format_has_alpha(memformat::GLY_MEMORY_R8G8B8A8),
            1
        );
        assert_eq!(gly_memory_format_has_alpha(memformat::GLY_MEMORY_R8G8B8), 0);
        assert_eq!(
            gly_memory_format_is_premultiplied(memformat::GLY_MEMORY_R8G8B8A8_PREMULTIPLIED),
            1
        );
        assert_eq!(
            gly_memory_format_is_premultiplied(memformat::GLY_MEMORY_R8G8B8A8),
            0
        );
    }

    #[test]
    fn creator_path_returns_nulls() {
        let p = gly_creator_new(ptr::null(), ptr::null_mut());
        assert!(p.is_null());
        let q = gly_creator_create(ptr::null_mut(), ptr::null_mut());
        assert!(q.is_null());
    }

    #[test]
    fn null_object_helpers_return_zero() {
        assert_eq!(unsafe { gly_image_get_width(ptr::null_mut()) }, 0);
        assert_eq!(unsafe { gly_image_get_height(ptr::null_mut()) }, 0);
        assert_eq!(
            unsafe { gly_image_get_transformation_orientation(ptr::null_mut()) },
            1
        );
        assert_eq!(unsafe { gly_frame_get_width(ptr::null_mut()) }, 0);
        assert_eq!(unsafe { gly_frame_get_delay(ptr::null_mut()) }, -1);
    }
}
