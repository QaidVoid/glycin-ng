//! FFI smoke tests for the C ABI.
//!
//! These call the `extern "C"` functions directly from Rust to keep
//! the test self-contained (no `cc` invocation required).

#![cfg(all(feature = "c-api", feature = "png"))]

use std::ffi::{CStr, c_char, c_uint};
use std::ptr;

use glycin_ng::c_api::{
    GLYCIN_NG_FORMAT_R8G8B8A8, GLYCIN_NG_KFMT_PNG, GlycinNgImage, GlycinNgLoader,
};

#[allow(improper_ctypes)]
unsafe extern "C" {
    fn glycin_ng_last_error() -> *const c_char;
    fn glycin_ng_loader_new_bytes(data: *const u8, len: usize) -> *mut GlycinNgLoader;
    fn glycin_ng_loader_new_path(path: *const c_char) -> *mut GlycinNgLoader;
    fn glycin_ng_loader_free(loader: *mut GlycinNgLoader);
    fn glycin_ng_loader_sandbox(
        loader: *mut GlycinNgLoader,
        landlock: i32,
        seccomp: i32,
        rlimit: i32,
        strict: i32,
    ) -> i32;
    fn glycin_ng_loader_format_hint(loader: *mut GlycinNgLoader, format: c_uint) -> i32;
    fn glycin_ng_loader_load(loader: *mut GlycinNgLoader) -> *mut GlycinNgImage;
    fn glycin_ng_image_free(image: *mut GlycinNgImage);
    fn glycin_ng_image_width(image: *const GlycinNgImage) -> u32;
    fn glycin_ng_image_height(image: *const GlycinNgImage) -> u32;
    fn glycin_ng_image_frame_count(image: *const GlycinNgImage) -> usize;
    fn glycin_ng_image_format_name(image: *const GlycinNgImage) -> *const c_char;
    fn glycin_ng_image_texture(
        image: *const GlycinNgImage,
        index: usize,
    ) -> *const glycin_ng::Texture;
    fn glycin_ng_texture_width(texture: *const glycin_ng::Texture) -> u32;
    fn glycin_ng_texture_height(texture: *const glycin_ng::Texture) -> u32;
    fn glycin_ng_texture_format(texture: *const glycin_ng::Texture) -> c_uint;
    fn glycin_ng_texture_data(texture: *const glycin_ng::Texture) -> *const u8;
    fn glycin_ng_texture_data_len(texture: *const glycin_ng::Texture) -> usize;
}

fn encode_rgba_png(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().unwrap();
        writer
            .write_image_data(&vec![0x80; (width * height * 4) as usize])
            .unwrap();
    }
    out
}

#[test]
fn loader_new_bytes_returns_handle() {
    let bytes = encode_rgba_png(4, 4);
    let loader = unsafe { glycin_ng_loader_new_bytes(bytes.as_ptr(), bytes.len()) };
    assert!(!loader.is_null());
    unsafe { glycin_ng_loader_free(loader) };
}

#[test]
fn loader_new_bytes_with_null_returns_null() {
    let loader = unsafe { glycin_ng_loader_new_bytes(ptr::null(), 16) };
    assert!(loader.is_null());
    let err = unsafe { glycin_ng_last_error() };
    assert!(!err.is_null());
}

#[test]
fn decode_png_through_c_api() {
    let bytes = encode_rgba_png(8, 8);
    let loader = unsafe { glycin_ng_loader_new_bytes(bytes.as_ptr(), bytes.len()) };
    assert!(!loader.is_null());

    let rc = unsafe { glycin_ng_loader_sandbox(loader, 0, 0, 0, 0) };
    assert_eq!(rc, 0);

    let rc = unsafe { glycin_ng_loader_format_hint(loader, GLYCIN_NG_KFMT_PNG) };
    assert_eq!(rc, 0);

    let image = unsafe { glycin_ng_loader_load(loader) };
    assert!(!image.is_null(), "decode failed");

    let width = unsafe { glycin_ng_image_width(image) };
    let height = unsafe { glycin_ng_image_height(image) };
    let frames = unsafe { glycin_ng_image_frame_count(image) };
    assert_eq!(width, 8);
    assert_eq!(height, 8);
    assert_eq!(frames, 1);

    let format_ptr = unsafe { glycin_ng_image_format_name(image) };
    let format = unsafe { CStr::from_ptr(format_ptr) };
    assert_eq!(format.to_str().unwrap(), "png");

    let tex = unsafe { glycin_ng_image_texture(image, 0) };
    assert!(!tex.is_null());
    let tw = unsafe { glycin_ng_texture_width(tex) };
    let th = unsafe { glycin_ng_texture_height(tex) };
    let tf = unsafe { glycin_ng_texture_format(tex) };
    let tlen = unsafe { glycin_ng_texture_data_len(tex) };
    let tdata = unsafe { glycin_ng_texture_data(tex) };
    assert_eq!(tw, 8);
    assert_eq!(th, 8);
    assert_eq!(tf, GLYCIN_NG_FORMAT_R8G8B8A8);
    assert_eq!(tlen, 8 * 8 * 4);
    assert!(!tdata.is_null());

    unsafe { glycin_ng_image_free(image) };
}

#[test]
fn decode_garbage_returns_null_with_error_message() {
    let bytes = b"not a png at all".to_vec();
    let loader = unsafe { glycin_ng_loader_new_bytes(bytes.as_ptr(), bytes.len()) };
    assert!(!loader.is_null());
    let image = unsafe { glycin_ng_loader_load(loader) };
    assert!(image.is_null());
    let err = unsafe { glycin_ng_last_error() };
    assert!(!err.is_null());
    let msg = unsafe { CStr::from_ptr(err) }.to_str().unwrap();
    assert!(!msg.is_empty(), "expected non-empty error message");
}

#[test]
fn loader_new_path_handles_invalid_path() {
    let path = c"/nonexistent/path/that/does/not/exist.png".as_ptr();
    let loader = unsafe { glycin_ng_loader_new_path(path) };
    assert!(!loader.is_null());
    let image = unsafe { glycin_ng_loader_load(loader) };
    assert!(image.is_null());
}
