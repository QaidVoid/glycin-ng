//! C ABI surface exposed by the `cdylib` build of this crate.
//!
//! Every function is `#[no_mangle] extern "C"` and uses opaque
//! heap-allocated handles. Callers free the handles they created;
//! everything else is borrowed from those handles and remains valid
//! until the handle that owns it is freed.
//!
//! # Error handling
//!
//! Fallible functions return either a non-NULL handle on success and
//! NULL on error, or `0` on success and a non-zero error code on
//! failure. After a failed call, [`glycin_ng_last_error`] returns a
//! NUL-terminated message describing what went wrong, valid until
//! the next call that produces (or clears) an error on the same
//! thread.
//!
//! # Threads
//!
//! Handles are not thread-safe; do not share a single handle across
//! threads without external synchronization. The last-error message
//! is thread-local.
//!
//! Every `unsafe extern "C" fn` in this module requires its handle
//! arguments to be either NULL or valid pointers returned by an
//! earlier function in this module. Functions that may consume or
//! free a handle document the resulting ownership transfer in their
//! item docs.

#![allow(clippy::missing_safety_doc)]

use std::cell::RefCell;
use std::ffi::{CStr, CString, c_char, c_int, c_uint};
use std::path::PathBuf;
use std::ptr;
use std::slice;

use crate::{
    EncodeFrame, Encoder, Error, Frame, Image, KnownFormat, Loader, MemoryFormat, SandboxSelector,
    Texture,
};

/// Opaque [`Loader`] handle.
#[repr(C)]
pub struct GlycinNgLoader {
    inner: Option<Loader>,
}

/// Opaque [`Image`] handle.
#[repr(C)]
pub struct GlycinNgImage {
    inner: Image,
}

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

fn set_error<E: std::fmt::Display>(e: E) {
    let msg = CString::new(e.to_string()).unwrap_or_else(|_| CString::new("error").unwrap());
    LAST_ERROR.with(|cell| *cell.borrow_mut() = Some(msg));
}

fn clear_error() {
    LAST_ERROR.with(|cell| *cell.borrow_mut() = None);
}

/// Return the last error message produced on this thread, or NULL
/// if none. The pointer is valid until the next call that produces
/// or clears an error on the same thread.
#[unsafe(no_mangle)]
pub extern "C" fn glycin_ng_last_error() -> *const c_char {
    LAST_ERROR.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|s| s.as_ptr())
            .unwrap_or(ptr::null())
    })
}

/// Clear the last-error slot for this thread.
#[unsafe(no_mangle)]
pub extern "C" fn glycin_ng_clear_last_error() {
    clear_error();
}

/// Free a loader handle. Safe to call on NULL.
///
/// # Safety
///
/// `loader` must have been returned by a `glycin_ng_loader_new_*`
/// function and must not have already been freed or consumed by
/// `glycin_ng_loader_load`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_loader_free(loader: *mut GlycinNgLoader) {
    if !loader.is_null() {
        drop(unsafe { Box::from_raw(loader) });
    }
}

/// Construct a loader from a filesystem path. Returns NULL on error.
///
/// # Safety
///
/// `path` must be a valid pointer to a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_loader_new_path(path: *const c_char) -> *mut GlycinNgLoader {
    clear_error();
    if path.is_null() {
        set_error("path is null");
        return ptr::null_mut();
    }
    let cstr = unsafe { CStr::from_ptr(path) };
    let pb = PathBuf::from(cstr.to_string_lossy().as_ref());
    let loader = Loader::new_path(pb);
    Box::into_raw(Box::new(GlycinNgLoader {
        inner: Some(loader),
    }))
}

/// Construct a loader from an in-memory byte buffer. Returns NULL on
/// error. The bytes are copied; the caller may free `data` after the
/// call returns.
///
/// # Safety
///
/// `data` must be a valid pointer to at least `len` bytes, or `len`
/// must be `0`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_loader_new_bytes(
    data: *const u8,
    len: usize,
) -> *mut GlycinNgLoader {
    clear_error();
    if data.is_null() && len != 0 {
        set_error("data is null but len is non-zero");
        return ptr::null_mut();
    }
    let bytes: Vec<u8> = if len == 0 {
        Vec::new()
    } else {
        unsafe { slice::from_raw_parts(data, len) }.to_vec()
    };
    let loader = Loader::new_bytes(bytes);
    Box::into_raw(Box::new(GlycinNgLoader {
        inner: Some(loader),
    }))
}

/// Toggle the sandbox layers on the loader.
///
/// # Safety
///
/// `loader` must be a valid pointer returned by
/// `glycin_ng_loader_new_*` and must not have been consumed or freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_loader_sandbox(
    loader: *mut GlycinNgLoader,
    landlock: c_int,
    seccomp: c_int,
    rlimit: c_int,
    strict: c_int,
) -> c_int {
    clear_error();
    let Some(handle) = (unsafe { loader.as_mut() }) else {
        set_error("loader is null");
        return -1;
    };
    let Some(inner) = handle.inner.take() else {
        set_error("loader has already been consumed");
        return -1;
    };
    let selector = SandboxSelector {
        landlock: landlock != 0,
        seccomp: seccomp != 0,
        rlimit: rlimit != 0,
        strict: strict != 0,
    };
    handle.inner = Some(inner.sandbox_selector(selector));
    0
}

