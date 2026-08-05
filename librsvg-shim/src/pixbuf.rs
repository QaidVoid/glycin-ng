//! `rsvg_*pixbuf*`: GdkPixbuf producers, including the four symbols
//! the gdk-pixbuf SVG loader module uses.

use std::ffi::{CString, c_char, c_int};
use std::ptr;

use crate::ffi::{self, GError, gpointer};
use crate::gobject::{self, RsvgHandle, state_of};
use crate::handle::{self, id_cstring, rsvg_handle_free, rsvg_handle_new_from_file};
use crate::ngapi;
use crate::pixbuf_ffi::{self, GdkPixbuf};
use crate::state::HandleState;

unsafe extern "C" fn free_pixels(pixels: *mut u8, _data: gpointer) {
    unsafe { ffi::g_free(pixels.cast()) };
}

/// Render at `width` x `height` (vector scale from the document
/// size) and wrap the pixels in a new `GdkPixbuf`.
unsafe fn render_pixbuf(
    state: &HandleState,
    id: Option<CString>,
    width: c_int,
    height: c_int,
    error: *mut *mut GError,
) -> *mut GdkPixbuf {
    let Some(doc) = state.document() else {
        gobject::set_gerror(error, "handle is not fully loaded");
        return ptr::null_mut();
    };
    let Some((dw, dh)) = handle::doc_size(state) else {
        gobject::set_gerror(error, "handle is not fully loaded");
        return ptr::null_mut();
    };
    if width <= 0 || height <= 0 || dw <= 0.0 || dh <= 0.0 {
        gobject::set_gerror(error, "document or target size is empty");
        return ptr::null_mut();
    }
    let transform = [width as f64 / dw, 0.0, 0.0, height as f64 / dh, 0.0, 0.0];
    let id_ptr: *const c_char = id.as_ref().map(|s| s.as_ptr()).unwrap_or(ptr::null());
    let render = unsafe {
        ngapi::glycin_ng_svg_render(
            doc,
            id_ptr,
            0,
            width as u32,
            height as u32,
            transform.as_ptr(),
            1,
        )
    };
    if render.is_null() {
        gobject::set_gerror_from_engine(error);
        return ptr::null_mut();
    }
    let len = unsafe { ngapi::glycin_ng_svg_render_len(render) };
    let src = unsafe { ngapi::glycin_ng_svg_render_data(render) };
    let pixels = unsafe { ffi::g_malloc(len) };
    if pixels.is_null() {
        unsafe { ngapi::glycin_ng_svg_render_free(render) };
        gobject::set_gerror(error, "out of memory");
        return ptr::null_mut();
    }
    unsafe {
        ptr::copy_nonoverlapping(src, pixels.cast::<u8>(), len);
        ngapi::glycin_ng_svg_render_free(render);
    }
    let pixbuf = unsafe {
        pixbuf_ffi::gdk_pixbuf_new_from_data(
            pixels.cast(),
            pixbuf_ffi::GDK_COLORSPACE_RGB,
            1,
            8,
            width,
            height,
            width * 4,
            Some(free_pixels),
            ptr::null_mut(),
        )
    };
    if pixbuf.is_null() {
        unsafe { ffi::g_free(pixels) };
        gobject::set_gerror(error, "gdk_pixbuf_new_from_data failed");
    }
    pixbuf
}

unsafe fn handle_pixbuf(
    handle: *mut RsvgHandle,
    id: Option<CString>,
    error: *mut *mut GError,
) -> *mut GdkPixbuf {
    let Some(state) = (unsafe { state_of(handle) }) else {
        gobject::set_gerror(error, "handle is NULL");
        return ptr::null_mut();
    };
    let dims = handle::natural_dimensions(state);
    unsafe { render_pixbuf(state, id, dims.width, dims.height, error) }
}

/// Render the document at its natural size (size callback applied)
/// into a new pixbuf.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_get_pixbuf_and_error(
    handle: *mut RsvgHandle,
    error: *mut *mut GError,
) -> *mut GdkPixbuf {
    unsafe { handle_pixbuf(handle, None, error) }
}

/// Deprecated variant of `rsvg_handle_get_pixbuf_and_error`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_get_pixbuf(handle: *mut RsvgHandle) -> *mut GdkPixbuf {
    unsafe { handle_pixbuf(handle, None, ptr::null_mut()) }
}

