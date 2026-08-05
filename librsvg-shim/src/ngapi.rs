//! The `glycin_ng_svg_*` engine C ABI this shim forwards to,
//! declared by hand from `include/glycin_ng.h`. Resolved at link
//! time against `libglycin_ng.so`.

use std::ffi::{c_char, c_double, c_int, c_uint};

#[repr(C)]
pub struct GlycinNgSvg {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GlycinNgSvgRender {
    _private: [u8; 0],
}

#[cfg(not(test))]
#[allow(dead_code)]
unsafe extern "C" {
    pub fn glycin_ng_last_error() -> *const c_char;

    pub fn glycin_ng_svg_new(
        data: *const u8,
        len: usize,
        dpi: c_double,
        resources_dir: *const c_char,
        system_fonts: c_int,
    ) -> *mut GlycinNgSvg;
    pub fn glycin_ng_svg_free(svg: *mut GlycinNgSvg);
    pub fn glycin_ng_svg_set_stylesheet(svg: *mut GlycinNgSvg, css: *const u8, len: usize)
    -> c_int;
    pub fn glycin_ng_svg_set_dpi(svg: *mut GlycinNgSvg, dpi: c_double) -> c_int;
    pub fn glycin_ng_svg_size(
        svg: *const GlycinNgSvg,
        width: *mut c_double,
        height: *mut c_double,
    ) -> c_int;
    pub fn glycin_ng_svg_intrinsic_dimensions(
        svg: *const GlycinNgSvg,
        width_value: *mut c_double,
        width_unit: *mut c_uint,
        height_value: *mut c_double,
        height_unit: *mut c_uint,
        viewbox: *mut c_double,
        has_viewbox: *mut c_int,
    ) -> c_int;
    pub fn glycin_ng_svg_has_element(svg: *const GlycinNgSvg, id: *const c_char) -> c_int;
    pub fn glycin_ng_svg_element_geometry(
        svg: *const GlycinNgSvg,
        id: *const c_char,
        element_mode: c_int,
        ink: *mut c_double,
        logical: *mut c_double,
    ) -> c_int;
    pub fn glycin_ng_svg_render(
        svg: *const GlycinNgSvg,
        id: *const c_char,
        element_mode: c_int,
        width: u32,
        height: u32,
        transform: *const c_double,
        unpremultiply: c_int,
    ) -> *mut GlycinNgSvgRender;
    pub fn glycin_ng_svg_render_free(render: *mut GlycinNgSvgRender);
    pub fn glycin_ng_svg_render_data(render: *const GlycinNgSvgRender) -> *const u8;
    pub fn glycin_ng_svg_render_len(render: *const GlycinNgSvgRender) -> usize;
}

#[cfg(test)]
pub use test_stubs::*;

#[cfg(test)]
#[allow(dead_code)]
mod test_stubs {
    use super::*;

    pub unsafe extern "C" fn glycin_ng_last_error() -> *const c_char {
        std::ptr::null()
    }
    pub unsafe extern "C" fn glycin_ng_svg_new(
        _: *const u8,
        _: usize,
        _: c_double,
        _: *const c_char,
        _: c_int,
    ) -> *mut GlycinNgSvg {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn glycin_ng_svg_free(_: *mut GlycinNgSvg) {}
    pub unsafe extern "C" fn glycin_ng_svg_set_stylesheet(
        _: *mut GlycinNgSvg,
        _: *const u8,
        _: usize,
    ) -> c_int {
        0
    }
    pub unsafe extern "C" fn glycin_ng_svg_set_dpi(_: *mut GlycinNgSvg, _: c_double) -> c_int {
        0
    }
    pub unsafe extern "C" fn glycin_ng_svg_size(
        _: *const GlycinNgSvg,
        w: *mut c_double,
        h: *mut c_double,
    ) -> c_int {
        unsafe {
            *w = 20.0;
            *h = 10.0;
        }
        0
    }
    pub unsafe extern "C" fn glycin_ng_svg_intrinsic_dimensions(
        _: *const GlycinNgSvg,
        _: *mut c_double,
        _: *mut c_uint,
        _: *mut c_double,
        _: *mut c_uint,
        _: *mut c_double,
        _: *mut c_int,
    ) -> c_int {
        0
    }
    pub unsafe extern "C" fn glycin_ng_svg_has_element(
        _: *const GlycinNgSvg,
        _: *const c_char,
    ) -> c_int {
        0
    }
    pub unsafe extern "C" fn glycin_ng_svg_element_geometry(
        _: *const GlycinNgSvg,
        _: *const c_char,
        _: c_int,
        _: *mut c_double,
        _: *mut c_double,
    ) -> c_int {
        -1
    }
    pub unsafe extern "C" fn glycin_ng_svg_render(
        _: *const GlycinNgSvg,
        _: *const c_char,
        _: c_int,
        _: u32,
        _: u32,
        _: *const c_double,
        _: c_int,
    ) -> *mut GlycinNgSvgRender {
        std::ptr::null_mut()
    }
    pub unsafe extern "C" fn glycin_ng_svg_render_free(_: *mut GlycinNgSvgRender) {}
    pub unsafe extern "C" fn glycin_ng_svg_render_data(_: *const GlycinNgSvgRender) -> *const u8 {
        std::ptr::null()
    }
    pub unsafe extern "C" fn glycin_ng_svg_render_len(_: *const GlycinNgSvgRender) -> usize {
        0
    }
}