/// Override the format-detection step with an explicit hint.
///
/// `format` must be one of the `GLYCIN_NG_FORMAT_*` constants.
///
/// # Safety
///
/// `loader` must be a valid pointer returned by
/// `glycin_ng_loader_new_*` and must not have been consumed or freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_loader_format_hint(
    loader: *mut GlycinNgLoader,
    format: c_uint,
) -> c_int {
    clear_error();
    let Some(handle) = (unsafe { loader.as_mut() }) else {
        set_error("loader is null");
        return -1;
    };
    let Some(inner) = handle.inner.take() else {
        set_error("loader has already been consumed");
        return -1;
    };
    let f = match c_uint_to_format(format) {
        Some(f) => f,
        None => {
            handle.inner = Some(inner);
            set_error("unknown format constant");
            return -1;
        }
    };
    handle.inner = Some(inner.format_hint(f));
    0
}

/// Apply (or skip) EXIF orientation during decode. Pass a non-zero
/// value to apply, 0 to leave the texture in its raw orientation.
/// Default behavior, with no call, is to apply.
///
/// # Safety
///
/// `loader` must be a valid pointer returned by
/// `glycin_ng_loader_new_*` and must not have been consumed or freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_loader_apply_transformations(
    loader: *mut GlycinNgLoader,
    apply: c_int,
) -> c_int {
    modify_loader(loader, |l| l.apply_transformations(apply != 0))
}

/// Request the decoder render at a specific output size in pixels.
/// Only resolution-independent formats (currently SVG) honor the
/// hint; raster decoders ignore it.
///
/// # Safety
///
/// `loader` must be a valid pointer returned by
/// `glycin_ng_loader_new_*` and must not have been consumed or freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_loader_render_size_hint(
    loader: *mut GlycinNgLoader,
    width: u32,
    height: u32,
) -> c_int {
    modify_loader(loader, |l| l.render_size_hint(width, height))
}

/// Cap the maximum decoded image width in pixels.
///
/// # Safety
///
/// `loader` must be a valid pointer returned by
/// `glycin_ng_loader_new_*` and must not have been consumed or freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_loader_set_max_width(
    loader: *mut GlycinNgLoader,
    max_width: u32,
) -> c_int {
    modify_loader(loader, |l| {
        let mut lim = l.current_limits();
        lim.max_width = max_width;
        l.limits(lim)
    })
}

/// Cap the maximum decoded image height in pixels.
///
/// # Safety
///
/// `loader` must be a valid pointer returned by
/// `glycin_ng_loader_new_*` and must not have been consumed or freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_loader_set_max_height(
    loader: *mut GlycinNgLoader,
    max_height: u32,
) -> c_int {
    modify_loader(loader, |l| {
        let mut lim = l.current_limits();
        lim.max_height = max_height;
        l.limits(lim)
    })
}

/// Cap the maximum total pixels (width times height times frame
/// count) for the decoded image.
///
/// # Safety
///
/// `loader` must be a valid pointer returned by
/// `glycin_ng_loader_new_*` and must not have been consumed or freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_loader_set_max_pixels(
    loader: *mut GlycinNgLoader,
    max_pixels: u64,
) -> c_int {
    modify_loader(loader, |l| {
        let mut lim = l.current_limits();
        lim.max_pixels = max_pixels;
        l.limits(lim)
    })
}

/// Cap the maximum frame count for animated decodes.
///
/// # Safety
///
/// `loader` must be a valid pointer returned by
/// `glycin_ng_loader_new_*` and must not have been consumed or freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_loader_set_max_frames(
    loader: *mut GlycinNgLoader,
    max_frames: u32,
) -> c_int {
    modify_loader(loader, |l| {
        let mut lim = l.current_limits();
        lim.max_frames = max_frames;
        l.limits(lim)
    })
}

/// Cap the maximum sum-of-frame-delays for animated decodes, in
/// seconds.
///
/// # Safety
///
/// `loader` must be a valid pointer returned by
/// `glycin_ng_loader_new_*` and must not have been consumed or freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_loader_set_max_animation_seconds(
    loader: *mut GlycinNgLoader,
    seconds: u64,
) -> c_int {
    modify_loader(loader, |l| {
        let mut lim = l.current_limits();
        lim.max_animation_duration = std::time::Duration::from_secs(seconds);
        l.limits(lim)
    })
}

/// Cap the `RLIMIT_AS` value applied while decoding under the
/// sandbox, in MiB.
///
/// # Safety
///
/// `loader` must be a valid pointer returned by
/// `glycin_ng_loader_new_*` and must not have been consumed or freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_loader_set_decode_memory_mib(
    loader: *mut GlycinNgLoader,
    mib: u64,
) -> c_int {
    modify_loader(loader, |l| {
        let mut lim = l.current_limits();
        lim.decode_memory_mib = mib;
        l.limits(lim)
    })
}

