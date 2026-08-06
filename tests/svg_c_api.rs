//! FFI smoke tests for the persistent SVG document C API.

#![cfg(all(feature = "c-api", feature = "svg"))]

use std::ffi::{CStr, c_char};
use std::ptr;

use glycin_ng::c_api::svg::{GlycinNgSvg, GlycinNgSvgRender};

#[allow(improper_ctypes)]
unsafe extern "C" {
    fn glycin_ng_last_error() -> *const c_char;
    fn glycin_ng_svg_new(
        data: *const u8,
        len: usize,
        dpi: f64,
        resources_dir: *const c_char,
        system_fonts: i32,
    ) -> *mut GlycinNgSvg;
    fn glycin_ng_svg_free(svg: *mut GlycinNgSvg);
    fn glycin_ng_svg_set_stylesheet(svg: *mut GlycinNgSvg, css: *const u8, len: usize) -> i32;
    fn glycin_ng_svg_set_dpi(svg: *mut GlycinNgSvg, dpi: f64) -> i32;
    fn glycin_ng_svg_size(svg: *const GlycinNgSvg, width: *mut f64, height: *mut f64) -> i32;
    fn glycin_ng_svg_intrinsic_dimensions(
        svg: *const GlycinNgSvg,
        width_value: *mut f64,
        width_unit: *mut u32,
        height_value: *mut f64,
        height_unit: *mut u32,
        viewbox: *mut f64,
        has_viewbox: *mut i32,
    ) -> i32;
    fn glycin_ng_svg_has_element(svg: *const GlycinNgSvg, id: *const c_char) -> i32;
    fn glycin_ng_svg_element_geometry(
        svg: *const GlycinNgSvg,
        id: *const c_char,
        element_mode: i32,
        ink: *mut f64,
        logical: *mut f64,
    ) -> i32;
    fn glycin_ng_svg_render(
        svg: *const GlycinNgSvg,
        id: *const c_char,
        element_mode: i32,
        width: u32,
        height: u32,
        transform: *const f64,
        unpremultiply: i32,
    ) -> *mut GlycinNgSvgRender;
    fn glycin_ng_svg_render_free(render: *mut GlycinNgSvgRender);
    fn glycin_ng_svg_render_data(render: *const GlycinNgSvgRender) -> *const u8;
    fn glycin_ng_svg_render_len(render: *const GlycinNgSvgRender) -> usize;
}

const TWO_RECTS: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10">
  <rect id="left" x="0" y="0" width="10" height="10" fill="red"/>
  <rect id="right" x="10" y="0" width="10" height="10" fill="blue"/>
</svg>"#;

fn new_two_rects() -> *mut GlycinNgSvg {
    let svg =
        unsafe { glycin_ng_svg_new(TWO_RECTS.as_ptr(), TWO_RECTS.len(), 0.0, ptr::null(), 0) };
    assert!(!svg.is_null(), "parse failed: {}", last_error());
    svg
}

