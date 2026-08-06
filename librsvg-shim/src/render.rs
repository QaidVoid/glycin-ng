//! `rsvg_handle_render_*`: painting engine pixmaps onto a caller's
//! cairo context.
//!
//! resvg produces pixels while librsvg draws vectors, so the shim
//! renders at the device resolution of the target context (derived
//! from its current transformation matrix) and paints the result
//! under an inverse scale. Screen output stays sharp at any zoom;
//! vector surfaces (PDF/PS/SVG) receive a raster at device
//! resolution instead of vectors, which is this shim's documented
//! trade-off.

use std::ffi::{CString, c_char, c_int};
use std::ptr;

use crate::cairo_ffi::{self as cairo, cairo_matrix_t, cairo_t};
use crate::ffi::{GError, gboolean};
use crate::gobject::{self, RsvgHandle, state_of};
use crate::handle::{self, FALSE, TRUE, id_cstring};
use crate::ngapi;
use crate::state::{HandleState, RsvgRectangle, fit_document};

/// Hard clamp for pixmap dimensions; cairo image surfaces cap at
/// 32767 per axis.
const MAX_PIXMAP_DIM: f64 = 16384.0;

struct Paint {
    /// Engine target: element id plus mode, or whole document.
    id: Option<CString>,
    element_mode: bool,
    /// Document-space to user-space scale.
    content_scale: (f64, f64),
    /// User-space size of the painted region.
    content_size: (f64, f64),
    /// User-space position of the painted region.
    dest: (f64, f64),
}

/// Render through the engine and paint onto `cr`. Returns a
/// descriptive error message on failure.
unsafe fn render_and_paint(
    state: &HandleState,
    cr: *mut cairo_t,
    paint: &Paint,
) -> Result<(), String> {
    let Some(doc) = state.document() else {
        return Err("handle is not fully loaded".into());
    };
    if unsafe { cairo::cairo_status(cr) } != cairo::CAIRO_STATUS_SUCCESS {
        return Err("cairo context is in an error state".into());
    }
    let (cw, ch) = paint.content_size;
    if cw <= 0.0 || ch <= 0.0 {
        return Ok(());
    }

    // Device scale from the context's CTM keeps output sharp when
    // the caller has zoomed.
    let mut m = cairo_matrix_t::default();
    unsafe { cairo::cairo_get_matrix(cr, &mut m) };
    let dev_sx = (m.xx * m.xx + m.yx * m.yx).sqrt().max(f64::MIN_POSITIVE);
    let dev_sy = (m.xy * m.xy + m.yy * m.yy).sqrt().max(f64::MIN_POSITIVE);

    let pw = (cw * dev_sx).ceil().clamp(1.0, MAX_PIXMAP_DIM) as u32;
    let ph = (ch * dev_sy).ceil().clamp(1.0, MAX_PIXMAP_DIM) as u32;
    // Exact user-to-pixel factors after rounding.
    let px_sx = pw as f64 / cw;
    let px_sy = ph as f64 / ch;

    let transform = [
        paint.content_scale.0 * px_sx,
        0.0,
        0.0,
        paint.content_scale.1 * px_sy,
        0.0,
        0.0,
    ];
    let id_ptr: *const c_char = paint.id.as_ref().map(|s| s.as_ptr()).unwrap_or(ptr::null());
    let render = unsafe {
        ngapi::glycin_ng_svg_render(
            doc,
            id_ptr,
            paint.element_mode as c_int,
            pw,
            ph,
            transform.as_ptr(),
            0,
        )
    };
    if render.is_null() {
        let msg = unsafe { ngapi::glycin_ng_last_error() };
        return Err(if msg.is_null() {
            "rendering failed".into()
        } else {
            unsafe { std::ffi::CStr::from_ptr(msg) }
                .to_string_lossy()
                .into_owned()
        });
    }
    let len = unsafe { ngapi::glycin_ng_svg_render_len(render) };
    let data = unsafe { ngapi::glycin_ng_svg_render_data(render) };
    let result = unsafe {
        paint_pixels(
            cr,
            std::slice::from_raw_parts(data, len),
            pw,
            ph,
            paint.dest,
            (cw / pw as f64, ch / ph as f64),
        )
    };
    unsafe { ngapi::glycin_ng_svg_render_free(render) };
    result
}