/// Document-sized pixbuf with only the `id` subtree rendered.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_get_pixbuf_sub(
    handle: *mut RsvgHandle,
    id: *const c_char,
) -> *mut GdkPixbuf {
    unsafe {
        let id = id_cstring(id);
        handle_pixbuf(handle, id, ptr::null_mut())
    }
}

/// Load `filename` and render a pixbuf sized by `resize`, which maps
/// the natural document size to the output size.
unsafe fn pixbuf_from_file_impl(
    filename: *const c_char,
    error: *mut *mut GError,
    resize: impl FnOnce(f64, f64) -> (f64, f64),
) -> *mut GdkPixbuf {
    let handle = unsafe { rsvg_handle_new_from_file(filename, error) };
    if handle.is_null() {
        return ptr::null_mut();
    }
    let pixbuf = match unsafe { state_of(handle) } {
        None => ptr::null_mut(),
        Some(state) => match handle::doc_size(state) {
            None => {
                gobject::set_gerror(error, "handle is not fully loaded");
                ptr::null_mut()
            }
            Some((dw, dh)) => {
                let (w, h) = resize(dw, dh);
                let w = w.ceil().max(1.0) as c_int;
                let h = h.ceil().max(1.0) as c_int;
                unsafe { render_pixbuf(state, None, w, h, error) }
            }
        },
    };
    unsafe { rsvg_handle_free(handle) };
    pixbuf
}

/// Shrink `(w, h)` uniformly so it fits within `max_w` x `max_h`.
fn cap_to_max(w: f64, h: f64, max_w: c_int, max_h: c_int) -> (f64, f64) {
    if w <= 0.0 || h <= 0.0 {
        return (w, h);
    }
    let factor = (max_w as f64 / w).min(max_h as f64 / h).min(1.0);
    (w * factor, h * factor)
}

/// Deprecated: load and render at natural size.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_pixbuf_from_file(
    filename: *const c_char,
    error: *mut *mut GError,
) -> *mut GdkPixbuf {
    unsafe { pixbuf_from_file_impl(filename, error, |w, h| (w, h)) }
}

/// Deprecated: load and render zoomed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_pixbuf_from_file_at_zoom(
    filename: *const c_char,
    x_zoom: f64,
    y_zoom: f64,
    error: *mut *mut GError,
) -> *mut GdkPixbuf {
    unsafe { pixbuf_from_file_impl(filename, error, |w, h| (w * x_zoom, h * y_zoom)) }
}

/// Deprecated: load and render at a fixed size (-1 keeps the
/// natural value).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_pixbuf_from_file_at_size(
    filename: *const c_char,
    width: c_int,
    height: c_int,
    error: *mut *mut GError,
) -> *mut GdkPixbuf {
    unsafe {
        pixbuf_from_file_impl(filename, error, |w, h| {
            (
                if width == -1 { w } else { width as f64 },
                if height == -1 { h } else { height as f64 },
            )
        })
    }
}

/// Deprecated: load and shrink uniformly to fit.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_pixbuf_from_file_at_max_size(
    filename: *const c_char,
    max_width: c_int,
    max_height: c_int,
    error: *mut *mut GError,
) -> *mut GdkPixbuf {
    unsafe {
        pixbuf_from_file_impl(filename, error, |w, h| {
            cap_to_max(w, h, max_width, max_height)
        })
    }
}

/// Deprecated: load, zoom, then shrink uniformly to fit.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_pixbuf_from_file_at_zoom_with_max(
    filename: *const c_char,
    x_zoom: f64,
    y_zoom: f64,
    max_width: c_int,
    max_height: c_int,
    error: *mut *mut GError,
) -> *mut GdkPixbuf {
    unsafe {
        pixbuf_from_file_impl(filename, error, |w, h| {
            cap_to_max(w * x_zoom, h * y_zoom, max_width, max_height)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_to_max_only_shrinks() {
        assert_eq!(cap_to_max(100.0, 50.0, 50, 50), (50.0, 25.0));
        assert_eq!(cap_to_max(10.0, 5.0, 50, 50), (10.0, 5.0));
        assert_eq!(cap_to_max(100.0, 100.0, 25, 50), (25.0, 25.0));
    }
}