fn last_error() -> String {
    let p = unsafe { glycin_ng_last_error() };
    if p.is_null() {
        "<no error>".into()
    } else {
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

#[test]
fn parse_size_and_elements() {
    let svg = new_two_rects();
    let (mut w, mut h) = (0.0f64, 0.0f64);
    assert_eq!(unsafe { glycin_ng_svg_size(svg, &mut w, &mut h) }, 0);
    assert_eq!((w, h), (20.0, 10.0));

    assert_eq!(
        unsafe { glycin_ng_svg_has_element(svg, c"left".as_ptr()) },
        1
    );
    assert_eq!(
        unsafe { glycin_ng_svg_has_element(svg, c"nope".as_ptr()) },
        0
    );
    assert_eq!(unsafe { glycin_ng_svg_has_element(svg, ptr::null()) }, 0);
    unsafe { glycin_ng_svg_free(svg) };
}

#[test]
fn rejects_garbage() {
    let junk = b"<svg garbage>not svg";
    let svg = unsafe { glycin_ng_svg_new(junk.as_ptr(), junk.len(), 0.0, ptr::null(), 0) };
    assert!(svg.is_null());
    assert_ne!(last_error(), "<no error>");
}

#[test]
fn intrinsic_dimensions_report_units() {
    let doc = br#"<svg xmlns="http://www.w3.org/2000/svg" width="2in" height="50%" viewBox="0 0 10 20"/>"#;
    let svg = unsafe { glycin_ng_svg_new(doc.as_ptr(), doc.len(), 0.0, ptr::null(), 0) };
    assert!(!svg.is_null(), "{}", last_error());

    let (mut wv, mut wu, mut hv, mut hu) = (0.0f64, 99u32, 0.0f64, 99u32);
    let mut vb = [0.0f64; 4];
    let mut has_vb = 0i32;
    let rc = unsafe {
        glycin_ng_svg_intrinsic_dimensions(
            svg,
            &mut wv,
            &mut wu,
            &mut hv,
            &mut hu,
            vb.as_mut_ptr(),
            &mut has_vb,
        )
    };
    assert_eq!(rc, 0);
    assert_eq!((wv, wu), (2.0, 4)); // 2in
    assert_eq!((hv, hu), (0.5, 0)); // 50%
    assert_eq!(has_vb, 1);
    assert_eq!(vb, [0.0, 0.0, 10.0, 20.0]);
    unsafe { glycin_ng_svg_free(svg) };
}

#[test]
fn geometry_layer_and_element() {
    let svg = new_two_rects();
    let mut ink = [0.0f64; 4];
    let mut logical = [0.0f64; 4];
    let rc = unsafe {
        glycin_ng_svg_element_geometry(
            svg,
            c"right".as_ptr(),
            0,
            ink.as_mut_ptr(),
            logical.as_mut_ptr(),
        )
    };
    assert_eq!(rc, 0);
    assert_eq!(ink, [10.0, 0.0, 10.0, 10.0]);

    let rc = unsafe {
        glycin_ng_svg_element_geometry(
            svg,
            c"right".as_ptr(),
            1,
            ink.as_mut_ptr(),
            logical.as_mut_ptr(),
        )
    };
    assert_eq!(rc, 0);
    assert_eq!(ink, [0.0, 0.0, 10.0, 10.0]);

    let rc = unsafe {
        glycin_ng_svg_element_geometry(
            svg,
            c"missing".as_ptr(),
            0,
            ink.as_mut_ptr(),
            logical.as_mut_ptr(),
        )
    };
    assert_eq!(rc, -1);
    unsafe { glycin_ng_svg_free(svg) };
}

#[test]
fn renders_document_layer_and_element() {
    let svg = new_two_rects();

    // Whole document at 2x via the transform.
    let ts = [2.0, 0.0, 0.0, 2.0, 0.0, 0.0];
    let render = unsafe { glycin_ng_svg_render(svg, ptr::null(), 0, 40, 20, ts.as_ptr(), 1) };
    assert!(!render.is_null(), "{}", last_error());
    let len = unsafe { glycin_ng_svg_render_len(render) };
    assert_eq!(len, 40 * 20 * 4);
    let data = unsafe { std::slice::from_raw_parts(glycin_ng_svg_render_data(render), len) };
    assert_eq!(&data[0..4], &[255, 0, 0, 255]); // left rect scaled up, red
    let px = |x: usize, y: usize| &data[(y * 40 + x) * 4..(y * 40 + x) * 4 + 4];
    assert_eq!(px(30, 10), &[0, 0, 255, 255]); // right rect, blue
    unsafe { glycin_ng_svg_render_free(render) };

    // Layer render keeps document position.
    let render = unsafe { glycin_ng_svg_render(svg, c"right".as_ptr(), 0, 20, 10, ptr::null(), 1) };
    assert!(!render.is_null(), "{}", last_error());
    let data =
        unsafe { std::slice::from_raw_parts(glycin_ng_svg_render_data(render), 20 * 10 * 4) };
    assert_eq!(&data[0..4], &[0, 0, 0, 0]);
    let px = |x: usize, y: usize| &data[(y * 20 + x) * 4..(y * 20 + x) * 4 + 4];
    assert_eq!(px(15, 5), &[0, 0, 255, 255]);
    unsafe { glycin_ng_svg_render_free(render) };

    // Element render is normalized to the origin.
    let render = unsafe { glycin_ng_svg_render(svg, c"right".as_ptr(), 1, 10, 10, ptr::null(), 1) };
    assert!(!render.is_null(), "{}", last_error());
    let data =
        unsafe { std::slice::from_raw_parts(glycin_ng_svg_render_data(render), 10 * 10 * 4) };
    assert_eq!(&data[0..4], &[0, 0, 255, 255]);
    unsafe { glycin_ng_svg_render_free(render) };

    unsafe { glycin_ng_svg_free(svg) };
}

#[test]
fn premultiplied_output_swaps_convention_only() {
    // Opaque pixels are identical under both alpha conventions; this
    // exercises the flag rather than the math.
    let svg = new_two_rects();
    let render = unsafe { glycin_ng_svg_render(svg, ptr::null(), 0, 20, 10, ptr::null(), 0) };
    assert!(!render.is_null(), "{}", last_error());
    let data =
        unsafe { std::slice::from_raw_parts(glycin_ng_svg_render_data(render), 20 * 10 * 4) };
    assert_eq!(&data[0..4], &[255, 0, 0, 255]);
    unsafe { glycin_ng_svg_render_free(render) };
    unsafe { glycin_ng_svg_free(svg) };
}

#[test]
fn stylesheet_and_dpi_setters_reparse() {
    let svg = new_two_rects();
    let css = b"rect { fill: #00ff00 !important; }";
    assert_eq!(
        unsafe { glycin_ng_svg_set_stylesheet(svg, css.as_ptr(), css.len()) },
        0,
        "{}",
        last_error()
    );
    let render = unsafe { glycin_ng_svg_render(svg, ptr::null(), 0, 20, 10, ptr::null(), 1) };
    assert!(!render.is_null(), "{}", last_error());
    let data =
        unsafe { std::slice::from_raw_parts(glycin_ng_svg_render_data(render), 20 * 10 * 4) };
    assert_eq!(&data[0..4], &[0, 255, 0, 255]);
    unsafe { glycin_ng_svg_render_free(render) };

    // Clearing restores the original fill.
    assert_eq!(
        unsafe { glycin_ng_svg_set_stylesheet(svg, ptr::null(), 0) },
        0
    );
    let render = unsafe { glycin_ng_svg_render(svg, ptr::null(), 0, 20, 10, ptr::null(), 1) };
    assert!(!render.is_null(), "{}", last_error());
    let data =
        unsafe { std::slice::from_raw_parts(glycin_ng_svg_render_data(render), 20 * 10 * 4) };
    assert_eq!(&data[0..4], &[255, 0, 0, 255]);
    unsafe { glycin_ng_svg_render_free(render) };

    // DPI changes rescale physical units: 1in wide at 96dpi vs 192dpi.
    let doc = br#"<svg xmlns="http://www.w3.org/2000/svg" width="1in" height="1in"/>"#;
    let phys = unsafe { glycin_ng_svg_new(doc.as_ptr(), doc.len(), 96.0, ptr::null(), 0) };
    assert!(!phys.is_null(), "{}", last_error());
    let (mut w, mut h) = (0.0f64, 0.0f64);
    unsafe { glycin_ng_svg_size(phys, &mut w, &mut h) };
    assert_eq!(w, 96.0);
    assert_eq!(unsafe { glycin_ng_svg_set_dpi(phys, 192.0) }, 0);
    unsafe { glycin_ng_svg_size(phys, &mut w, &mut h) };
    assert_eq!(w, 192.0);
    unsafe { glycin_ng_svg_free(phys) };

    unsafe { glycin_ng_svg_free(svg) };
}

#[test]
fn null_handles_are_tolerated() {
    unsafe {
        glycin_ng_svg_free(ptr::null_mut());
        glycin_ng_svg_render_free(ptr::null_mut());
        assert!(glycin_ng_svg_render_data(ptr::null()).is_null());
        assert_eq!(glycin_ng_svg_render_len(ptr::null()), 0);
        assert_eq!(
            glycin_ng_svg_size(ptr::null(), ptr::null_mut(), ptr::null_mut()),
            -1
        );
        assert!(glycin_ng_svg_render(ptr::null(), ptr::null(), 0, 4, 4, ptr::null(), 1).is_null());
        assert_eq!(glycin_ng_svg_set_dpi(ptr::null_mut(), 96.0), -1);
    }
}