/// Copy premultiplied RGBA pixels into a `CAIRO_FORMAT_ARGB32`
/// surface and paint it at `dest` under `inv_scale`.
unsafe fn paint_pixels(
    cr: *mut cairo_t,
    rgba: &[u8],
    width: u32,
    height: u32,
    dest: (f64, f64),
    inv_scale: (f64, f64),
) -> Result<(), String> {
    if rgba.len() < (width as usize) * (height as usize) * 4 {
        return Err("engine returned a short pixel buffer".into());
    }
    let surface = unsafe {
        cairo::cairo_image_surface_create(
            cairo::CAIRO_FORMAT_ARGB32,
            width as c_int,
            height as c_int,
        )
    };
    if unsafe { cairo::cairo_surface_status(surface) } != cairo::CAIRO_STATUS_SUCCESS {
        unsafe { cairo::cairo_surface_destroy(surface) };
        return Err("failed to create cairo image surface".into());
    }
    unsafe {
        cairo::cairo_surface_flush(surface);
        let dst = cairo::cairo_image_surface_get_data(surface);
        let stride = cairo::cairo_image_surface_get_stride(surface) as usize;
        for row in 0..height as usize {
            let src = &rgba[row * width as usize * 4..][..width as usize * 4];
            let out = dst.add(row * stride);
            for (col, px) in src.as_chunks::<4>().0.iter().enumerate() {
                // Premultiplied RGBA bytes to little-endian ARGB32
                // (B, G, R, A in memory).
                let out = out.add(col * 4);
                *out = px[2];
                *out.add(1) = px[1];
                *out.add(2) = px[0];
                *out.add(3) = px[3];
            }
        }
        cairo::cairo_surface_mark_dirty(surface);

        cairo::cairo_save(cr);
        cairo::cairo_translate(cr, dest.0, dest.1);
        cairo::cairo_scale(cr, inv_scale.0, inv_scale.1);
        cairo::cairo_set_source_surface(cr, surface, 0.0, 0.0);
        cairo::cairo_paint(cr);
        cairo::cairo_restore(cr);
        cairo::cairo_surface_destroy(surface);
    }
    Ok(())
}

/// Fit the loaded document into `viewport` and describe the painted
/// region.
fn document_paint(
    state: &HandleState,
    id: Option<CString>,
    viewport: &RsvgRectangle,
) -> Result<Paint, String> {
    let (dw, dh) = handle::doc_size(state).ok_or("handle is not fully loaded")?;
    let fit = fit_document(dw, dh, viewport).ok_or("viewport or document is empty")?;
    Ok(Paint {
        id,
        element_mode: false,
        content_scale: (fit.scale, fit.scale),
        content_size: (dw * fit.scale, dh * fit.scale),
        dest: (fit.offset_x, fit.offset_y),
    })
}

/// Render the whole document into `viewport`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_render_document(
    handle: *mut RsvgHandle,
    cr: *mut cairo_t,
    viewport: *const RsvgRectangle,
    error: *mut *mut GError,
) -> gboolean {
    unsafe { render_layer_impl(handle, cr, None, viewport, error) }
}

/// Render a single element (or the whole document) in its
/// in-document place, fitted to `viewport`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_render_layer(
    handle: *mut RsvgHandle,
    cr: *mut cairo_t,
    id: *const c_char,
    viewport: *const RsvgRectangle,
    error: *mut *mut GError,
) -> gboolean {
    unsafe {
        let id = id_cstring(id);
        render_layer_impl(handle, cr, id, viewport, error)
    }
}

