//! `libglycin-2.so.0` compat shim that forwards to `glycin-ng`.
//!
//! Arch's `gdk-pixbuf2` package is built with the glycin-backed
//! loader compiled in, so its `libgdk_pixbuf-2.0.so.0` has a hard
//! `NEEDED libglycin-2.so.0` and calls the `gly_*` C API directly.
//! This crate produces a shared object with the same SONAME that
//! re-exports those symbols, routing every call to the glycin-ng C
//! ABI exported by `libglycin_ng.so`.
//!
//! The shim does not statically link glycin-ng; the codec stack
//! lives in `libglycin_ng.so` and is dynamically linked at runtime.
//! `libglycin-2.so.0` therefore stays small and the same Rust code
//! is not bundled into two shared objects.
//!
//! The opaque `GlyLoader`, `GlyImage`, `GlyFrame`, and
//! `GlyFrameRequest` handles are returned as base `GObject`s with our
//! Rust-side state attached via `g_object_set_data_full`, rather than
//! as instances of their registered subtypes. The `gly_*_get_type`
//! functions still register those subtypes (see [`gtypes`]) so callers
//! and introspection see the expected `GType`s, but the handles
//! themselves stay base `GObject`s, which is sufficient for everything
//! Arch's gdk-pixbuf does with them.

mod asyncops;
mod cicp;
mod convert;
mod ffi;
mod gtypes;
mod memformat;
mod mime;
mod mimelist;
mod ngapi;
mod types;

use std::ffi::{CStr, CString, c_char, c_int, c_uint, c_void};
use std::ptr;
use std::slice;
use std::sync::{Arc, Mutex, OnceLock};

use crate::convert::RawFrame;
use crate::ffi::{
    GBytes, GError, GFile, GInputStream, GObject, GQuark, GStrv, GType, gboolean, gpointer,
};
use crate::types::{
    CreatorState, EncodedImageState, FrameRequestState, FrameState, ImageState, LoaderState,
    Rerender,
};

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

pub(crate) unsafe fn set_error(error: *mut *mut GError, code: c_int, msg: &str) {
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

unsafe fn set_error_from_ng(error: *mut *mut GError, prefix: &str) {
    let msg = format!("{prefix}: {}", ngapi::last_error_message());
    unsafe { set_error(error, 0, &msg) };
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
    let path_owned = unsafe { CStr::from_ptr(path_ptr) }
        .to_string_lossy()
        .into_owned();
    unsafe { ffi::g_free(path_ptr as *mut c_void) };
    let bytes = match std::fs::read(&path_owned) {
        Ok(b) => b,
        Err(_) => return ptr::null_mut(),
    };
    new_loader_from_bytes(bytes)
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
    new_loader_from_bytes(vec)
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
            return ptr::null_mut();
        }
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n as usize]);
    }
    new_loader_from_bytes(buf)
}

fn new_loader_from_bytes(bytes: Vec<u8>) -> *mut GObject {
    let source_bytes: Arc<[u8]> = Arc::from(bytes.clone().into_boxed_slice());
    let loader = unsafe { ngapi::glycin_ng_loader_new_bytes(bytes.as_ptr(), bytes.len()) };
    if loader.is_null() {
        return ptr::null_mut();
    }
    unsafe { attach_state(LoaderState::new(loader, Some(source_bytes))) }
}

// ----- gly_loader_set_* -----

/// Accepted for ABI compatibility, but ignored. The in-process
/// landlock + seccomp + rlimit posture is fixed and cannot be
/// disabled through this entrypoint.
///
/// # Safety
/// `loader` may be NULL or a Loader handle returned from
/// `gly_loader_new*`; either way nothing is read or written.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_loader_set_sandbox_selector(_loader: *mut GObject, _selector: c_int) {}

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

