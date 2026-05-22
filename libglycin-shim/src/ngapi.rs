//! `extern "C"` declarations for the glycin-ng C ABI exported by
//! `libglycin_ng.so`.
//!
//! The shim talks to glycin-ng exclusively through this surface; we
//! no longer pull the Rust API in. At link time these symbols are
//! satisfied by `libglycin_ng.so`, so the resulting `libglycin-2.so.0`
//! does not bundle a copy of the codec stack.

#![allow(non_camel_case_types, dead_code)]

use std::ffi::{c_char, c_int, c_uint};

#[repr(C)]
pub(crate) struct GlycinNgLoader {
    _opaque: [u8; 0],
}

#[repr(C)]
pub(crate) struct GlycinNgImage {
    _opaque: [u8; 0],
}

#[repr(C)]
pub(crate) struct GlycinNgTexture {
    _opaque: [u8; 0],
}

#[repr(C)]
pub(crate) struct GlycinNgEncoder {
    _opaque: [u8; 0],
}

#[repr(C)]
pub(crate) struct GlycinNgEncodedImage {
    _opaque: [u8; 0],
}

// Memory-format constants, mirroring glycin-ng's `GLYCIN_NG_FORMAT_*`.
pub(crate) const GLYCIN_NG_FORMAT_UNKNOWN: c_uint = 0;
pub(crate) const GLYCIN_NG_FORMAT_G8: c_uint = 1;
pub(crate) const GLYCIN_NG_FORMAT_G8A8: c_uint = 2;
pub(crate) const GLYCIN_NG_FORMAT_G8A8_PRE: c_uint = 3;
pub(crate) const GLYCIN_NG_FORMAT_G16: c_uint = 4;
pub(crate) const GLYCIN_NG_FORMAT_G16A16: c_uint = 5;
pub(crate) const GLYCIN_NG_FORMAT_G16A16_PRE: c_uint = 6;
pub(crate) const GLYCIN_NG_FORMAT_R8G8B8: c_uint = 10;
pub(crate) const GLYCIN_NG_FORMAT_R8G8B8A8: c_uint = 11;
pub(crate) const GLYCIN_NG_FORMAT_R8G8B8A8_PRE: c_uint = 12;
pub(crate) const GLYCIN_NG_FORMAT_B8G8R8: c_uint = 13;
pub(crate) const GLYCIN_NG_FORMAT_B8G8R8A8: c_uint = 14;
pub(crate) const GLYCIN_NG_FORMAT_B8G8R8A8_PRE: c_uint = 15;
pub(crate) const GLYCIN_NG_FORMAT_A8R8G8B8: c_uint = 16;
pub(crate) const GLYCIN_NG_FORMAT_A8R8G8B8_PRE: c_uint = 17;
pub(crate) const GLYCIN_NG_FORMAT_A8B8G8R8: c_uint = 18;
pub(crate) const GLYCIN_NG_FORMAT_R16G16B16: c_uint = 20;
pub(crate) const GLYCIN_NG_FORMAT_R16G16B16A16: c_uint = 21;
pub(crate) const GLYCIN_NG_FORMAT_R16G16B16A16_PRE: c_uint = 22;
pub(crate) const GLYCIN_NG_FORMAT_R16G16B16_F: c_uint = 23;
pub(crate) const GLYCIN_NG_FORMAT_R16G16B16A16_F: c_uint = 24;
pub(crate) const GLYCIN_NG_FORMAT_R32G32B32_F: c_uint = 25;
pub(crate) const GLYCIN_NG_FORMAT_R32G32B32A32_F: c_uint = 26;
pub(crate) const GLYCIN_NG_FORMAT_R32G32B32A32_F_PRE: c_uint = 27;