/// Cap the `RLIMIT_CPU` value applied while decoding under the
/// sandbox, in seconds.
///
/// # Safety
///
/// `loader` must be a valid pointer returned by
/// `glycin_ng_loader_new_*` and must not have been consumed or freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_loader_set_decode_cpu_seconds(
    loader: *mut GlycinNgLoader,
    seconds: u64,
) -> c_int {
    modify_loader(loader, |l| {
        let mut lim = l.current_limits();
        lim.decode_cpu_seconds = seconds;
        l.limits(lim)
    })
}

fn modify_loader<F: FnOnce(Loader) -> Loader>(loader: *mut GlycinNgLoader, mutate: F) -> c_int {
    clear_error();
    let Some(handle) = (unsafe { loader.as_mut() }) else {
        set_error("loader is null");
        return -1;
    };
    let Some(inner) = handle.inner.take() else {
        set_error("loader has already been consumed");
        return -1;
    };
    handle.inner = Some(mutate(inner));
    0
}

/// Consume the loader and decode the image.
///
/// On success returns a non-NULL [`GlycinNgImage`] handle and the
/// loader is freed. On failure returns NULL and the loader is also
/// freed. Either way the caller must not use `loader` after this
/// call.
///
/// # Safety
///
/// `loader` must be a valid pointer returned by
/// `glycin_ng_loader_new_*` and must not have been consumed or freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_loader_load(loader: *mut GlycinNgLoader) -> *mut GlycinNgImage {
    clear_error();
    if loader.is_null() {
        set_error("loader is null");
        return ptr::null_mut();
    }
    let mut handle = unsafe { Box::from_raw(loader) };
    let Some(inner) = handle.inner.take() else {
        set_error("loader has already been consumed");
        return ptr::null_mut();
    };
    match inner.load() {
        Ok(image) => Box::into_raw(Box::new(GlycinNgImage { inner: image })),
        Err(e) => {
            set_error(e);
            ptr::null_mut()
        }
    }
}

/// Free an image handle. Safe to call on NULL.
///
/// # Safety
///
/// `image` must have been returned by `glycin_ng_loader_load` and
/// must not have already been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_image_free(image: *mut GlycinNgImage) {
    if !image.is_null() {
        drop(unsafe { Box::from_raw(image) });
    }
}

fn image_ref<'a>(image: *const GlycinNgImage) -> Option<&'a Image> {
    unsafe { image.as_ref() }.map(|h| &h.inner)
}

/// Width of the image in pixels (0 if `image` is NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_image_width(image: *const GlycinNgImage) -> u32 {
    image_ref(image).map(|i| i.width()).unwrap_or(0)
}

/// Height of the image in pixels (0 if `image` is NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_image_height(image: *const GlycinNgImage) -> u32 {
    image_ref(image).map(|i| i.height()).unwrap_or(0)
}

/// Number of frames in the image (0 if `image` is NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_image_frame_count(image: *const GlycinNgImage) -> usize {
    image_ref(image).map(|i| i.frames().len()).unwrap_or(0)
}

/// Whether the image has more than one frame (non-zero == animated,
/// 0 == still or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_image_is_animated(image: *const GlycinNgImage) -> c_int {
    image_ref(image)
        .map(|i| i.is_animated() as c_int)
        .unwrap_or(0)
}

/// EXIF orientation value (1..=8) for the decoded image, or `1`
/// (identity) when `image` is NULL or has no orientation set. The
/// values follow the EXIF spec: `1` = upright, `3` = rotated 180,
/// `6` = rotated 90 CW, `8` = rotated 90 CCW, etc.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_image_orientation(image: *const GlycinNgImage) -> u16 {
    image_ref(image)
        .map(|i| i.orientation().exif_value())
        .unwrap_or(1)
}

/// Short lowercase format name (e.g. "png"). Returns NULL if
/// `image` is NULL. The pointer is valid for the lifetime of the
/// image handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_image_format_name(image: *const GlycinNgImage) -> *const c_char {
    let Some(img) = image_ref(image) else {
        return ptr::null();
    };
    // The format-name strings are 'static and ASCII; we can return a
    // pointer into the byte buffer plus a trailing NUL via a small
    // lookup table.
    format_name_cstr(img.format_name())
}

fn format_name_cstr(name: &'static str) -> *const c_char {
    // The names are static ASCII, but Rust string slices are not
    // NUL-terminated. Lookup table of every value the decoders return.
    const NAMES: &[(&str, &CStr)] = &[
        ("png", c"png"),
        ("jpeg", c"jpeg"),
        ("gif", c"gif"),
        ("webp", c"webp"),
        ("tiff", c"tiff"),
        ("bmp", c"bmp"),
        ("ico", c"ico"),
        ("tga", c"tga"),
        ("qoi", c"qoi"),
        ("exr", c"exr"),
        ("pnm", c"pnm"),
        ("dds", c"dds"),
        ("jxl", c"jxl"),
    ];
    for (rust, cstr) in NAMES {
        if *rust == name {
            return cstr.as_ptr();
        }
    }
    ptr::null()
}