/// Record the bitmask of `GlyMemoryFormatSelection` values the
/// caller accepts. On `gly_loader_load`, each decoded frame whose
/// native format is not in the set is converted to a compatible
/// format (when we know how) before being returned.
///
/// # Safety
/// `loader` must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_loader_set_accepted_memory_formats(
    loader: *mut GObject,
    selection: c_uint,
) {
    let Some(state) = (unsafe { state_ref::<LoaderState>(loader) }) else {
        return;
    };
    *state.accepted_memory_formats.lock().unwrap() = selection;
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

    let inner_ptr = match state.inner.lock().unwrap().take() {
        Some(p) => p,
        None => {
            unsafe { set_error(error, 0, "loader already consumed") };
            return ptr::null_mut();
        }
    };

    let apply = *state.apply_transformations.lock().unwrap();
    let limits = *state.limits.lock().unwrap();
    let accepted = *state.accepted_memory_formats.lock().unwrap();

    unsafe {
        ngapi::glycin_ng_loader_apply_transformations(inner_ptr, apply as c_int);
    }
    limits.apply(inner_ptr);

    let image_ptr = unsafe { ngapi::glycin_ng_loader_load(inner_ptr) };
    if image_ptr.is_null() {
        unsafe { set_error_from_ng(error, "gly_loader_load") };
        return ptr::null_mut();
    }

    let frame_count = unsafe { ngapi::glycin_ng_image_frame_count(image_ptr) };
    let format_name_ptr = unsafe { ngapi::glycin_ng_image_format_name(image_ptr) };
    let format_name = if format_name_ptr.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(format_name_ptr) }
            .to_string_lossy()
            .into_owned()
    };
    let rerender = state.source_bytes.as_ref().and_then(|bytes| {
        if is_rescalable_format(&format_name) {
            Some(Rerender {
                source_bytes: bytes.clone(),
                limits,
                apply_transformations: apply,
                accepted_memory_formats: accepted,
            })
        } else {
            None
        }
    });
    let mime = mime::from_format_name(&format_name).and_then(|m| CString::new(m).ok());
    unsafe { attach_state(ImageState::new(image_ptr, frame_count, rerender, mime)) }
}

fn is_rescalable_format(name: &str) -> bool {
    matches!(name, "svg")
}

// ----- gly_image_get_* -----

/// # Safety
/// `image` must be valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_image_get_width(image: *mut GObject) -> u32 {
    let Some(state) = (unsafe { state_ref::<ImageState>(image) }) else {
        return 0;
    };
    unsafe { ngapi::glycin_ng_image_width(state.inner) }
}

/// # Safety
/// `image` must be valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_image_get_height(image: *mut GObject) -> u32 {
    let Some(state) = (unsafe { state_ref::<ImageState>(image) }) else {
        return 0;
    };
    unsafe { ngapi::glycin_ng_image_height(state.inner) }
}

