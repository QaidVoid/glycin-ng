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

mod convert;
mod ffi;
mod memformat;
mod types;

use std::ffi::{CStr, CString, c_char, c_int, c_uint, c_void};
use std::ptr;
use std::slice;
use std::sync::{Arc, Mutex, OnceLock};

use glycin_ng::Loader;

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
    let path_owned = unsafe { CStr::from_ptr(path_ptr) }
        .to_string_lossy()
        .into_owned();
    unsafe { ffi::g_free(path_ptr as *mut c_void) };
    // Read the file eagerly so we retain the source bytes for a
    // possible later re-decode at a caller-requested scale. The
    // `Loader::new_bytes` path is the same one the existing flow
    // would have used after `Loader::new_path` lazily read the file.
    let bytes = match std::fs::read(&path_owned) {
        Ok(b) => b,
        Err(_) => return ptr::null_mut(),
    };
    let source_bytes: Arc<[u8]> = Arc::from(bytes.clone().into_boxed_slice());
    let loader = Loader::new_bytes(bytes);
    unsafe { attach_state(LoaderState::new(loader, Some(source_bytes))) }
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
    let source_bytes: Arc<[u8]> = Arc::from(vec.clone().into_boxed_slice());
    let loader = Loader::new_bytes(vec);
    unsafe { attach_state(LoaderState::new(loader, Some(source_bytes))) }
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
    let source_bytes: Arc<[u8]> = Arc::from(buf.clone().into_boxed_slice());
    let loader = Loader::new_bytes(buf);
    unsafe { attach_state(LoaderState::new(loader, Some(source_bytes))) }
}

// ----- gly_loader_set_* -----

/// Accepted for ABI compatibility, but ignored. The in-process
/// landlock + seccomp + rlimit posture is fixed and cannot be
/// disabled through this entrypoint, so an `LD_PRELOAD` that pins
/// the upstream `NOT_SANDBOXED` selector has no effect here.
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

    let inner = match state.inner.lock().unwrap().take() {
        Some(l) => l,
        None => {
            unsafe { set_error(error, 0, "loader already consumed") };
            return ptr::null_mut();
        }
    };

    let apply = *state.apply_transformations.lock().unwrap();
    let limits = *state.limits.lock().unwrap();
    let accepted = *state.accepted_memory_formats.lock().unwrap();
    let source_bytes = state.source_bytes.clone();

    let configured = inner.apply_transformations(apply).limits(limits);

    match configured.load() {
        Ok(mut image) => {
            if accepted != 0 {
                let width = image.width();
                let height = image.height();
                let new_frames: Vec<_> = image
                    .frames()
                    .iter()
                    .cloned()
                    .map(|f| convert::maybe_convert(f, accepted))
                    .collect();
                image.replace_frames(new_frames, width, height);
            }
            // Vector formats can be re-rendered when the consumer
            // later requests a different scale; raster decoders
            // ignore the hint anyway, so we only stash bytes for
            // formats that can usefully take it.
            let rerender = source_bytes.and_then(|bytes| {
                if is_rescalable_format(image.format_name()) {
                    Some(Rerender {
                        source_bytes: bytes,
                        limits,
                        apply_transformations: apply,
                        accepted_memory_formats: accepted,
                    })
                } else {
                    None
                }
            });
            unsafe { attach_state(ImageState::new(image, rerender)) }
        }
        Err(e) => {
            unsafe { set_error(error, 0, &e.to_string()) };
            ptr::null_mut()
        }
    }
}