fn frame_ref<'a>(image: *const GlycinNgImage, index: usize) -> Option<&'a Frame> {
    let img = image_ref(image)?;
    img.frames().get(index)
}

/// Texture of the frame at `index` (NULL on out-of-bounds or NULL
/// image). The pointer remains valid for the lifetime of the image
/// handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_image_texture(
    image: *const GlycinNgImage,
    index: usize,
) -> *const Texture {
    match frame_ref(image, index) {
        Some(frame) => frame.texture() as *const Texture,
        None => ptr::null(),
    }
}

/// Frame delay for animation, in milliseconds (0 if the frame is
/// not animated or `image` is NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_image_frame_delay_ms(
    image: *const GlycinNgImage,
    index: usize,
) -> u64 {
    frame_ref(image, index)
        .and_then(|f| f.delay())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Width of the texture in pixels.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_texture_width(texture: *const Texture) -> u32 {
    unsafe { texture.as_ref() }.map(|t| t.width()).unwrap_or(0)
}

/// Height of the texture in pixels.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_texture_height(texture: *const Texture) -> u32 {
    unsafe { texture.as_ref() }.map(|t| t.height()).unwrap_or(0)
}

/// Stride in bytes between successive rows.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_texture_stride(texture: *const Texture) -> u32 {
    unsafe { texture.as_ref() }.map(|t| t.stride()).unwrap_or(0)
}

/// Pixel memory format, encoded as one of the `GLYCIN_NG_FORMAT_*`
/// constants.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_texture_format(texture: *const Texture) -> c_uint {
    let Some(t) = (unsafe { texture.as_ref() }) else {
        return GLYCIN_NG_FORMAT_UNKNOWN;
    };
    memory_format_to_c_uint(t.format())
}

/// Raw pixel data pointer (length is
/// `glycin_ng_texture_data_len`). Returns NULL if `texture` is
/// NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_texture_data(texture: *const Texture) -> *const u8 {
    unsafe { texture.as_ref() }
        .map(|t| t.data().as_ptr())
        .unwrap_or(ptr::null())
}

/// Length in bytes of the texture's pixel data.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_texture_data_len(texture: *const Texture) -> usize {
    unsafe { texture.as_ref() }
        .map(|t| t.data().len())
        .unwrap_or(0)
}

/// Texture format constant: unknown (returned only for NULL inputs).
pub const GLYCIN_NG_FORMAT_UNKNOWN: c_uint = 0;
/// 8 bit grayscale.
pub const GLYCIN_NG_FORMAT_G8: c_uint = 1;
/// 8 bit grayscale with straight alpha.
pub const GLYCIN_NG_FORMAT_G8A8: c_uint = 2;
/// 8 bit grayscale with premultiplied alpha.
pub const GLYCIN_NG_FORMAT_G8A8_PRE: c_uint = 3;
/// 16 bit grayscale.
pub const GLYCIN_NG_FORMAT_G16: c_uint = 4;
/// 16 bit grayscale with straight alpha.
pub const GLYCIN_NG_FORMAT_G16A16: c_uint = 5;
/// 16 bit grayscale with premultiplied alpha.
pub const GLYCIN_NG_FORMAT_G16A16_PRE: c_uint = 6;
/// 8 bit per channel RGB.
pub const GLYCIN_NG_FORMAT_R8G8B8: c_uint = 10;
/// 8 bit per channel RGBA with straight alpha.
pub const GLYCIN_NG_FORMAT_R8G8B8A8: c_uint = 11;
/// 8 bit per channel RGBA with premultiplied alpha.
pub const GLYCIN_NG_FORMAT_R8G8B8A8_PRE: c_uint = 12;
/// 8 bit per channel BGR.
pub const GLYCIN_NG_FORMAT_B8G8R8: c_uint = 13;
/// 8 bit per channel BGRA with straight alpha.
pub const GLYCIN_NG_FORMAT_B8G8R8A8: c_uint = 14;
/// 8 bit per channel BGRA with premultiplied alpha.
pub const GLYCIN_NG_FORMAT_B8G8R8A8_PRE: c_uint = 15;
/// 8 bit per channel ARGB with straight alpha.
pub const GLYCIN_NG_FORMAT_A8R8G8B8: c_uint = 16;
/// 8 bit per channel ARGB with premultiplied alpha.
pub const GLYCIN_NG_FORMAT_A8R8G8B8_PRE: c_uint = 17;
/// 8 bit per channel ABGR with straight alpha.
pub const GLYCIN_NG_FORMAT_A8B8G8R8: c_uint = 18;
/// 16 bit per channel RGB.
pub const GLYCIN_NG_FORMAT_R16G16B16: c_uint = 20;
/// 16 bit per channel RGBA with straight alpha.
pub const GLYCIN_NG_FORMAT_R16G16B16A16: c_uint = 21;
/// 16 bit per channel RGBA with premultiplied alpha.
pub const GLYCIN_NG_FORMAT_R16G16B16A16_PRE: c_uint = 22;
/// IEEE 754 binary16 per channel RGB.
pub const GLYCIN_NG_FORMAT_R16G16B16_F: c_uint = 23;
/// IEEE 754 binary16 per channel RGBA.
pub const GLYCIN_NG_FORMAT_R16G16B16A16_F: c_uint = 24;
/// IEEE 754 binary32 per channel RGB.
pub const GLYCIN_NG_FORMAT_R32G32B32_F: c_uint = 25;
/// IEEE 754 binary32 per channel RGBA with straight alpha.
pub const GLYCIN_NG_FORMAT_R32G32B32A32_F: c_uint = 26;
/// IEEE 754 binary32 per channel RGBA with premultiplied alpha.
pub const GLYCIN_NG_FORMAT_R32G32B32A32_F_PRE: c_uint = 27;