/// # Safety
/// `image` must be valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_image_get_transformation_orientation(image: *mut GObject) -> u16 {
    let Some(state) = (unsafe { state_ref::<ImageState>(image) }) else {
        return 1;
    };
    unsafe { ngapi::glycin_ng_image_orientation(state.inner) }
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
    let request_state = unsafe { state_ref::<FrameRequestState>(frame_request) };
    let loop_animation = request_state
        .map(|s| *s.loop_animation.lock().unwrap())
        .unwrap_or(true);
    let scale = request_state.and_then(|s| *s.scale.lock().unwrap());
    // The accepted-formats bitmask lives on the Loader; we only
    // have the Image here. Re-derive from the rerender snapshot for
    // vector formats, otherwise leave at 0 (passthrough). gdk-pixbuf
    // does not currently rely on conversion for raster frames in
    // this code path.
    let accepted = img_state
        .rerender
        .as_ref()
        .map(|r| r.accepted_memory_formats)
        .unwrap_or(0);

    // SVG re-render: when the caller asks for a different size, redo
    // the decode at that size so the output is crisp instead of
    // gdk-pixbuf bitmap-stretching the intrinsic size.
    if let Some(rerender) = img_state.rerender.as_ref()
        && let Some((sw, sh)) = scale
        && sw > 0
        && sh > 0
        && (sw != unsafe { ngapi::glycin_ng_image_width(img_state.inner) }
            || sh != unsafe { ngapi::glycin_ng_image_height(img_state.inner) })
    {
        let new_loader = unsafe {
            ngapi::glycin_ng_loader_new_bytes(
                rerender.source_bytes.as_ptr(),
                rerender.source_bytes.len(),
            )
        };
        if !new_loader.is_null() {
            unsafe {
                ngapi::glycin_ng_loader_apply_transformations(
                    new_loader,
                    rerender.apply_transformations as c_int,
                );
                ngapi::glycin_ng_loader_render_size_hint(new_loader, sw, sh);
            }
            rerender.limits.apply(new_loader);
            let new_image = unsafe { ngapi::glycin_ng_loader_load(new_loader) };
            if !new_image.is_null()
                && let Some(raw) = extract_frame(new_image, 0, rerender.accepted_memory_formats)
            {
                unsafe { ngapi::glycin_ng_image_free(new_image) };
                return unsafe { attach_state(FrameState { frame: raw }) };
            }
            if !new_image.is_null() {
                unsafe { ngapi::glycin_ng_image_free(new_image) };
            }
            // Fall through to the cached frame so the caller still
            // gets something to display.
        }
    }

    let idx = match img_state.advance(loop_animation) {
        Some(i) => i,
        None => {
            unsafe { set_error(error, 2, "no more frames") };
            return ptr::null_mut();
        }
    };
    let Some(raw) = extract_frame(img_state.inner, idx, accepted) else {
        unsafe { set_error(error, 0, "frame extraction failed") };
        return ptr::null_mut();
    };
    unsafe { attach_state(FrameState { frame: raw }) }
}

// ----- gly_image_next_frame -----

/// Load the next animation frame, looping to the first frame after
/// the last one. Equivalent to `gly_image_get_specific_frame` with a
/// default frame request.
///
/// # Safety
/// `image` must be valid or NULL; `error` may be NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_image_next_frame(
    image: *mut GObject,
    error: *mut *mut GError,
) -> *mut GObject {
    unsafe { gly_image_get_specific_frame(image, ptr::null_mut(), error) }
}

// ----- gly_image_get_mime_type -----

/// Return the detected MIME type of the image.
///
/// # Safety
/// `image` must be valid or NULL. The returned string is owned by the
/// image and stays valid until the image is freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_image_get_mime_type(image: *mut GObject) -> *const c_char {
    let Some(state) = (unsafe { state_ref::<ImageState>(image) }) else {
        return ptr::null();
    };
    match &state.mime {
        Some(m) => m.as_ptr(),
        None => ptr::null(),
    }
}

fn extract_frame(
    image: *mut ngapi::GlycinNgImage,
    index: usize,
    accepted: u32,
) -> Option<RawFrame> {
    let texture = unsafe { ngapi::glycin_ng_image_texture(image, index) };
    if texture.is_null() {
        return None;
    }
    let width = unsafe { ngapi::glycin_ng_texture_width(texture) };
    let height = unsafe { ngapi::glycin_ng_texture_height(texture) };
    let stride = unsafe { ngapi::glycin_ng_texture_stride(texture) };
    let format = unsafe { ngapi::glycin_ng_texture_format(texture) };
    let data_ptr = unsafe { ngapi::glycin_ng_texture_data(texture) };
    let data_len = unsafe { ngapi::glycin_ng_texture_data_len(texture) };
    let data = if data_ptr.is_null() || data_len == 0 {
        Vec::new()
    } else {
        unsafe { slice::from_raw_parts(data_ptr, data_len) }.to_vec()
    };
    let delay_ms = unsafe { ngapi::glycin_ng_image_frame_delay_ms(image, index) };
    let raw = RawFrame {
        width,
        height,
        stride,
        format,
        data,
        delay_ms,
    };
    Some(convert::maybe_convert(raw, accepted))
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
        .map(|s| s.frame.width)
        .unwrap_or(0)
}

