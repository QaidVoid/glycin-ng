//! `rsvg_handle_*` lifecycle: construction, loading, options, and
//! dimension queries.

use std::ffi::{CStr, CString, c_char, c_int};
use std::ptr;

use crate::ffi::{self, GCancellable, GError, GFile, GInputStream, gboolean, gpointer};
use crate::gobject::{self, RsvgHandle, state_of};
use crate::ngapi;
use crate::state::{
    HandleState, LoadState, RsvgDimensionData, RsvgLength, RsvgPositionData, RsvgRectangle,
    RsvgSizeFunc, SizeCallback, length_to_pixels,
};

pub const TRUE: gboolean = 1;
pub const FALSE: gboolean = 0;

/// Convert a public element id (`#foo`) to the engine's bare form.
/// NULL means "the whole document".
pub unsafe fn id_cstring(id: *const c_char) -> Option<CString> {
    if id.is_null() {
        return None;
    }
    let bytes = unsafe { CStr::from_ptr(id) }.to_bytes();
    let bytes = bytes.strip_prefix(b"#").unwrap_or(bytes);
    CString::new(bytes).ok()
}

/// Natural integer dimensions plus the exact em/ex values, with the
/// deprecated size callback applied to the integer part.
pub fn natural_dimensions(state: &HandleState) -> RsvgDimensionData {
    let Some((w, h)) = doc_size(state) else {
        return RsvgDimensionData::default();
    };
    let (width, height) = state
        .size_callback
        .apply(w.ceil() as c_int, h.ceil() as c_int);
    RsvgDimensionData {
        width,
        height,
        em: w,
        ex: h,
    }
}

/// Resolved document pixel size from the engine.
pub fn doc_size(state: &HandleState) -> Option<(f64, f64)> {
    let doc = state.document()?;
    let (mut w, mut h) = (0.0f64, 0.0f64);
    if unsafe { ngapi::glycin_ng_svg_size(doc, &mut w, &mut h) } != 0 {
        return None;
    }
    Some((w, h))
}

/// Parse accumulated bytes into the engine document. On failure the
/// handle moves to `Failed` and the engine's last error describes
/// why.
pub fn finish_load(state: &mut HandleState, bytes: &[u8]) -> bool {
    let resources_dir = state.resources_dir();
    let doc = unsafe {
        ngapi::glycin_ng_svg_new(
            bytes.as_ptr(),
            bytes.len(),
            state.effective_dpi(),
            resources_dir
                .as_ref()
                .map(|d| d.as_ptr())
                .unwrap_or(ptr::null()),
            1,
        )
    };
    if doc.is_null() {
        state.load = LoadState::Failed;
        return false;
    }
    if let Some(css) = &state.stylesheet {
        // A pre-load stylesheet re-applies to the fresh document; a
        // failure here fails the load like librsvg's own error path.
        if unsafe { ngapi::glycin_ng_svg_set_stylesheet(doc, css.as_ptr(), css.len()) } != 0 {
            unsafe { ngapi::glycin_ng_svg_free(doc) };
            state.load = LoadState::Failed;
            return false;
        }
    }
    if let LoadState::Loaded(old) = state.load {
        unsafe { ngapi::glycin_ng_svg_free(old) };
    }
    state.load = LoadState::Loaded(doc);
    true
}

pub fn set_dpi_values(state: &mut HandleState, dpi_x: Option<f64>, dpi_y: Option<f64>) {
    if let Some(x) = dpi_x {
        state.dpi_x = x;
    }
    if let Some(y) = dpi_y {
        state.dpi_y = y;
    }
    if let Some(doc) = state.document() {
        let _ = unsafe { ngapi::glycin_ng_svg_set_dpi(doc, state.effective_dpi()) };
    }
}

fn new_handle() -> *mut RsvgHandle {
    unsafe { ffi::g_object_new(gobject::rsvg_handle_get_type(), ptr::null()) }.cast()
}

/// Returns a new, unloaded `RsvgHandle`.
#[unsafe(no_mangle)]
pub extern "C" fn rsvg_handle_new() -> *mut RsvgHandle {
    new_handle()
}

/// Returns a new, unloaded `RsvgHandle` with `flags` set.
#[unsafe(no_mangle)]
pub extern "C" fn rsvg_handle_new_with_flags(flags: u32) -> *mut RsvgHandle {
    let handle = new_handle();
    if let Some(state) = unsafe { state_of(handle) } {
        state.flags = flags;
    }
    handle
}