fn memory_format_to_c_uint(f: MemoryFormat) -> c_uint {
    match f {
        MemoryFormat::G8 => GLYCIN_NG_FORMAT_G8,
        MemoryFormat::G8a8 => GLYCIN_NG_FORMAT_G8A8,
        MemoryFormat::G8a8Premultiplied => GLYCIN_NG_FORMAT_G8A8_PRE,
        MemoryFormat::G16 => GLYCIN_NG_FORMAT_G16,
        MemoryFormat::G16a16 => GLYCIN_NG_FORMAT_G16A16,
        MemoryFormat::G16a16Premultiplied => GLYCIN_NG_FORMAT_G16A16_PRE,
        MemoryFormat::R8g8b8 => GLYCIN_NG_FORMAT_R8G8B8,
        MemoryFormat::R8g8b8a8 => GLYCIN_NG_FORMAT_R8G8B8A8,
        MemoryFormat::R8g8b8a8Premultiplied => GLYCIN_NG_FORMAT_R8G8B8A8_PRE,
        MemoryFormat::B8g8r8 => GLYCIN_NG_FORMAT_B8G8R8,
        MemoryFormat::B8g8r8a8 => GLYCIN_NG_FORMAT_B8G8R8A8,
        MemoryFormat::B8g8r8a8Premultiplied => GLYCIN_NG_FORMAT_B8G8R8A8_PRE,
        MemoryFormat::A8r8g8b8 => GLYCIN_NG_FORMAT_A8R8G8B8,
        MemoryFormat::A8r8g8b8Premultiplied => GLYCIN_NG_FORMAT_A8R8G8B8_PRE,
        MemoryFormat::A8b8g8r8 => GLYCIN_NG_FORMAT_A8B8G8R8,
        MemoryFormat::R16g16b16 => GLYCIN_NG_FORMAT_R16G16B16,
        MemoryFormat::R16g16b16a16 => GLYCIN_NG_FORMAT_R16G16B16A16,
        MemoryFormat::R16g16b16a16Premultiplied => GLYCIN_NG_FORMAT_R16G16B16A16_PRE,
        MemoryFormat::R16g16b16Float => GLYCIN_NG_FORMAT_R16G16B16_F,
        MemoryFormat::R16g16b16a16Float => GLYCIN_NG_FORMAT_R16G16B16A16_F,
        MemoryFormat::R32g32b32Float => GLYCIN_NG_FORMAT_R32G32B32_F,
        MemoryFormat::R32g32b32a32Float => GLYCIN_NG_FORMAT_R32G32B32A32_F,
        MemoryFormat::R32g32b32a32FloatPremultiplied => GLYCIN_NG_FORMAT_R32G32B32A32_F_PRE,
    }
}

fn c_uint_to_memory_format(value: c_uint) -> Option<MemoryFormat> {
    Some(match value {
        GLYCIN_NG_FORMAT_G8 => MemoryFormat::G8,
        GLYCIN_NG_FORMAT_G8A8 => MemoryFormat::G8a8,
        GLYCIN_NG_FORMAT_G8A8_PRE => MemoryFormat::G8a8Premultiplied,
        GLYCIN_NG_FORMAT_G16 => MemoryFormat::G16,
        GLYCIN_NG_FORMAT_G16A16 => MemoryFormat::G16a16,
        GLYCIN_NG_FORMAT_G16A16_PRE => MemoryFormat::G16a16Premultiplied,
        GLYCIN_NG_FORMAT_R8G8B8 => MemoryFormat::R8g8b8,
        GLYCIN_NG_FORMAT_R8G8B8A8 => MemoryFormat::R8g8b8a8,
        GLYCIN_NG_FORMAT_R8G8B8A8_PRE => MemoryFormat::R8g8b8a8Premultiplied,
        GLYCIN_NG_FORMAT_B8G8R8 => MemoryFormat::B8g8r8,
        GLYCIN_NG_FORMAT_B8G8R8A8 => MemoryFormat::B8g8r8a8,
        GLYCIN_NG_FORMAT_B8G8R8A8_PRE => MemoryFormat::B8g8r8a8Premultiplied,
        GLYCIN_NG_FORMAT_A8R8G8B8 => MemoryFormat::A8r8g8b8,
        GLYCIN_NG_FORMAT_A8R8G8B8_PRE => MemoryFormat::A8r8g8b8Premultiplied,
        GLYCIN_NG_FORMAT_A8B8G8R8 => MemoryFormat::A8b8g8r8,
        GLYCIN_NG_FORMAT_R16G16B16 => MemoryFormat::R16g16b16,
        GLYCIN_NG_FORMAT_R16G16B16A16 => MemoryFormat::R16g16b16a16,
        GLYCIN_NG_FORMAT_R16G16B16A16_PRE => MemoryFormat::R16g16b16a16Premultiplied,
        GLYCIN_NG_FORMAT_R16G16B16_F => MemoryFormat::R16g16b16Float,
        GLYCIN_NG_FORMAT_R16G16B16A16_F => MemoryFormat::R16g16b16a16Float,
        GLYCIN_NG_FORMAT_R32G32B32_F => MemoryFormat::R32g32b32Float,
        GLYCIN_NG_FORMAT_R32G32B32A32_F => MemoryFormat::R32g32b32a32Float,
        GLYCIN_NG_FORMAT_R32G32B32A32_F_PRE => MemoryFormat::R32g32b32a32FloatPremultiplied,
        _ => return None,
    })
}