/// # Safety
/// `frame` must be valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_frame_get_height(frame: *mut GObject) -> u32 {
    unsafe { state_ref::<FrameState>(frame) }
        .map(|s| s.frame.height)
        .unwrap_or(0)
}

/// # Safety
/// `frame` must be valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_frame_get_stride(frame: *mut GObject) -> u32 {
    unsafe { state_ref::<FrameState>(frame) }
        .map(|s| s.frame.stride)
        .unwrap_or(0)
}

/// # Safety
/// `frame` must be valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_frame_get_memory_format(frame: *mut GObject) -> c_int {
    unsafe { state_ref::<FrameState>(frame) }
        .map(|s| memformat::ng_to_gly(s.frame.format))
        .unwrap_or(memformat::GLY_MEMORY_R8G8B8A8)
}

/// # Safety
/// `frame` must be valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_frame_get_delay(frame: *mut GObject) -> i64 {
    let Some(state) = (unsafe { state_ref::<FrameState>(frame) }) else {
        return -1;
    };
    if state.frame.delay_ms == 0 {
        -1
    } else {
        // glycin's API documents the unit as microseconds.
        (state.frame.delay_ms as i64).saturating_mul(1000)
    }
}

/// # Safety
/// `frame` must be valid or NULL. The returned `GBytes` is a fresh
/// reference; the caller frees it with `g_bytes_unref`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_frame_get_buf_bytes(frame: *mut GObject) -> *mut GBytes {
    let Some(state) = (unsafe { state_ref::<FrameState>(frame) }) else {
        return ptr::null_mut();
    };
    unsafe {
        ffi::g_bytes_new(
            state.frame.data.as_ptr() as *const c_void,
            state.frame.data.len(),
        )
    }
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

// ----- Encode path -----
//
// Thin wrapper over `glycin_ng_encoder_*`. Each `gly_creator_*` entry
// point forwards its arguments to the inner encoder; the heavy
// lifting (pixel-format conversion, bounds-checking, codec dispatch,
// ICC embedding) lives in `libglycin_ng.so`.

fn with_encoder<R>(
    creator: *mut GObject,
    error: *mut *mut GError,
    fname: &str,
    f: impl FnOnce(*mut ngapi::GlycinNgEncoder) -> R,
) -> Option<R> {
    let Some(state) = (unsafe { state_ref::<CreatorState>(creator) }) else {
        unsafe { set_error(error, 0, &format!("{fname}: invalid creator")) };
        return None;
    };
    let guard = state.encoder.lock().unwrap();
    if guard.is_null() {
        unsafe { set_error(error, 0, &format!("{fname}: encoder already consumed")) };
        return None;
    }
    Some(f(*guard))
}

/// # Safety
/// `mime_type` must be a valid NUL-terminated C string or NULL.
/// `error` must be NULL or a pointer to a NULL `*mut GError`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_creator_new(
    mime_type: *const c_char,
    error: *mut *mut GError,
) -> *mut GObject {
    if mime_type.is_null() {
        unsafe { set_error(error, 0, "gly_creator_new: mime_type is NULL") };
        return ptr::null_mut();
    }
    let kfmt = unsafe { ngapi::glycin_ng_known_format_from_mime(mime_type) };
    if kfmt == 0 {
        let mime = unsafe { CStr::from_ptr(mime_type) }
            .to_string_lossy()
            .into_owned();
        unsafe {
            set_error(
                error,
                0,
                &format!("gly_creator_new: unsupported MIME type \"{mime}\""),
            )
        };
        return ptr::null_mut();
    }
    let encoder = unsafe { ngapi::glycin_ng_encoder_new(kfmt) };
    if encoder.is_null() {
        unsafe { set_error_from_ng(error, "gly_creator_new") };
        return ptr::null_mut();
    }
    let state = CreatorState {
        encoder: Mutex::new(encoder),
    };
    unsafe { attach_state(state) }
}