/// Deprecated alias for `g_object_unref`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_free(handle: *mut RsvgHandle) {
    if !handle.is_null() {
        unsafe { ffi::g_object_unref(handle.cast()) };
    }
}

/// Load an SVG from an in-memory buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_new_from_data(
    data: *const u8,
    data_len: usize,
    error: *mut *mut GError,
) -> *mut RsvgHandle {
    if data.is_null() && data_len != 0 {
        gobject::set_gerror(error, "data is NULL");
        return ptr::null_mut();
    }
    let handle = new_handle();
    let Some(state) = (unsafe { state_of(handle) }) else {
        gobject::set_gerror(error, "failed to construct RsvgHandle");
        return ptr::null_mut();
    };
    let bytes = if data_len == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(data, data_len) }
    };
    if finish_load(state, bytes) {
        handle
    } else {
        gobject::set_gerror_from_engine(error);
        unsafe { ffi::g_object_unref(handle.cast()) };
        ptr::null_mut()
    }
}

/// Load an SVG from a filename or URI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_new_from_file(
    filename: *const c_char,
    error: *mut *mut GError,
) -> *mut RsvgHandle {
    if filename.is_null() {
        gobject::set_gerror(error, "filename is NULL");
        return ptr::null_mut();
    }
    let file = unsafe { ffi::g_file_new_for_commandline_arg(filename) };
    if file.is_null() {
        gobject::set_gerror(error, "could not create GFile");
        return ptr::null_mut();
    }
    let handle = unsafe { rsvg_handle_new_from_gfile_sync(file, 0, ptr::null_mut(), error) };
    unsafe { ffi::g_object_unref(file.cast()) };
    handle
}

/// Load an SVG from a `GFile`, which also becomes the base file.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_new_from_gfile_sync(
    file: *mut GFile,
    flags: u32,
    cancellable: *mut GCancellable,
    error: *mut *mut GError,
) -> *mut RsvgHandle {
    if file.is_null() {
        gobject::set_gerror(error, "file is NULL");
        return ptr::null_mut();
    }
    let stream = unsafe { ffi::g_file_read(file, cancellable, error) };
    if stream.is_null() {
        return ptr::null_mut();
    }
    let handle =
        unsafe { rsvg_handle_new_from_stream_sync(stream.cast(), file, flags, cancellable, error) };
    unsafe { ffi::g_object_unref(stream.cast()) };
    handle
}

/// Load an SVG from a `GInputStream`, with an optional base file.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_new_from_stream_sync(
    input_stream: *mut GInputStream,
    base_file: *mut GFile,
    flags: u32,
    cancellable: *mut GCancellable,
    error: *mut *mut GError,
) -> *mut RsvgHandle {
    let handle = rsvg_handle_new_with_flags(flags);
    if !base_file.is_null() {
        unsafe { rsvg_handle_set_base_gfile(handle, base_file) };
    }
    if unsafe { rsvg_handle_read_stream_sync(handle, input_stream, cancellable, error) } == TRUE {
        handle
    } else {
        unsafe { ffi::g_object_unref(handle.cast()) };
        ptr::null_mut()
    }
}

/// Read the whole stream and parse it into the handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_read_stream_sync(
    handle: *mut RsvgHandle,
    stream: *mut GInputStream,
    cancellable: *mut GCancellable,
    error: *mut *mut GError,
) -> gboolean {
    let Some(state) = (unsafe { state_of(handle) }) else {
        gobject::set_gerror(error, "handle is NULL");
        return FALSE;
    };
    if stream.is_null() {
        gobject::set_gerror(error, "stream is NULL");
        return FALSE;
    }
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 65536];
    loop {
        let mut read_err: *mut GError = ptr::null_mut();
        let n = unsafe {
            ffi::g_input_stream_read(
                stream,
                chunk.as_mut_ptr().cast(),
                chunk.len(),
                cancellable,
                &mut read_err,
            )
        };
        if n < 0 {
            state.load = LoadState::Failed;
            if error.is_null() {
                unsafe { ffi::g_error_free(read_err) };
            } else {
                unsafe { ffi::g_propagate_error(error, read_err) };
            }
            return FALSE;
        }
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..n as usize]);
    }
    if finish_load(state, &bytes) {
        TRUE
    } else {
        gobject::set_gerror_from_engine(error);
        FALSE
    }
}

