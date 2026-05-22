//! FFI smoke tests for the C ABI.
//!
//! These call the `extern "C"` functions directly from Rust to keep
//! the test self-contained (no `cc` invocation required).

#![cfg(all(feature = "c-api", feature = "png"))]

use std::ffi::{CStr, c_char, c_uint};
use std::ptr;

use glycin_ng::c_api::{
    GLYCIN_NG_FORMAT_R8G8B8A8, GLYCIN_NG_KFMT_PNG, GlycinNgEncodedImage, GlycinNgEncoder,
    GlycinNgImage, GlycinNgLoader,
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
    fn glycin_ng_loader_apply_transformations(loader: *mut GlycinNgLoader, apply: i32) -> i32;
    fn glycin_ng_loader_render_size_hint(
        loader: *mut GlycinNgLoader,
        width: u32,
        height: u32,
    ) -> i32;
    fn glycin_ng_loader_set_max_width(loader: *mut GlycinNgLoader, max_width: u32) -> i32;
    fn glycin_ng_loader_set_max_height(loader: *mut GlycinNgLoader, max_height: u32) -> i32;
    fn glycin_ng_loader_set_max_pixels(loader: *mut GlycinNgLoader, max_pixels: u64) -> i32;
    fn glycin_ng_loader_set_max_frames(loader: *mut GlycinNgLoader, max_frames: u32) -> i32;
    fn glycin_ng_loader_set_max_animation_seconds(loader: *mut GlycinNgLoader, secs: u64) -> i32;
    fn glycin_ng_loader_set_decode_memory_mib(loader: *mut GlycinNgLoader, mib: u64) -> i32;
    fn glycin_ng_loader_set_decode_cpu_seconds(loader: *mut GlycinNgLoader, secs: u64) -> i32;
    fn glycin_ng_loader_load(loader: *mut GlycinNgLoader) -> *mut GlycinNgImage;
    fn glycin_ng_image_free(image: *mut GlycinNgImage);
    fn glycin_ng_image_width(image: *const GlycinNgImage) -> u32;
    fn glycin_ng_image_height(image: *const GlycinNgImage) -> u32;
    fn glycin_ng_image_frame_count(image: *const GlycinNgImage) -> usize;
    fn glycin_ng_image_is_animated(image: *const GlycinNgImage) -> i32;
    fn glycin_ng_image_orientation(image: *const GlycinNgImage) -> u16;
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
    fn glycin_ng_encoder_new(format: c_uint) -> *mut GlycinNgEncoder;
    fn glycin_ng_encoder_free(encoder: *mut GlycinNgEncoder);
    fn glycin_ng_encoder_add_frame(
        encoder: *mut GlycinNgEncoder,
        width: u32,
        height: u32,
        stride: u32,
        format: c_uint,
        data: *const u8,
        data_len: usize,
    ) -> i32;
    fn glycin_ng_encoder_encode(encoder: *mut GlycinNgEncoder) -> *mut GlycinNgEncodedImage;
    fn glycin_ng_encoded_image_free(image: *mut GlycinNgEncodedImage);
    fn glycin_ng_encoded_image_data(image: *const GlycinNgEncodedImage) -> *const u8;
    fn glycin_ng_encoded_image_len(image: *const GlycinNgEncodedImage) -> usize;
    fn glycin_ng_known_format_from_mime(mime: *const c_char) -> c_uint;
    fn glycin_ng_known_format_from_extension(ext: *const c_char) -> c_uint;
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

#[cfg(feature = "encode")]
#[test]
fn encoder_round_trip_png_through_c_api() {
    // Authored pixels: 2x2 RGBA8 with four distinct colors so we can
    // verify ordering survives encode + decode.
    let width: u32 = 2;
    let height: u32 = 2;
    let pixels: [u8; 16] = [
        255, 0, 0, 255, // red
        0, 255, 0, 255, // green
        0, 0, 255, 255, // blue
        255, 255, 0, 255, // yellow
    ];

    let enc = unsafe { glycin_ng_encoder_new(GLYCIN_NG_KFMT_PNG) };
    assert!(!enc.is_null(), "encoder_new returned NULL");

    let rc = unsafe {
        glycin_ng_encoder_add_frame(
            enc,
            width,
            height,
            width * 4,
            GLYCIN_NG_FORMAT_R8G8B8A8,
            pixels.as_ptr(),
            pixels.len(),
        )
    };
    assert_eq!(rc, 0, "add_frame failed");

    let encoded = unsafe { glycin_ng_encoder_encode(enc) };
    assert!(!encoded.is_null(), "encode returned NULL");

    let out_ptr = unsafe { glycin_ng_encoded_image_data(encoded) };
    let out_len = unsafe { glycin_ng_encoded_image_len(encoded) };
    assert!(!out_ptr.is_null());
    assert!(out_len > 0);
    let out = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
    assert!(out.starts_with(b"\x89PNG"), "output is not a PNG");

    // Decode the encoded bytes back through the C loader and confirm
    // pixel parity.
    let loader = unsafe { glycin_ng_loader_new_bytes(out.as_ptr(), out.len()) };
    assert!(!loader.is_null());
    let image = unsafe { glycin_ng_loader_load(loader) };
    assert!(!image.is_null(), "decode of encoded PNG failed");
    assert_eq!(unsafe { glycin_ng_image_width(image) }, width);
    assert_eq!(unsafe { glycin_ng_image_height(image) }, height);

    let tex = unsafe { glycin_ng_image_texture(image, 0) };
    let tlen = unsafe { glycin_ng_texture_data_len(tex) };
    let tdata = unsafe { glycin_ng_texture_data(tex) };
    let decoded = unsafe { std::slice::from_raw_parts(tdata, tlen) };
    assert_eq!(decoded, &pixels[..]);

    unsafe { glycin_ng_image_free(image) };
    unsafe { glycin_ng_encoded_image_free(encoded) };
    unsafe { glycin_ng_encoder_free(enc) };
}

#[test]
fn encoder_new_rejects_unknown_format_constant() {
    let enc = unsafe { glycin_ng_encoder_new(99999) };
    assert!(enc.is_null());
    let err = unsafe { glycin_ng_last_error() };
    assert!(!err.is_null());
}

#[test]
fn known_format_from_mime_recognises_common_types() {
    let png = unsafe { glycin_ng_known_format_from_mime(c"image/png".as_ptr()) };
    let jpeg = unsafe { glycin_ng_known_format_from_mime(c"image/jpeg".as_ptr()) };
    let webp = unsafe { glycin_ng_known_format_from_mime(c"image/webp".as_ptr()) };
    let bogus = unsafe { glycin_ng_known_format_from_mime(c"image/not-a-real-format".as_ptr()) };
    let null = unsafe { glycin_ng_known_format_from_mime(ptr::null()) };
    assert_eq!(png, GLYCIN_NG_KFMT_PNG);
    assert!(jpeg != 0 && jpeg != png);
    assert!(webp != 0 && webp != png && webp != jpeg);
    assert_eq!(bogus, 0);
    assert_eq!(null, 0);
}

#[test]
fn loader_extras_apply_cleanly() {
    let bytes = encode_rgba_png(4, 4);
    let loader = unsafe { glycin_ng_loader_new_bytes(bytes.as_ptr(), bytes.len()) };
    assert!(!loader.is_null());
    assert_eq!(
        unsafe { glycin_ng_loader_apply_transformations(loader, 0) },
        0
    );
    assert_eq!(
        unsafe { glycin_ng_loader_render_size_hint(loader, 16, 16) },
        0
    );
    assert_eq!(unsafe { glycin_ng_loader_set_max_width(loader, 8192) }, 0);
    assert_eq!(unsafe { glycin_ng_loader_set_max_height(loader, 8192) }, 0);
    assert_eq!(
        unsafe { glycin_ng_loader_set_max_pixels(loader, 1 << 20) },
        0
    );
    assert_eq!(unsafe { glycin_ng_loader_set_max_frames(loader, 64) }, 0);
    assert_eq!(
        unsafe { glycin_ng_loader_set_max_animation_seconds(loader, 5) },
        0
    );
    assert_eq!(
        unsafe { glycin_ng_loader_set_decode_memory_mib(loader, 64) },
        0
    );
    assert_eq!(
        unsafe { glycin_ng_loader_set_decode_cpu_seconds(loader, 5) },
        0
    );
    let image = unsafe { glycin_ng_loader_load(loader) };
    assert!(!image.is_null());
    unsafe { glycin_ng_image_free(image) };
}

#[test]
fn loader_set_max_pixels_rejects_oversize_image() {
    // 4x4 = 16 pixels; cap at 8 so the decoder rejects it.
    let bytes = encode_rgba_png(4, 4);
    let loader = unsafe { glycin_ng_loader_new_bytes(bytes.as_ptr(), bytes.len()) };
    assert!(!loader.is_null());
    assert_eq!(unsafe { glycin_ng_loader_set_max_pixels(loader, 8) }, 0);
    let image = unsafe { glycin_ng_loader_load(loader) };
    assert!(
        image.is_null(),
        "decode should fail on too-small max_pixels"
    );
    let err = unsafe { glycin_ng_last_error() };
    assert!(!err.is_null());
}

#[test]
fn loader_setters_reject_null_handle() {
    assert_eq!(
        unsafe { glycin_ng_loader_apply_transformations(ptr::null_mut(), 1) },
        -1
    );
    assert_eq!(
        unsafe { glycin_ng_loader_set_max_width(ptr::null_mut(), 1024) },
        -1
    );
}

#[test]
fn image_orientation_defaults_to_identity_on_null() {
    assert_eq!(unsafe { glycin_ng_image_orientation(ptr::null()) }, 1);
}

#[test]
fn image_is_animated_defaults_to_false_on_still() {
    let bytes = encode_rgba_png(4, 4);
    let loader = unsafe { glycin_ng_loader_new_bytes(bytes.as_ptr(), bytes.len()) };
    let image = unsafe { glycin_ng_loader_load(loader) };
    assert!(!image.is_null());
    assert_eq!(unsafe { glycin_ng_image_is_animated(image) }, 0);
    assert_eq!(unsafe { glycin_ng_image_orientation(image) }, 1);
    unsafe { glycin_ng_image_free(image) };
}

#[test]
fn known_format_from_extension_handles_aliases() {
    let png_lower = unsafe { glycin_ng_known_format_from_extension(c"png".as_ptr()) };
    let png_upper = unsafe { glycin_ng_known_format_from_extension(c"PNG".as_ptr()) };
    let jpg = unsafe { glycin_ng_known_format_from_extension(c"jpg".as_ptr()) };
    let jpeg = unsafe { glycin_ng_known_format_from_extension(c"jpeg".as_ptr()) };
    let bogus = unsafe { glycin_ng_known_format_from_extension(c"xyz123".as_ptr()) };
    assert_eq!(png_lower, GLYCIN_NG_KFMT_PNG);
    assert_eq!(png_upper, GLYCIN_NG_KFMT_PNG);
    assert_eq!(jpg, jpeg);
    assert!(jpeg != 0);
    assert_eq!(bogus, 0);
}