/// # Safety
/// `creator` must be a valid `GlyCreator` returned by `gly_creator_new`.
/// `texture_bytes` must be a valid `GBytes`. `error` must be NULL or a
/// pointer to a NULL `*mut GError`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_creator_add_frame(
    creator: *mut GObject,
    width: u32,
    height: u32,
    memory_format: c_int,
    texture_bytes: *mut GBytes,
    error: *mut *mut GError,
) -> *mut GObject {
    unsafe {
        gly_creator_add_frame_with_stride(
            creator,
            width,
            height,
            0,
            memory_format,
            texture_bytes,
            error,
        )
    }
}

/// # Safety
/// Same as `gly_creator_add_frame`, with explicit stride.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_creator_add_frame_with_stride(
    creator: *mut GObject,
    width: u32,
    height: u32,
    stride: u32,
    memory_format: c_int,
    texture_bytes: *mut GBytes,
    error: *mut *mut GError,
) -> *mut GObject {
    if texture_bytes.is_null() {
        unsafe { set_error(error, 0, "gly_creator_add_frame: texture_bytes is NULL") };
        return ptr::null_mut();
    }
    let mut size: usize = 0;
    let data_ptr = unsafe { ffi::g_bytes_get_data(texture_bytes, &mut size) };
    if data_ptr.is_null() || size == 0 {
        unsafe { set_error(error, 0, "gly_creator_add_frame: empty texture data") };
        return ptr::null_mut();
    }
    let Some(ng_format) = memformat::gly_to_ng(memory_format) else {
        unsafe { set_error(error, 0, "gly_creator_add_frame: unsupported memory format") };
        return ptr::null_mut();
    };
    let bpp = memformat::bytes_per_pixel_ng(ng_format) as u32;
    let actual_stride = if stride > 0 { stride } else { width * bpp };

    let rc = match with_encoder(creator, error, "gly_creator_add_frame", |enc| unsafe {
        ngapi::glycin_ng_encoder_add_frame(
            enc,
            width,
            height,
            actual_stride,
            ng_format,
            data_ptr as *const u8,
            size,
        )
    }) {
        Some(v) => v,
        None => return ptr::null_mut(),
    };
    if rc != 0 {
        unsafe { set_error_from_ng(error, "gly_creator_add_frame") };
        return ptr::null_mut();
    }

    // Return the creator itself as the frame handle so gdk-pixbuf
    // can optionally call `gly_new_frame_set_color_icc_profile`.
    unsafe { ffi::g_object_ref(creator as *mut c_void) as *mut GObject }
}

/// Capture a metadata key/value pair on the encoder. The pair is
/// retained on the underlying encoder so the data is not lost, even
/// though current encoders do not yet embed it into the output.
///
/// # Safety
/// `creator`, `key`, `value` must be valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_creator_add_metadata_key_value(
    creator: *mut GObject,
    key: *const c_char,
    value: *const c_char,
    _error: *mut *mut GError,
) -> gboolean {
    if key.is_null() || value.is_null() {
        return 0;
    }
    let rc = with_encoder(
        creator,
        ptr::null_mut(),
        "gly_creator_add_metadata_key_value",
        |enc| unsafe { ngapi::glycin_ng_encoder_add_metadata(enc, key, value) },
    );
    matches!(rc, Some(0)) as gboolean
}

/// # Safety
/// `creator` must be a valid `GlyCreator`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_creator_set_encoding_quality(
    creator: *mut GObject,
    quality: u8,
    _error: *mut *mut GError,
) -> gboolean {
    let ok = with_encoder(
        creator,
        ptr::null_mut(),
        "gly_creator_set_encoding_quality",
        |enc| unsafe { ngapi::glycin_ng_encoder_set_quality(enc, quality) },
    );
    ok.is_some() as gboolean
}