fn is_rescalable_format(name: &str) -> bool {
    matches!(name, "svg")
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
pub unsafe extern "C" fn gly_image_get_transformation_orientation(image: *mut GObject) -> u16 {
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
    let request_state = unsafe { state_ref::<FrameRequestState>(frame_request) };
    let loop_animation = request_state
        .map(|s| *s.loop_animation.lock().unwrap())
        .unwrap_or(true);
    let scale = request_state.and_then(|s| *s.scale.lock().unwrap());

    // If this is a vector format and the caller asked for a different
    // size, re-decode at that size. Falling back to the cached frame
    // would force gdk-pixbuf to bitmap-stretch the intrinsic size,
    // which is what made symbolic icons look blurry next to upstream
    // glycin's librsvg-backed output.
    if let Some(rerender) = img_state.rerender.as_ref()
        && let Some((sw, sh)) = scale
        && sw > 0
        && sh > 0
        && (sw != img_state.image.width() || sh != img_state.image.height())
    {
        let bytes = rerender.source_bytes.to_vec();
        let result = Loader::new_bytes(bytes)
            .apply_transformations(rerender.apply_transformations)
            .limits(rerender.limits)
            .render_size_hint(sw, sh)
            .load();
        if let Ok(image) = result
            && let Some(mut frame) = image.frames().first().cloned()
        {
            if rerender.accepted_memory_formats != 0 {
                frame = convert::maybe_convert(frame, rerender.accepted_memory_formats);
            }
            return unsafe { attach_state(FrameState { frame }) };
        }
        // Re-decode failed; fall through to the cached frame so the
        // caller still gets a pixbuf rather than NULL.
    }

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

// ----- Encode path -----
//
// Thin wrapper over `glycin_ng::Encoder`. Each `gly_creator_*` entry
// point forwards its arguments to the inner encoder; the heavy
// lifting (pixel-format conversion, bounds-checking, codec dispatch,
// ICC embedding) lives in `glycin_ng::encoder`.
fn with_encoder<R>(
    creator: *mut GObject,
    error: *mut *mut GError,
    fname: &str,
    f: impl FnOnce(&mut glycin_ng::Encoder) -> R,
) -> Option<R> {
    let Some(state) = (unsafe { state_ref::<CreatorState>(creator) }) else {
        unsafe { set_error(error, 0, &format!("{fname}: invalid creator")) };
        return None;
    };
    let mut guard = state.encoder.lock().unwrap();
    let Some(enc) = guard.as_mut() else {
        unsafe { set_error(error, 0, &format!("{fname}: encoder already consumed")) };
        return None;
    };
    Some(f(enc))
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
    let mime = match unsafe { CStr::from_ptr(mime_type) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            unsafe { set_error(error, 0, "gly_creator_new: invalid UTF-8 in mime_type") };
            return ptr::null_mut();
        }
    };
    let Some(target) = glycin_ng::KnownFormat::from_mime_type(mime) else {
        unsafe {
            set_error(
                error,
                0,
                &format!("gly_creator_new: unsupported MIME type \"{mime}\""),
            )
        };
        return ptr::null_mut();
    };
    let encoder = match glycin_ng::Encoder::new(target) {
        Ok(e) => e,
        Err(e) => {
            unsafe { set_error(error, 0, &format!("gly_creator_new: {e}")) };
            return ptr::null_mut();
        }
    };
    let state = CreatorState {
        encoder: Mutex::new(Some(encoder)),
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
    let Some(mf) = memformat::from_gly(memory_format) else {
        unsafe { set_error(error, 0, "gly_creator_add_frame: unsupported memory format") };
        return ptr::null_mut();
    };
    let data = unsafe { slice::from_raw_parts(data_ptr as *const u8, size) }.to_vec();
    // Caller-supplied stride wins. When zero, fall back to
    // `width * bytes_per_pixel` for the resolved format - using a
    // hardcoded bpp of 4 would tear apart 24-bit RGB textures.
    let actual_stride = if stride > 0 {
        stride
    } else {
        width * mf.bytes_per_pixel() as u32
    };
    let frame = glycin_ng::EncodeFrame {
        width,
        height,
        stride: actual_stride,
        format: mf,
        data,
    };
    if with_encoder(creator, error, "gly_creator_add_frame", |enc| {
        enc.add_frame(frame);
    })
    .is_none()
    {
        return ptr::null_mut();
    }
    // Return the creator itself as the frame handle so gdk-pixbuf
    // can optionally call `gly_new_frame_set_color_icc_profile`.
    unsafe { ffi::g_object_ref(creator as *mut c_void) as *mut GObject }
}

/// Capture a metadata key/value pair on the creator. The pair is
/// retained on the underlying [`glycin_ng::Encoder`] so the data is
/// not lost, even though current encoders do not yet embed it into
/// the output.
///
/// Returns TRUE on capture so gdk-pixbuf-driven saves do not fail
/// when the host forwards a benign metadata key.
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
    let k = match unsafe { CStr::from_ptr(key) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return 0,
    };
    let v = match unsafe { CStr::from_ptr(value) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return 0,
    };
    match with_encoder(
        creator,
        ptr::null_mut(),
        "gly_creator_add_metadata_key_value",
        |enc| {
            enc.add_metadata(k, v);
        },
    ) {
        Some(()) => 1,
        None => 0,
    }
}

/// # Safety
/// `creator` must be a valid `GlyCreator`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_creator_set_encoding_quality(
    creator: *mut GObject,
    quality: u8,
    _error: *mut *mut GError,
) -> gboolean {
    match with_encoder(
        creator,
        ptr::null_mut(),
        "gly_creator_set_encoding_quality",
        |enc| {
            enc.set_quality(quality);
        },
    ) {
        Some(()) => 1,
        None => 0,
    }
}