unsafe fn render_layer_impl(
    handle: *mut RsvgHandle,
    cr: *mut cairo_t,
    id: Option<CString>,
    viewport: *const RsvgRectangle,
    error: *mut *mut GError,
) -> gboolean {
    let Some(state) = (unsafe { state_of(handle) }) else {
        gobject::set_gerror(error, "handle is NULL");
        return FALSE;
    };
    if cr.is_null() {
        gobject::set_gerror(error, "cairo context is NULL");
        return FALSE;
    }
    if viewport.is_null() {
        gobject::set_gerror(error, "viewport is NULL");
        return FALSE;
    }
    let viewport = unsafe { *viewport };
    let paint = match document_paint(state, id, &viewport) {
        Ok(p) => p,
        Err(msg) => {
            gobject::set_gerror(error, &msg);
            return FALSE;
        }
    };
    match unsafe { render_and_paint(state, cr, &paint) } {
        Ok(()) => TRUE,
        Err(msg) => {
            gobject::set_gerror(error, &msg);
            FALSE
        }
    }
}

/// Render a single element extracted by itself, scaled into
/// `element_viewport`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_render_element(
    handle: *mut RsvgHandle,
    cr: *mut cairo_t,
    id: *const c_char,
    element_viewport: *const RsvgRectangle,
    error: *mut *mut GError,
) -> gboolean {
    // A NULL id means the whole document. Dispatched before taking the
    // state borrow so the delegate does not alias it.
    let Some(id) = (unsafe { id_cstring(id) }) else {
        return unsafe { rsvg_handle_render_document(handle, cr, element_viewport, error) };
    };
    let Some(state) = (unsafe { state_of(handle) }) else {
        gobject::set_gerror(error, "handle is NULL");
        return FALSE;
    };
    if cr.is_null() {
        gobject::set_gerror(error, "cairo context is NULL");
        return FALSE;
    }
    if element_viewport.is_null() {
        gobject::set_gerror(error, "element_viewport is NULL");
        return FALSE;
    }
    let viewport = unsafe { *element_viewport };
    let Some(doc) = state.document() else {
        gobject::set_gerror(error, "handle is not fully loaded");
        return FALSE;
    };
    let mut ink = [0.0f64; 4];
    let rc = unsafe {
        ngapi::glycin_ng_svg_element_geometry(
            doc,
            id.as_ptr(),
            1,
            ink.as_mut_ptr(),
            ptr::null_mut(),
        )
    };
    if rc != 0 {
        gobject::set_gerror_from_engine(error);
        return FALSE;
    }
    let (ew, eh) = (ink[2], ink[3]);
    if ew <= 0.0 || eh <= 0.0 {
        return TRUE;
    }
    // librsvg scales the element uniformly to fit and anchors it at
    // the viewport origin.
    let factor = (viewport.width / ew).min(viewport.height / eh);
    if factor <= 0.0 {
        return TRUE;
    }
    let paint = Paint {
        id: Some(id),
        element_mode: true,
        content_scale: (factor, factor),
        content_size: (ew * factor, eh * factor),
        dest: (viewport.x, viewport.y),
    };
    match unsafe { render_and_paint(state, cr, &paint) } {
        Ok(()) => TRUE,
        Err(msg) => {
            gobject::set_gerror(error, &msg);
            FALSE
        }
    }
}

/// Deprecated whole-document render at the natural size.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_render_cairo(
    handle: *mut RsvgHandle,
    cr: *mut cairo_t,
) -> gboolean {
    unsafe { rsvg_handle_render_cairo_sub(handle, cr, ptr::null()) }
}

/// Deprecated single-element render at the natural document size.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_render_cairo_sub(
    handle: *mut RsvgHandle,
    cr: *mut cairo_t,
    id: *const c_char,
) -> gboolean {
    let Some(state) = (unsafe { state_of(handle) }) else {
        return FALSE;
    };
    let dims = handle::natural_dimensions(state);
    if dims.width <= 0 || dims.height <= 0 {
        return FALSE;
    }
    let viewport = RsvgRectangle {
        x: 0.0,
        y: 0.0,
        width: dims.width as f64,
        height: dims.height as f64,
    };
    unsafe {
        let id = id_cstring(id);
        render_layer_impl(handle, cr, id, &viewport, ptr::null_mut())
    }
}