/// # Safety
/// `creator` must be a valid `GlyCreator`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_creator_set_encoding_compression(
    creator: *mut GObject,
    compression: u8,
    _error: *mut *mut GError,
) -> gboolean {
    let ok = with_encoder(
        creator,
        ptr::null_mut(),
        "gly_creator_set_encoding_compression",
        |enc| unsafe { ngapi::glycin_ng_encoder_set_compression(enc, compression) },
    );
    ok.is_some() as gboolean
}

/// # Safety
/// Stub; sandboxing is not applicable to the encode path. Returns TRUE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_creator_set_sandbox_selector(
    _creator: *mut GObject,
    _selector: c_int,
) -> gboolean {
    1
}

/// # Safety
/// `creator` must be a valid `GlyCreator` with at least one frame added.
/// `error` must be NULL or a pointer to a NULL `*mut GError`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_creator_create(
    creator: *mut GObject,
    error: *mut *mut GError,
) -> *mut GObject {
    let encoded = match with_encoder(creator, error, "gly_creator_create", |enc| unsafe {
        ngapi::glycin_ng_encoder_encode(enc)
    }) {
        Some(p) => p,
        None => return ptr::null_mut(),
    };
    if encoded.is_null() {
        unsafe { set_error_from_ng(error, "gly_creator_create") };
        return ptr::null_mut();
    }
    unsafe { attach_state(EncodedImageState { inner: encoded }) }
}

/// Attach an ICC profile to the encoder. `gly_creator_add_frame*`
/// returns the creator itself (with an extra ref) as the "new frame"
/// handle, so this function unwraps that and forwards to the
/// underlying encoder.
///
/// # Safety
/// `new_frame` must be a creator handle and `icc_profile` a valid
/// `GBytes`, or both may be NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_new_frame_set_color_icc_profile(
    new_frame: *mut GObject,
    icc_profile: *mut GBytes,
) -> gboolean {
    let (data, len) = if icc_profile.is_null() {
        (ptr::null(), 0usize)
    } else {
        let mut size: usize = 0;
        let data_ptr = unsafe { ffi::g_bytes_get_data(icc_profile, &mut size) };
        if data_ptr.is_null() || size == 0 {
            (ptr::null(), 0)
        } else {
            (data_ptr as *const u8, size)
        }
    };
    let rc = with_encoder(
        new_frame,
        ptr::null_mut(),
        "gly_new_frame_set_color_icc_profile",
        |enc| unsafe { ngapi::glycin_ng_encoder_set_icc_profile(enc, data, len) },
    );
    matches!(rc, Some(0)) as gboolean
}

/// # Safety
/// `encoded` must be a valid `GlyEncodedImage` returned by `gly_creator_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_encoded_image_get_data(encoded: *mut GObject) -> *mut GBytes {
    let Some(state) = (unsafe { state_ref::<EncodedImageState>(encoded) }) else {
        return ptr::null_mut();
    };
    let data_ptr = unsafe { ngapi::glycin_ng_encoded_image_data(state.inner) };
    let data_len = unsafe { ngapi::glycin_ng_encoded_image_len(state.inner) };
    if data_ptr.is_null() || data_len == 0 {
        return ptr::null_mut();
    }
    unsafe { ffi::g_bytes_new(data_ptr as *const c_void, data_len) }
}

#[cfg(test)]
mod symbol_coverage {
    //! Guard the full `libglycin-2` export surface. Removing any
    //! entry point breaks ABI compatibility for apps built against
    //! upstream glycin, so this test fails the build if the set of
    //! exported `gly_*` functions ever shrinks below the 63 symbols
    //! upstream's `glycin.h` declares for `libglycin-2.so.0`.