/// Deprecated push-parser: buffer `count` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_write(
    handle: *mut RsvgHandle,
    buf: *const u8,
    count: usize,
    error: *mut *mut GError,
) -> gboolean {
    let Some(state) = (unsafe { state_of(handle) }) else {
        gobject::set_gerror(error, "handle is NULL");
        return FALSE;
    };
    if buf.is_null() && count != 0 {
        gobject::set_gerror(error, "buffer is NULL");
        return FALSE;
    }
    let bytes = if count == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(buf, count) }
    };
    match &mut state.load {
        LoadState::Start => {
            state.load = LoadState::Loading(bytes.to_vec());
            TRUE
        }
        LoadState::Loading(buffer) => {
            buffer.extend_from_slice(bytes);
            TRUE
        }
        _ => {
            gobject::set_gerror(error, "handle is already loaded");
            FALSE
        }
    }
}

/// Deprecated push-parser: parse everything fed via
/// `rsvg_handle_write`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_close(
    handle: *mut RsvgHandle,
    error: *mut *mut GError,
) -> gboolean {
    let Some(state) = (unsafe { state_of(handle) }) else {
        gobject::set_gerror(error, "handle is NULL");
        return FALSE;
    };
    match std::mem::replace(&mut state.load, LoadState::Start) {
        LoadState::Loading(buffer) => {
            if finish_load(state, &buffer) {
                TRUE
            } else {
                gobject::set_gerror_from_engine(error);
                FALSE
            }
        }
        loaded @ LoadState::Loaded(_) => {
            // Close after a successful load is a documented no-op.
            state.load = loaded;
            TRUE
        }
        LoadState::Start => {
            gobject::set_gerror(error, "no data has been fed to the handle");
            FALSE
        }
        LoadState::Failed => {
            state.load = LoadState::Failed;
            gobject::set_gerror(error, "handle is in an error state");
            FALSE
        }
    }
}

/// Base URI accessor; the returned string stays owned by the handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_get_base_uri(handle: *mut RsvgHandle) -> *const c_char {
    match unsafe { state_of(handle) } {
        Some(state) => state
            .base_uri
            .as_ref()
            .map(|s| s.as_ptr())
            .unwrap_or(ptr::null()),
        None => ptr::null(),
    }
}

/// Set the base URI used to resolve relative references. Only
/// effective before loading.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_set_base_uri(
    handle: *mut RsvgHandle,
    base_uri: *const c_char,
) {
    let Some(state) = (unsafe { state_of(handle) }) else {
        return;
    };
    // A NULL URI is ignored, matching upstream's `!uri.is_null()`
    // precondition and the `base-uri` property setter.
    if base_uri.is_null() {
        return;
    }
    state.base_uri = Some(unsafe { CStr::from_ptr(base_uri) }.to_owned());
}

/// Set the base URI from a `GFile`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_set_base_gfile(
    handle: *mut RsvgHandle,
    base_file: *mut GFile,
) {
    let Some(state) = (unsafe { state_of(handle) }) else {
        return;
    };
    if base_file.is_null() {
        return;
    }
    let uri = unsafe { ffi::g_file_get_uri(base_file) };
    if uri.is_null() {
        return;
    }
    state.base_uri = Some(unsafe { CStr::from_ptr(uri) }.to_owned());
    unsafe { ffi::g_free(uri.cast()) };
}

/// Set both DPI axes at once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_set_dpi(handle: *mut RsvgHandle, dpi: f64) {
    if let Some(state) = unsafe { state_of(handle) } {
        set_dpi_values(state, Some(dpi), Some(dpi));
    }
}

/// Set the DPI axes independently.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_set_dpi_x_y(handle: *mut RsvgHandle, dpi_x: f64, dpi_y: f64) {
    if let Some(state) = unsafe { state_of(handle) } {
        set_dpi_values(state, Some(dpi_x), Some(dpi_y));
    }
}