unsafe extern "C" {
    // Error helpers.
    pub(crate) fn glycin_ng_last_error() -> *const c_char;
    pub(crate) fn glycin_ng_clear_last_error();

    // Loader lifecycle and configuration.
    pub(crate) fn glycin_ng_loader_new_path(path: *const c_char) -> *mut GlycinNgLoader;
    pub(crate) fn glycin_ng_loader_new_bytes(data: *const u8, len: usize) -> *mut GlycinNgLoader;
    pub(crate) fn glycin_ng_loader_free(loader: *mut GlycinNgLoader);
    pub(crate) fn glycin_ng_loader_sandbox(
        loader: *mut GlycinNgLoader,
        landlock: c_int,
        seccomp: c_int,
        rlimit: c_int,
        strict: c_int,
    ) -> c_int;
    pub(crate) fn glycin_ng_loader_format_hint(
        loader: *mut GlycinNgLoader,
        format: c_uint,
    ) -> c_int;
    pub(crate) fn glycin_ng_loader_apply_transformations(
        loader: *mut GlycinNgLoader,
        apply: c_int,
    ) -> c_int;
    pub(crate) fn glycin_ng_loader_render_size_hint(
        loader: *mut GlycinNgLoader,
        width: u32,
        height: u32,
    ) -> c_int;
    pub(crate) fn glycin_ng_loader_set_max_width(
        loader: *mut GlycinNgLoader,
        max_width: u32,
    ) -> c_int;
    pub(crate) fn glycin_ng_loader_set_max_height(
        loader: *mut GlycinNgLoader,
        max_height: u32,
    ) -> c_int;
    pub(crate) fn glycin_ng_loader_set_max_pixels(
        loader: *mut GlycinNgLoader,
        max_pixels: u64,
    ) -> c_int;
    pub(crate) fn glycin_ng_loader_set_max_frames(
        loader: *mut GlycinNgLoader,
        max_frames: u32,
    ) -> c_int;
    pub(crate) fn glycin_ng_loader_set_max_animation_seconds(
        loader: *mut GlycinNgLoader,
        seconds: u64,
    ) -> c_int;
    pub(crate) fn glycin_ng_loader_set_decode_memory_mib(
        loader: *mut GlycinNgLoader,
        mib: u64,
    ) -> c_int;
    pub(crate) fn glycin_ng_loader_set_decode_cpu_seconds(
        loader: *mut GlycinNgLoader,
        seconds: u64,
    ) -> c_int;
    pub(crate) fn glycin_ng_loader_load(loader: *mut GlycinNgLoader) -> *mut GlycinNgImage;

    // Image accessors.
    pub(crate) fn glycin_ng_image_free(image: *mut GlycinNgImage);
    pub(crate) fn glycin_ng_image_width(image: *const GlycinNgImage) -> u32;
    pub(crate) fn glycin_ng_image_height(image: *const GlycinNgImage) -> u32;
    pub(crate) fn glycin_ng_image_frame_count(image: *const GlycinNgImage) -> usize;
    pub(crate) fn glycin_ng_image_is_animated(image: *const GlycinNgImage) -> c_int;
    pub(crate) fn glycin_ng_image_orientation(image: *const GlycinNgImage) -> u16;
    pub(crate) fn glycin_ng_image_format_name(image: *const GlycinNgImage) -> *const c_char;
    pub(crate) fn glycin_ng_image_texture(
        image: *const GlycinNgImage,
        index: usize,
    ) -> *const GlycinNgTexture;
    pub(crate) fn glycin_ng_image_frame_delay_ms(image: *const GlycinNgImage, index: usize) -> u64;

    // Texture accessors.
    pub(crate) fn glycin_ng_texture_width(texture: *const GlycinNgTexture) -> u32;
    pub(crate) fn glycin_ng_texture_height(texture: *const GlycinNgTexture) -> u32;
    pub(crate) fn glycin_ng_texture_stride(texture: *const GlycinNgTexture) -> u32;
    pub(crate) fn glycin_ng_texture_format(texture: *const GlycinNgTexture) -> c_uint;
    pub(crate) fn glycin_ng_texture_data(texture: *const GlycinNgTexture) -> *const u8;
    pub(crate) fn glycin_ng_texture_data_len(texture: *const GlycinNgTexture) -> usize;

    // Known-format helpers.
    pub(crate) fn glycin_ng_known_format_from_mime(mime: *const c_char) -> c_uint;
    pub(crate) fn glycin_ng_known_format_from_extension(ext: *const c_char) -> c_uint;

    // Encoder lifecycle and configuration.
    pub(crate) fn glycin_ng_encoder_new(format: c_uint) -> *mut GlycinNgEncoder;
    pub(crate) fn glycin_ng_encoder_free(encoder: *mut GlycinNgEncoder);
    pub(crate) fn glycin_ng_encoder_set_quality(encoder: *mut GlycinNgEncoder, quality: u8);
    pub(crate) fn glycin_ng_encoder_set_compression(encoder: *mut GlycinNgEncoder, compression: u8);
    pub(crate) fn glycin_ng_encoder_set_icc_profile(
        encoder: *mut GlycinNgEncoder,
        data: *const u8,
        len: usize,
    ) -> c_int;
    pub(crate) fn glycin_ng_encoder_add_metadata(
        encoder: *mut GlycinNgEncoder,
        key: *const c_char,
        value: *const c_char,
    ) -> c_int;
    pub(crate) fn glycin_ng_encoder_add_frame(
        encoder: *mut GlycinNgEncoder,
        width: u32,
        height: u32,
        stride: u32,
        format: c_uint,
        data: *const u8,
        data_len: usize,
    ) -> c_int;
    pub(crate) fn glycin_ng_encoder_encode(
        encoder: *mut GlycinNgEncoder,
    ) -> *mut GlycinNgEncodedImage;

    // Encoded-image accessors.
    pub(crate) fn glycin_ng_encoded_image_free(image: *mut GlycinNgEncodedImage);
    pub(crate) fn glycin_ng_encoded_image_data(image: *const GlycinNgEncodedImage) -> *const u8;
    pub(crate) fn glycin_ng_encoded_image_len(image: *const GlycinNgEncodedImage) -> usize;
}

/// Convert the last error message reported by `libglycin_ng.so` to
/// an owned `String`. Falls back to a generic message when no error
/// is set or the C string is not valid UTF-8.
pub(crate) fn last_error_message() -> String {
    let ptr = unsafe { glycin_ng_last_error() };
    if ptr.is_null() {
        return "glycin-ng reported failure".into();
    }
    let cstr = unsafe { std::ffi::CStr::from_ptr(ptr) };
    cstr.to_string_lossy().into_owned()
}