/// Geometry of an element (or the document) as rendered into
/// `viewport`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_get_geometry_for_layer(
    handle: *mut RsvgHandle,
    id: *const c_char,
    viewport: *const RsvgRectangle,
    out_ink_rect: *mut RsvgRectangle,
    out_logical_rect: *mut RsvgRectangle,
    error: *mut *mut GError,
) -> gboolean {
    let Some(state) = (unsafe { state_of(handle) }) else {
        gobject::set_gerror(error, "handle is NULL");
        return FALSE;
    };
    if viewport.is_null() {
        gobject::set_gerror(error, "viewport is NULL");
        return FALSE;
    }
    let viewport = unsafe { *viewport };
    let Some(doc) = state.document() else {
        gobject::set_gerror(error, "handle is not fully loaded");
        return FALSE;
    };
    let Some((dw, dh)) = handle::doc_size(state) else {
        gobject::set_gerror(error, "handle is not fully loaded");
        return FALSE;
    };
    let Some(fit) = fit_document(dw, dh, &viewport) else {
        gobject::set_gerror(error, "viewport or document is empty");
        return FALSE;
    };
    let id = unsafe { id_cstring(id) };
    let id_ptr: *const c_char = id.as_ref().map(|s| s.as_ptr()).unwrap_or(ptr::null());
    let mut ink = [0.0f64; 4];
    let mut logical = [0.0f64; 4];
    let rc = unsafe {
        ngapi::glycin_ng_svg_element_geometry(
            doc,
            id_ptr,
            0,
            ink.as_mut_ptr(),
            logical.as_mut_ptr(),
        )
    };
    if rc != 0 {
        gobject::set_gerror_from_engine(error);
        return FALSE;
    }
    let map = |r: [f64; 4]| RsvgRectangle {
        x: fit.offset_x + r[0] * fit.scale,
        y: fit.offset_y + r[1] * fit.scale,
        width: r[2] * fit.scale,
        height: r[3] * fit.scale,
    };
    unsafe {
        if !out_ink_rect.is_null() {
            *out_ink_rect = map(ink);
        }
        if !out_logical_rect.is_null() {
            *out_logical_rect = map(logical);
        }
    }
    TRUE
}

/// Geometry of an element as if extracted by itself.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsvg_handle_get_geometry_for_element(
    handle: *mut RsvgHandle,
    id: *const c_char,
    out_ink_rect: *mut RsvgRectangle,
    out_logical_rect: *mut RsvgRectangle,
    error: *mut *mut GError,
) -> gboolean {
    let Some(state) = (unsafe { state_of(handle) }) else {
        gobject::set_gerror(error, "handle is NULL");
        return FALSE;
    };
    let Some(doc) = state.document() else {
        gobject::set_gerror(error, "handle is not fully loaded");
        return FALSE;
    };
    let id = unsafe { id_cstring(id) };
    let id_ptr: *const c_char = id.as_ref().map(|s| s.as_ptr()).unwrap_or(ptr::null());
    let mut ink = [0.0f64; 4];
    let mut logical = [0.0f64; 4];
    let rc = unsafe {
        ngapi::glycin_ng_svg_element_geometry(
            doc,
            id_ptr,
            1,
            ink.as_mut_ptr(),
            logical.as_mut_ptr(),
        )
    };
    if rc != 0 {
        gobject::set_gerror_from_engine(error);
        return FALSE;
    }
    let to_rect = |r: [f64; 4]| RsvgRectangle {
        x: r[0],
        y: r[1],
        width: r[2],
        height: r[3],
    };
    unsafe {
        if !out_ink_rect.is_null() {
            *out_ink_rect = to_rect(ink);
        }
        if !out_logical_rect.is_null() {
            *out_logical_rect = to_rect(logical);
        }
    }
    TRUE
}