/// Inject a user-origin CSS stylesheet.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_set_stylesheet(
    handle: *mut RsvgHandle,
    css: *const u8,
    css_len: usize,
    error: *mut *mut GError,
) -> gboolean {
    let Some(state) = (unsafe { state_of(handle) }) else {
        gobject::set_gerror(error, "handle is NULL");
        return FALSE;
    };
    if css.is_null() && css_len != 0 {
        gobject::set_gerror(error, "css is NULL");
        return FALSE;
    }
    let bytes = if css.is_null() || css_len == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(css, css_len) }.to_vec()
    };
    if let Some(doc) = state.document()
        && unsafe { ngapi::glycin_ng_svg_set_stylesheet(doc, bytes.as_ptr(), bytes.len()) } != 0
    {
        gobject::set_gerror_from_engine(error);
        return FALSE;
    }
    state.stylesheet = Some(bytes);
    TRUE
}

/// Store a cancellable for rendering. Loading honors cancellables;
/// renders currently run to completion.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_set_cancellable_for_rendering(
    handle: *mut RsvgHandle,
    cancellable: *mut GCancellable,
) {
    if let Some(state) = unsafe { state_of(handle) } {
        state.rendering_cancellable = cancellable.cast();
    }
}

/// Whether the element `id` (with leading `#`) exists.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_has_sub(
    handle: *mut RsvgHandle,
    id: *const c_char,
) -> gboolean {
    let Some(state) = (unsafe { state_of(handle) }) else {
        return FALSE;
    };
    let Some(doc) = state.document() else {
        return FALSE;
    };
    let Some(id) = (unsafe { id_cstring(id) }) else {
        return FALSE;
    };
    unsafe { ngapi::glycin_ng_svg_has_element(doc, id.as_ptr()) }
}

/// Report the toplevel `width`/`height`/`viewBox` attributes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_get_intrinsic_dimensions(
    handle: *mut RsvgHandle,
    out_has_width: *mut gboolean,
    out_width: *mut RsvgLength,
    out_has_height: *mut gboolean,
    out_height: *mut RsvgLength,
    out_has_viewbox: *mut gboolean,
    out_viewbox: *mut RsvgRectangle,
) {
    let doc = unsafe { state_of(handle) }.and_then(|s| s.document());

    let (mut wv, mut wu, mut hv, mut hu) = (1.0f64, 0u32, 1.0f64, 0u32);
    let mut vb = [0.0f64; 4];
    let mut has_vb: c_int = 0;
    if let Some(doc) = doc {
        unsafe {
            ngapi::glycin_ng_svg_intrinsic_dimensions(
                doc,
                &mut wv,
                &mut wu,
                &mut hv,
                &mut hu,
                vb.as_mut_ptr(),
                &mut has_vb,
            )
        };
    }
    unsafe {
        // Since SVG2 both dimensions always exist, defaulting to 100%.
        if !out_has_width.is_null() {
            *out_has_width = TRUE;
        }
        if !out_has_height.is_null() {
            *out_has_height = TRUE;
        }
        if !out_width.is_null() {
            *out_width = RsvgLength {
                length: wv,
                unit: wu,
            };
        }
        if !out_height.is_null() {
            *out_height = RsvgLength {
                length: hv,
                unit: hu,
            };
        }
        if !out_has_viewbox.is_null() {
            *out_has_viewbox = if has_vb != 0 { TRUE } else { FALSE };
        }
        if !out_viewbox.is_null() {
            *out_viewbox = RsvgRectangle {
                x: vb[0],
                y: vb[1],
                width: vb[2],
                height: vb[3],
            };
        }
    }
}