    /// Every `gly_*` symbol the shim must export, referenced by
    /// address so the test stops compiling if one is deleted.
    fn exports() -> Vec<*const ()> {
        vec![
        super::gly_loader_error_quark as *const (),
        super::gly_loader_new as *const (),
        super::gly_loader_new_for_bytes as *const (),
        super::gly_loader_new_for_stream as *const (),
        super::gly_loader_set_sandbox_selector as *const (),
        super::gly_loader_set_apply_transformations as *const (),
        super::gly_loader_set_accepted_memory_formats as *const (),
        super::gly_loader_load as *const (),
        super::gly_image_get_width as *const (),
        super::gly_image_get_height as *const (),
        super::gly_image_get_transformation_orientation as *const (),
        super::gly_image_get_metadata_keys as *const (),
        super::gly_image_get_metadata_key_value as *const (),
        super::gly_image_get_specific_frame as *const (),
        super::gly_image_next_frame as *const (),
        super::gly_image_get_mime_type as *const (),
        super::gly_frame_request_new as *const (),
        super::gly_frame_request_set_scale as *const (),
        super::gly_frame_request_set_loop_animation as *const (),
        super::gly_frame_get_width as *const (),
        super::gly_frame_get_height as *const (),
        super::gly_frame_get_stride as *const (),
        super::gly_frame_get_memory_format as *const (),
        super::gly_frame_get_delay as *const (),
        super::gly_frame_get_buf_bytes as *const (),
        super::gly_memory_format_has_alpha as *const (),
        super::gly_memory_format_is_premultiplied as *const (),
        super::gly_creator_new as *const (),
        super::gly_creator_add_frame as *const (),
        super::gly_creator_add_frame_with_stride as *const (),
        super::gly_creator_add_metadata_key_value as *const (),
        super::gly_creator_set_encoding_quality as *const (),
        super::gly_creator_set_encoding_compression as *const (),
        super::gly_creator_set_sandbox_selector as *const (),
        super::gly_creator_create as *const (),
        super::gly_new_frame_set_color_icc_profile as *const (),
        super::gly_encoded_image_get_data as *const (),
        super::cicp::gly_cicp_copy as *const (),
        super::cicp::gly_cicp_free as *const (),
        super::cicp::gly_frame_get_color_cicp as *const (),
        super::gtypes::gly_memory_format_get_type as *const (),
        super::gtypes::gly_sandbox_selector_get_type as *const (),
        super::gtypes::gly_loader_error_get_type as *const (),
        super::gtypes::gly_memory_format_selection_get_type as *const (),
        super::gtypes::gly_loader_get_type as *const (),
        super::gtypes::gly_image_get_type as *const (),
        super::gtypes::gly_frame_get_type as *const (),
        super::gtypes::gly_frame_request_get_type as *const (),
        super::gtypes::gly_creator_get_type as *const (),
        super::gtypes::gly_encoded_image_get_type as *const (),
        super::gtypes::gly_new_frame_get_type as *const (),
        super::gtypes::gly_cicp_get_type as *const (),
        super::asyncops::gly_loader_load_async as *const (),
        super::asyncops::gly_loader_load_finish as *const (),
        super::asyncops::gly_image_next_frame_async as *const (),
        super::asyncops::gly_image_next_frame_finish as *const (),
        super::asyncops::gly_image_get_specific_frame_async as *const (),
        super::asyncops::gly_image_get_specific_frame_finish as *const (),
        super::asyncops::gly_creator_create_async as *const (),
        super::asyncops::gly_creator_create_finish as *const (),
        super::mimelist::gly_loader_get_mime_types as *const (),
        super::mimelist::gly_loader_get_mime_types_async as *const (),
        super::mimelist::gly_loader_get_mime_types_finish as *const (),
        ]
    }

    #[test]
    fn exports_full_libglycin2_surface() {
        let exports = exports();
        assert_eq!(exports.len(), 63, "expected 63 gly_* exports");
        assert!(exports.iter().all(|&addr| !addr.is_null()));
    }
}