/// # Safety
/// `creator` must be a valid `GlyCreator`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_creator_set_encoding_compression(
    creator: *mut GObject,
    compression: u8,
    _error: *mut *mut GError,
) -> gboolean {
    match with_encoder(
        creator,
        ptr::null_mut(),
        "gly_creator_set_encoding_compression",
        |enc| {
            enc.set_compression(compression);
        },
    ) {
        Some(()) => 1,
        None => 0,
    }
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
    let Some(state) = (unsafe { state_ref::<CreatorState>(creator) }) else {
        unsafe { set_error(error, 0, "gly_creator_create: invalid creator") };
        return ptr::null_mut();
    };
    let Some(encoder) = state.encoder.lock().unwrap().take() else {
        unsafe { set_error(error, 0, "gly_creator_create: encoder already consumed") };
        return ptr::null_mut();
    };
    match encoder.encode() {
        Ok(data) => unsafe { attach_state(EncodedImageState { data }) },
        Err(e) => {
            unsafe { set_error(error, 0, &format!("gly_creator_create: {e}")) };
            ptr::null_mut()
        }
    }
}

/// Attach an ICC profile to the creator. `gly_creator_add_frame*`
/// returns the creator itself (with an extra ref) as the "new frame"
/// handle, so this function unwraps that and stores the profile on
/// the inner encoder. It is then applied by [`gly_creator_create`]
/// for PNG (iCCP), JPEG (APP2 ICC_PROFILE), and WebP (ICCP) outputs.
/// Other formats ignore the profile.
///
/// # Safety
/// `new_frame` must be a creator handle and `icc_profile` a valid
/// `GBytes`, or both may be NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_new_frame_set_color_icc_profile(
    new_frame: *mut GObject,
    icc_profile: *mut GBytes,
) -> gboolean {
    let icc = if icc_profile.is_null() {
        None
    } else {
        let mut size: usize = 0;
        let data_ptr = unsafe { ffi::g_bytes_get_data(icc_profile, &mut size) };
        if data_ptr.is_null() || size == 0 {
            None
        } else {
            Some(unsafe { slice::from_raw_parts(data_ptr as *const u8, size) }.to_vec())
        }
    };
    match with_encoder(
        new_frame,
        ptr::null_mut(),
        "gly_new_frame_set_color_icc_profile",
        |enc| {
            enc.set_icc_profile(icc);
        },
    ) {
        Some(()) => 1,
        None => 0,
    }
}

/// # Safety
/// `encoded` must be a valid `GlyEncodedImage` returned by `gly_creator_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_encoded_image_get_data(encoded: *mut GObject) -> *mut GBytes {
    let state = match unsafe { state_ref::<EncodedImageState>(encoded) } {
        Some(s) => s,
        None => return ptr::null_mut(),
    };
    unsafe { ffi::g_bytes_new(state.data.as_ptr() as *const c_void, state.data.len()) }
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
        let p = unsafe { gly_creator_new(ptr::null(), ptr::null_mut()) };
        assert!(p.is_null());
        let q = unsafe { gly_creator_create(ptr::null_mut(), ptr::null_mut()) };
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