/// Convert the intrinsic dimensions to pixels, when they are not
/// percentages.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_get_intrinsic_size_in_pixels(
    handle: *mut RsvgHandle,
    out_width: *mut f64,
    out_height: *mut f64,
) -> gboolean {
    let write = |w: f64, h: f64| unsafe {
        if !out_width.is_null() {
            *out_width = w;
        }
        if !out_height.is_null() {
            *out_height = h;
        }
    };
    let Some(state) = (unsafe { state_of(handle) }) else {
        write(0.0, 0.0);
        return FALSE;
    };
    let Some(doc) = state.document() else {
        write(0.0, 0.0);
        return FALSE;
    };
    let (mut wv, mut wu, mut hv, mut hu) = (1.0f64, 0u32, 1.0f64, 0u32);
    unsafe {
        ngapi::glycin_ng_svg_intrinsic_dimensions(
            doc,
            &mut wv,
            &mut wu,
            &mut hv,
            &mut hu,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    let dpi = state.effective_dpi();
    match (length_to_pixels(wv, wu, dpi), length_to_pixels(hv, hu, dpi)) {
        (Some(w), Some(h)) => {
            write(w, h);
            TRUE
        }
        _ => {
            write(0.0, 0.0);
            FALSE
        }
    }
}

/// Deprecated integer document size (size callback applied).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_get_dimensions(
    handle: *mut RsvgHandle,
    dimension_data: *mut RsvgDimensionData,
) {
    if dimension_data.is_null() {
        return;
    }
    let dims = match unsafe { state_of(handle) } {
        Some(state) => natural_dimensions(state),
        None => RsvgDimensionData::default(),
    };
    unsafe { *dimension_data = dims };
}

/// Deprecated per-element size query.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_get_dimensions_sub(
    handle: *mut RsvgHandle,
    dimension_data: *mut RsvgDimensionData,
    id: *const c_char,
) -> gboolean {
    if dimension_data.is_null() {
        return FALSE;
    }
    unsafe { *dimension_data = RsvgDimensionData::default() };
    let Some(state) = (unsafe { state_of(handle) }) else {
        return FALSE;
    };
    let Some(doc) = state.document() else {
        return FALSE;
    };
    if id.is_null() {
        // Whole-document query: same result as get_dimensions, computed
        // from the borrow already in hand.
        unsafe { *dimension_data = natural_dimensions(state) };
        return TRUE;
    }
    let Some(id) = (unsafe { id_cstring(id) }) else {
        return FALSE;
    };
    let mut ink = [0.0f64; 4];
    let rc = unsafe {
        ngapi::glycin_ng_svg_element_geometry(
            doc,
            id.as_ptr(),
            0,
            ink.as_mut_ptr(),
            ptr::null_mut(),
        )
    };
    if rc != 0 {
        return FALSE;
    }
    unsafe {
        *dimension_data = RsvgDimensionData {
            width: ink[2].ceil() as c_int,
            height: ink[3].ceil() as c_int,
            em: ink[2],
            ex: ink[3],
        };
    }
    TRUE
}

/// Deprecated per-element position query.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_get_position_sub(
    handle: *mut RsvgHandle,
    position_data: *mut RsvgPositionData,
    id: *const c_char,
) -> gboolean {
    if position_data.is_null() {
        return FALSE;
    }
    unsafe { *position_data = RsvgPositionData::default() };
    let Some(state) = (unsafe { state_of(handle) }) else {
        return FALSE;
    };
    let Some(doc) = state.document() else {
        return FALSE;
    };
    // The whole document is always at the origin.
    if id.is_null() {
        return TRUE;
    }
    let Some(id) = (unsafe { id_cstring(id) }) else {
        return FALSE;
    };
    let mut ink = [0.0f64; 4];
    let rc = unsafe {
        ngapi::glycin_ng_svg_element_geometry(
            doc,
            id.as_ptr(),
            0,
            ink.as_mut_ptr(),
            ptr::null_mut(),
        )
    };
    if rc != 0 {
        return FALSE;
    }
    unsafe {
        *position_data = RsvgPositionData {
            x: ink[0].round() as c_int,
            y: ink[1].round() as c_int,
        };
    }
    TRUE
}

/// Deprecated sizing-callback override.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_set_size_callback(
    handle: *mut RsvgHandle,
    size_func: RsvgSizeFunc,
    user_data: gpointer,
    user_data_destroy: ffi::GDestroyNotify,
) {
    if let Some(state) = unsafe { state_of(handle) } {
        state.size_callback = SizeCallback {
            func: size_func,
            data: user_data,
            destroy: user_data_destroy,
        };
    }
}

/// Deprecated; always NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_get_title(_handle: *mut RsvgHandle) -> *const c_char {
    ptr::null()
}

/// Deprecated; always NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_get_desc(_handle: *mut RsvgHandle) -> *const c_char {
    ptr::null()
}

/// Deprecated; always NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_get_metadata(_handle: *mut RsvgHandle) -> *const c_char {
    ptr::null()
}

/// Internal test hook; accepted and ignored.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_internal_set_testing(
    _handle: *mut RsvgHandle,
    _testing: gboolean,
) {
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_cstring_strips_the_hash() {
        unsafe {
            assert!(id_cstring(ptr::null()).is_none());
            assert_eq!(id_cstring(c"#foo".as_ptr()).unwrap().as_c_str(), c"foo");
            assert_eq!(id_cstring(c"bar".as_ptr()).unwrap().as_c_str(), c"bar");
        }
    }
}