/// Known-format constant: PNG / APNG.
pub const GLYCIN_NG_KFMT_PNG: c_uint = 1;
/// Known-format constant: JPEG.
pub const GLYCIN_NG_KFMT_JPEG: c_uint = 2;
/// Known-format constant: GIF.
pub const GLYCIN_NG_KFMT_GIF: c_uint = 3;
/// Known-format constant: WebP.
pub const GLYCIN_NG_KFMT_WEBP: c_uint = 4;
/// Known-format constant: TIFF.
pub const GLYCIN_NG_KFMT_TIFF: c_uint = 5;
/// Known-format constant: BMP.
pub const GLYCIN_NG_KFMT_BMP: c_uint = 6;
/// Known-format constant: ICO / CUR.
pub const GLYCIN_NG_KFMT_ICO: c_uint = 7;
/// Known-format constant: TGA.
pub const GLYCIN_NG_KFMT_TGA: c_uint = 8;
/// Known-format constant: QOI.
pub const GLYCIN_NG_KFMT_QOI: c_uint = 9;
/// Known-format constant: OpenEXR.
pub const GLYCIN_NG_KFMT_EXR: c_uint = 10;
/// Known-format constant: PNM family.
pub const GLYCIN_NG_KFMT_PNM: c_uint = 11;
/// Known-format constant: DDS.
pub const GLYCIN_NG_KFMT_DDS: c_uint = 12;
/// Known-format constant: JPEG XL.
pub const GLYCIN_NG_KFMT_JXL: c_uint = 13;
/// Known-format constant: SVG.
pub const GLYCIN_NG_KFMT_SVG: c_uint = 14;

fn c_uint_to_format(value: c_uint) -> Option<KnownFormat> {
    Some(match value {
        GLYCIN_NG_KFMT_PNG => KnownFormat::Png,
        GLYCIN_NG_KFMT_JPEG => KnownFormat::Jpeg,
        GLYCIN_NG_KFMT_GIF => KnownFormat::Gif,
        GLYCIN_NG_KFMT_WEBP => KnownFormat::WebP,
        GLYCIN_NG_KFMT_TIFF => KnownFormat::Tiff,
        GLYCIN_NG_KFMT_BMP => KnownFormat::Bmp,
        GLYCIN_NG_KFMT_ICO => KnownFormat::Ico,
        GLYCIN_NG_KFMT_TGA => KnownFormat::Tga,
        GLYCIN_NG_KFMT_QOI => KnownFormat::Qoi,
        GLYCIN_NG_KFMT_EXR => KnownFormat::Exr,
        GLYCIN_NG_KFMT_PNM => KnownFormat::Pnm,
        GLYCIN_NG_KFMT_DDS => KnownFormat::Dds,
        GLYCIN_NG_KFMT_JXL => KnownFormat::Jxl,
        GLYCIN_NG_KFMT_SVG => KnownFormat::Svg,
        _ => return None,
    })
}

fn format_to_c_uint(f: KnownFormat) -> c_uint {
    match f {
        KnownFormat::Png => GLYCIN_NG_KFMT_PNG,
        KnownFormat::Jpeg => GLYCIN_NG_KFMT_JPEG,
        KnownFormat::Gif => GLYCIN_NG_KFMT_GIF,
        KnownFormat::WebP => GLYCIN_NG_KFMT_WEBP,
        KnownFormat::Tiff => GLYCIN_NG_KFMT_TIFF,
        KnownFormat::Bmp => GLYCIN_NG_KFMT_BMP,
        KnownFormat::Ico => GLYCIN_NG_KFMT_ICO,
        KnownFormat::Tga => GLYCIN_NG_KFMT_TGA,
        KnownFormat::Qoi => GLYCIN_NG_KFMT_QOI,
        KnownFormat::Exr => GLYCIN_NG_KFMT_EXR,
        KnownFormat::Pnm => GLYCIN_NG_KFMT_PNM,
        KnownFormat::Dds => GLYCIN_NG_KFMT_DDS,
        KnownFormat::Jxl => GLYCIN_NG_KFMT_JXL,
        KnownFormat::Svg => GLYCIN_NG_KFMT_SVG,
    }
}

/// Resolve an IANA media type (e.g. `"image/png"`) to a known-format
/// constant. Returns one of the `GLYCIN_NG_KFMT_*` values on success,
/// or 0 if the MIME type is unknown or the input is invalid.
///
/// # Safety
///
/// `mime` must be a valid NUL-terminated C string, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_known_format_from_mime(mime: *const c_char) -> c_uint {
    if mime.is_null() {
        return 0;
    }
    let Ok(s) = (unsafe { CStr::from_ptr(mime) }).to_str() else {
        return 0;
    };
    KnownFormat::from_mime_type(s)
        .map(format_to_c_uint)
        .unwrap_or(0)
}

/// Resolve a filename extension (case-insensitive, no leading dot)
/// to a known-format constant. Returns one of the `GLYCIN_NG_KFMT_*`
/// values on success, or 0 if the extension is unknown or the input
/// is invalid.
///
/// # Safety
///
/// `ext` must be a valid NUL-terminated C string, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_known_format_from_extension(ext: *const c_char) -> c_uint {
    if ext.is_null() {
        return 0;
    }
    let Ok(s) = (unsafe { CStr::from_ptr(ext) }).to_str() else {
        return 0;
    };
    KnownFormat::from_extension(s)
        .map(format_to_c_uint)
        .unwrap_or(0)
}

// ---------- Encoder ----------

/// Opaque [`Encoder`] handle. Created by [`glycin_ng_encoder_new`]
/// and freed by [`glycin_ng_encoder_free`]. All `glycin_ng_encoder_*`
/// setters mutate the encoder in place. [`glycin_ng_encoder_encode`]
/// can be called multiple times on the same handle; the queued
/// frames and configured options stay on the encoder until it is
/// freed.
#[repr(C)]
pub struct GlycinNgEncoder {
    inner: Encoder,
}

/// Opaque handle wrapping the bytes produced by
/// [`glycin_ng_encoder_encode`]. The buffer is owned by the handle;
/// release it with [`glycin_ng_encoded_image_free`].
#[repr(C)]
pub struct GlycinNgEncodedImage {
    bytes: Vec<u8>,
}

/// Construct an encoder for `format`, which must be one of the
/// `GLYCIN_NG_KFMT_*` constants. Returns NULL on error (unknown
/// constant or no encoder compiled in for that target); see
/// [`glycin_ng_last_error`] for the reason.
#[unsafe(no_mangle)]
pub extern "C" fn glycin_ng_encoder_new(format: c_uint) -> *mut GlycinNgEncoder {
    clear_error();
    let Some(target) = c_uint_to_format(format) else {
        set_error("unknown known-format constant");
        return ptr::null_mut();
    };
    match Encoder::new(target) {
        Ok(enc) => Box::into_raw(Box::new(GlycinNgEncoder { inner: enc })),
        Err(e) => {
            set_error(e);
            ptr::null_mut()
        }
    }
}

/// Free an encoder handle. Safe to call on NULL.
///
/// # Safety
///
/// `encoder` must have been returned by [`glycin_ng_encoder_new`]
/// and must not have already been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_encoder_free(encoder: *mut GlycinNgEncoder) {
    if !encoder.is_null() {
        drop(unsafe { Box::from_raw(encoder) });
    }
}

/// Set the lossy quality (0..=100) for codecs that honor it (JPEG).
/// Lossless codecs ignore the value.
///
/// # Safety
///
/// `encoder` must be a valid pointer returned by
/// [`glycin_ng_encoder_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_encoder_set_quality(encoder: *mut GlycinNgEncoder, quality: u8) {
    if let Some(handle) = unsafe { encoder.as_mut() } {
        handle.inner.set_quality(quality);
    }
}

/// Set the compression level. Retained on the encoder; current
/// `image`-crate encoders ignore the value.
///
/// # Safety
///
/// `encoder` must be a valid pointer returned by
/// [`glycin_ng_encoder_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_encoder_set_compression(
    encoder: *mut GlycinNgEncoder,
    compression: u8,
) {
    if let Some(handle) = unsafe { encoder.as_mut() } {
        handle.inner.set_compression(compression);
    }
}

/// Attach an ICC profile to the encoder. Pass `data = NULL` and
/// `len = 0` to clear a previously-attached profile. Returns 0 on
/// success, -1 on error.
///
/// # Safety
///
/// `encoder` must be a valid pointer returned by
/// [`glycin_ng_encoder_new`]. `data` must point to at least `len`
/// bytes, or both must be null/zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_encoder_set_icc_profile(
    encoder: *mut GlycinNgEncoder,
    data: *const u8,
    len: usize,
) -> c_int {
    clear_error();
    let Some(handle) = (unsafe { encoder.as_mut() }) else {
        set_error("encoder is null");
        return -1;
    };
    if data.is_null() && len != 0 {
        set_error("icc data is null but len is non-zero");
        return -1;
    }
    let icc = if data.is_null() || len == 0 {
        None
    } else {
        Some(unsafe { slice::from_raw_parts(data, len) }.to_vec())
    };
    handle.inner.set_icc_profile(icc);
    0
}

/// Attach a metadata key/value pair to the encoder. Both strings
/// must be NUL-terminated and valid UTF-8. Returns 0 on success,
/// -1 on error.
///
/// # Safety
///
/// `encoder` must be a valid pointer returned by
/// [`glycin_ng_encoder_new`]. `key` and `value` must be valid
/// NUL-terminated C strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_encoder_add_metadata(
    encoder: *mut GlycinNgEncoder,
    key: *const c_char,
    value: *const c_char,
) -> c_int {
    clear_error();
    let Some(handle) = (unsafe { encoder.as_mut() }) else {
        set_error("encoder is null");
        return -1;
    };
    if key.is_null() || value.is_null() {
        set_error("key or value is null");
        return -1;
    }
    let k = match unsafe { CStr::from_ptr(key) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => {
            set_error("metadata key is not valid UTF-8");
            return -1;
        }
    };
    let v = match unsafe { CStr::from_ptr(value) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => {
            set_error("metadata value is not valid UTF-8");
            return -1;
        }
    };
    handle.inner.add_metadata(k, v);
    0
}

/// Queue a frame for encoding. The bytes pointed to by `data` are
/// copied; the caller may free `data` after the call returns.
/// `format` must be one of the `GLYCIN_NG_FORMAT_*` constants.
/// Returns 0 on success, -1 on error.
///
/// # Safety
///
/// `encoder` must be a valid pointer returned by
/// [`glycin_ng_encoder_new`]. `data` must point to at least
/// `data_len` bytes (or both may be null/zero, though such a frame
/// will be rejected at encode time).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_encoder_add_frame(
    encoder: *mut GlycinNgEncoder,
    width: u32,
    height: u32,
    stride: u32,
    format: c_uint,
    data: *const u8,
    data_len: usize,
) -> c_int {
    clear_error();
    let Some(handle) = (unsafe { encoder.as_mut() }) else {
        set_error("encoder is null");
        return -1;
    };
    let Some(mf) = c_uint_to_memory_format(format) else {
        set_error("unknown memory-format constant");
        return -1;
    };
    if data.is_null() && data_len != 0 {
        set_error("data is null but data_len is non-zero");
        return -1;
    }
    let bytes: Vec<u8> = if data.is_null() || data_len == 0 {
        Vec::new()
    } else {
        unsafe { slice::from_raw_parts(data, data_len) }.to_vec()
    };
    handle.inner.add_frame(EncodeFrame {
        width,
        height,
        stride,
        format: mf,
        data: bytes,
    });
    0
}

/// Encode the queued frames into a new [`GlycinNgEncodedImage`].
/// Returns NULL on error; see [`glycin_ng_last_error`].
///
/// # Safety
///
/// `encoder` must be a valid pointer returned by
/// [`glycin_ng_encoder_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_encoder_encode(
    encoder: *mut GlycinNgEncoder,
) -> *mut GlycinNgEncodedImage {
    clear_error();
    let Some(handle) = (unsafe { encoder.as_mut() }) else {
        set_error("encoder is null");
        return ptr::null_mut();
    };
    match handle.inner.encode() {
        Ok(bytes) => Box::into_raw(Box::new(GlycinNgEncodedImage { bytes })),
        Err(e) => {
            set_error(e);
            ptr::null_mut()
        }
    }
}

/// Free an encoded-image handle. Safe to call on NULL.
///
/// # Safety
///
/// `image` must have been returned by [`glycin_ng_encoder_encode`]
/// and must not have already been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_encoded_image_free(image: *mut GlycinNgEncodedImage) {
    if !image.is_null() {
        drop(unsafe { Box::from_raw(image) });
    }
}

/// Borrow the encoded bytes from the handle. The pointer is valid
/// until the handle is freed. Returns NULL when `image` is NULL.
///
/// # Safety
///
/// `image` must be a valid pointer returned by
/// [`glycin_ng_encoder_encode`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_encoded_image_data(
    image: *const GlycinNgEncodedImage,
) -> *const u8 {
    unsafe { image.as_ref() }
        .map(|i| i.bytes.as_ptr())
        .unwrap_or(ptr::null())
}

/// Length in bytes of the encoded image. Returns 0 when `image` is
/// NULL.
///
/// # Safety
///
/// `image` must be a valid pointer returned by
/// [`glycin_ng_encoder_encode`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glycin_ng_encoded_image_len(image: *const GlycinNgEncodedImage) -> usize {
    unsafe { image.as_ref() }
        .map(|i| i.bytes.len())
        .unwrap_or(0)
}

// Silence the unused-import warning when the Error variant is only
// used inside set_error().
#[allow(dead_code)]
fn _dummy(_: &Error) {}
