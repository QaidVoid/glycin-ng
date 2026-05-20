//! gdk-pixbuf loader module backed by `glycin-ng`.
//!
//! Build this crate to produce `libpixbufloader_glycin_ng.so`. Drop
//! the shared object into the gdk-pixbuf loaders directory (usually
//! `${libdir}/gdk-pixbuf-2.0/2.10.0/loaders/`) and run
//! `gdk-pixbuf-query-loaders --update-cache` so the framework
//! registers the new MIME types.
//!
//! The plugin exposes the two entry points gdk-pixbuf calls during
//! cache population: `fill_vtable` populates the function-pointer
//! table on the supplied [`GdkPixbufModule`], and `fill_info`
//! advertises the formats we can decode.
//!
//! Decoding goes through the same sandboxed worker thread that
//! powers [`glycin_ng::Loader`], so every call is landlocked and
//! seccomped (when both kernel features are available).

mod convert;
mod ffi;

use std::ffi::{CString, c_char, c_int, c_uint, c_void};
use std::ptr;
use std::slice;
use std::sync::OnceLock;

use glycin_ng::{Image, Loader};

use crate::convert::texture_to_rgba8;
use crate::ffi::{
    GDK_COLORSPACE_RGB, GDK_PIXBUF_FORMAT_THREADSAFE, GError, GdkPixbuf, GdkPixbufFormat,
    GdkPixbufModule, GdkPixbufModulePattern, GdkPixbufModulePreparedFunc, GdkPixbufModuleSizeFunc,
    GdkPixbufModuleUpdatedFunc,
};

/// Populate the supplied module's vtable with our entry points.
///
/// Called once per loaded module by gdk-pixbuf during cache
/// population and at runtime when the framework opens this `.so`.
///
/// # Safety
///
/// `module` must point to a `GdkPixbufModule` allocated by
/// gdk-pixbuf; the layout must match the one we declare in
/// [`ffi::GdkPixbufModule`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fill_vtable(module: *mut GdkPixbufModule) {
    if module.is_null() {
        return;
    }
    unsafe {
        (*module).load = Some(load);
        (*module).load_xpm_data = None;
        (*module).begin_load = Some(begin_load);
        (*module).stop_load = Some(stop_load);
        (*module).load_increment = Some(load_increment);
        (*module).load_animation = Some(load_animation);
        (*module).save = None;
        (*module).save_to_callback = None;
        (*module).is_save_option_supported = None;
    }
}

/// Advertise the formats this loader handles.
///
/// gdk-pixbuf reads the populated [`GdkPixbufFormat`] when building
/// `loaders.cache`. Strings are statically allocated and must stay
/// valid for the program lifetime.
///
/// # Safety
///
/// `info` must point to a writable `GdkPixbufFormat`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fill_info(info: *mut GdkPixbufFormat) {
    if info.is_null() {
        return;
    }
    let signatures = signatures();
    unsafe {
        (*info).name = c"glycin_ng".as_ptr();
        (*info).signature = signatures.as_ptr() as *mut GdkPixbufModulePattern;
        (*info).domain = c"glycin_ng".as_ptr();
        (*info).description = c"glycin-ng image loader".as_ptr();
        (*info).mime_types = mime_types().as_ptr() as *mut *const c_char;
        (*info).extensions = extensions().as_ptr() as *mut *const c_char;
        (*info).flags = GDK_PIXBUF_FORMAT_THREADSAFE;
        (*info).disabled = 0;
        (*info).license = c"MIT OR Apache-2.0".as_ptr();
    }
}

// Static signature byte strings. Each pattern matches gdk-pixbuf's
// `prefix_matches` rules: matching stops at the first NUL byte in
// `prefix` (unless a `mask` extends it), so embedded NULs cannot be
// used and must be skipped via a mask. The trailing `\0` on every
// slice is the C-string terminator gdk-pixbuf reads against.
struct SigDef {
    prefix: &'static [u8],
    mask: Option<&'static [u8]>,
    relevance: c_int,
}

const SIG_DEFS: &[SigDef] = &[
    SigDef {
        prefix: b"\x89PNG\r\n\x1a\n\0",
        mask: None,
        relevance: 100,
    },
    SigDef {
        prefix: b"\xff\xd8\xff\0",
        mask: None,
        relevance: 100,
    },
    SigDef {
        prefix: b"GIF87a\0",
        mask: None,
        relevance: 100,
    },
    SigDef {
        prefix: b"GIF89a\0",
        mask: None,
        relevance: 100,
    },
    SigDef {
        prefix: b"RIFF    WEBP\0",
        mask: Some(b"xxxx    xxxx\0"),
        relevance: 100,
    },
    SigDef {
        prefix: b"II*\0",
        mask: None,
        relevance: 80,
    },
    SigDef {
        prefix: b"MM\0",
        mask: None,
        relevance: 80,
    },
    SigDef {
        prefix: b"BM\0",
        mask: None,
        relevance: 60,
    },
    SigDef {
        prefix: b"qoif\0",
        mask: None,
        relevance: 100,
    },
    SigDef {
        prefix: b"\xff\x0a\0",
        mask: None,
        relevance: 100,
    },
    SigDef {
        prefix: b"DDS \0",
        mask: None,
        relevance: 100,
    },
    SigDef {
        prefix: b"\x76\x2f\x31\x01\0",
        mask: None,
        relevance: 100,
    },
    // ICO: magic 00 00 01 00. Placeholder bytes at NUL positions
    // ('a'), mask 'z' asserts each placeholder byte is actually 0 in
    // the input; 'x' at position 2 enforces the literal 0x01.
    SigDef {
        prefix: b"aa\x01a\0",
        mask: Some(b"zzxz\0"),
        relevance: 100,
    },
    // CUR: magic 00 00 02 00.
    SigDef {
        prefix: b"aa\x02a\0",
        mask: Some(b"zzxz\0"),
        relevance: 100,
    },
    // JXL container box: 00 00 00 0c 4A 58 4C 20 0D 0A 87 0A.
    SigDef {
        prefix: b"aaa\x0cJXL \r\n\x87\n\0",
        mask: Some(b"zzzxxxxxxxxx\0"),
        relevance: 100,
    },
];

struct Patterns(Vec<GdkPixbufModulePattern>);
struct PointerList(Vec<*const c_char>);

// SAFETY: both wrappers hold pointers that target `'static` byte
// slices compiled into the binary. The vectors are populated once
// inside `get_or_init` and never mutated again. No interior
// mutability means concurrent reads through the shared `&'static`
// reference are sound.
unsafe impl Sync for Patterns {}
unsafe impl Send for Patterns {}
unsafe impl Sync for PointerList {}
unsafe impl Send for PointerList {}

fn signatures() -> &'static [GdkPixbufModulePattern] {
    static CELL: OnceLock<Patterns> = OnceLock::new();
    let inner = CELL.get_or_init(|| {
        let mut v: Vec<GdkPixbufModulePattern> = SIG_DEFS
            .iter()
            .map(|d| GdkPixbufModulePattern {
                prefix: d.prefix.as_ptr() as *const c_char,
                mask: d
                    .mask
                    .map(|m| m.as_ptr() as *const c_char)
                    .unwrap_or(ptr::null()),
                relevance: d.relevance,
            })
            .collect();
        v.push(GdkPixbufModulePattern {
            prefix: ptr::null(),
            mask: ptr::null(),
            relevance: 0,
        });
        Patterns(v)
    });
    &inner.0
}

fn mime_types() -> &'static [*const c_char] {
    static CELL: OnceLock<PointerList> = OnceLock::new();
    let inner = CELL.get_or_init(|| {
        const NAMES: &[&[u8]] = &[
            b"image/png\0",
            b"image/apng\0",
            b"image/jpeg\0",
            b"image/gif\0",
            b"image/webp\0",
            b"image/tiff\0",
            b"image/bmp\0",
            b"image/x-bmp\0",
            b"image/x-ico\0",
            b"image/x-icon\0",
            b"image/vnd.microsoft.icon\0",
            b"image/x-tga\0",
            b"image/x-targa\0",
            b"image/x-qoi\0",
            b"image/x-exr\0",
            b"image/x-portable-anymap\0",
            b"image/x-portable-bitmap\0",
            b"image/x-portable-graymap\0",
            b"image/x-portable-pixmap\0",
            b"image/vnd-ms.dds\0",
            b"image/jxl\0",
        ];
        let mut v: Vec<*const c_char> = NAMES.iter().map(|n| n.as_ptr() as *const c_char).collect();
        v.push(ptr::null());
        PointerList(v)
    });
    &inner.0
}

fn extensions() -> &'static [*const c_char] {
    static CELL: OnceLock<PointerList> = OnceLock::new();
    let inner = CELL.get_or_init(|| {
        const EXTS: &[&[u8]] = &[
            b"png\0", b"apng\0", b"jpg\0", b"jpeg\0", b"jpe\0", b"jfif\0", b"gif\0", b"webp\0",
            b"tif\0", b"tiff\0", b"bmp\0", b"dib\0", b"ico\0", b"cur\0", b"tga\0", b"qoi\0",
            b"exr\0", b"pbm\0", b"pgm\0", b"ppm\0", b"pnm\0", b"pam\0", b"dds\0", b"jxl\0",
        ];
        let mut v: Vec<*const c_char> = EXTS.iter().map(|e| e.as_ptr() as *const c_char).collect();
        v.push(ptr::null());
        PointerList(v)
    });
    &inner.0
}

/// Incremental loading state owned by gdk-pixbuf. Returned from
/// [`begin_load`] as an opaque `gpointer`, mutated by
/// [`load_increment`], consumed by [`stop_load`].
///
/// glycin-ng decoders are not streaming, so we accumulate the entire
/// input buffer here and decode it in one shot on `stop_load`. The
/// `prepared_func` and `updated_func` callbacks fire once at that
/// point with the fully-decoded pixbuf.
struct LoadContext {
    buffer: Vec<u8>,
    size_func: Option<GdkPixbufModuleSizeFunc>,
    prepared_func: Option<GdkPixbufModulePreparedFunc>,
    updated_func: Option<GdkPixbufModuleUpdatedFunc>,
    user_data: *mut c_void,
}

unsafe extern "C" fn begin_load(
    size_func: Option<GdkPixbufModuleSizeFunc>,
    prepared_func: Option<GdkPixbufModulePreparedFunc>,
    updated_func: Option<GdkPixbufModuleUpdatedFunc>,
    user_data: *mut c_void,
    _error: *mut *mut GError,
) -> *mut c_void {
    let ctx = Box::new(LoadContext {
        buffer: Vec::new(),
        size_func,
        prepared_func,
        updated_func,
        user_data,
    });
    Box::into_raw(ctx) as *mut c_void
}

unsafe extern "C" fn load_increment(
    context: *mut c_void,
    buf: *const u8,
    size: c_uint,
    error: *mut *mut GError,
) -> c_int {
    if context.is_null() {
        unsafe { set_error(error, "load_increment called with null context") };
        return 0;
    }
    if buf.is_null() || size == 0 {
        return 1;
    }
    let ctx = unsafe { &mut *(context as *mut LoadContext) };
    let slice = unsafe { slice::from_raw_parts(buf, size as usize) };
    ctx.buffer.extend_from_slice(slice);
    1
}

unsafe extern "C" fn stop_load(context: *mut c_void, error: *mut *mut GError) -> c_int {
    if context.is_null() {
        unsafe { set_error(error, "stop_load called with null context") };
        return 0;
    }
    let ctx = unsafe { Box::from_raw(context as *mut LoadContext) };
    let LoadContext {
        buffer,
        size_func,
        prepared_func,
        updated_func,
        user_data,
    } = *ctx;

    let image = match Loader::new_bytes(buffer).load() {
        Ok(img) => img,
        Err(e) => {
            unsafe { set_error(error, &e.to_string()) };
            return 0;
        }
    };

    let pixbuf = unsafe { image_to_pixbuf(&image, error) };
    if pixbuf.is_null() {
        return 0;
    }

    // Optional size hint. Callers that don't care leave `size_func`
    // NULL; we still report the actual dimensions so resize callbacks
    // can apply.
    if let Some(size_func) = size_func {
        let mut w = unsafe { crate::ffi::gdk_pixbuf_get_width(pixbuf) };
        let mut h = unsafe { crate::ffi::gdk_pixbuf_get_height(pixbuf) };
        unsafe { size_func(&mut w, &mut h, user_data) };
    }

    if let Some(prepared) = prepared_func {
        unsafe { prepared(pixbuf, ptr::null_mut(), user_data) };
    }
    if let Some(updated) = updated_func {
        let w = unsafe { crate::ffi::gdk_pixbuf_get_width(pixbuf) };
        let h = unsafe { crate::ffi::gdk_pixbuf_get_height(pixbuf) };
        unsafe { updated(pixbuf, 0, 0, w, h, user_data) };
    }

    // The prepared callback inside GdkPixbufLoader takes its own ref
    // via g_object_ref; release ours so the only outstanding ref
    // belongs to the consumer.
    unsafe { crate::ffi::g_object_unref(pixbuf as *mut c_void) };

    1
}

/// `GdkPixbufModuleLoadFunc`: read the entire image from a `FILE*`
/// stream, decode through glycin-ng, and hand back a freshly
/// allocated `GdkPixbuf`.
unsafe extern "C" fn load(file: *mut libc::FILE, error: *mut *mut GError) -> *mut GdkPixbuf {
    let bytes = match unsafe { read_file_to_vec(file) } {
        Ok(b) => b,
        Err(msg) => {
            unsafe { set_error(error, &msg) };
            return ptr::null_mut();
        }
    };

    let image = match Loader::new_bytes(bytes).load() {
        Ok(img) => img,
        Err(e) => {
            unsafe { set_error(error, &e.to_string()) };
            return ptr::null_mut();
        }
    };

    unsafe { image_to_pixbuf(&image, error) }
}

unsafe fn read_file_to_vec(file: *mut libc::FILE) -> Result<Vec<u8>, String> {
    if file.is_null() {
        return Err("null FILE*".into());
    }
    let mut buf = Vec::new();
    let mut chunk = [0u8; 65536];
    loop {
        let n = unsafe { libc::fread(chunk.as_mut_ptr() as *mut c_void, 1, chunk.len(), file) };
        if n == 0 {
            if unsafe { libc::ferror(file) } != 0 {
                return Err("fread reported an error".into());
            }
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if n < chunk.len() {
            break;
        }
    }
    Ok(buf)
}

unsafe fn image_to_pixbuf(image: &Image, error: *mut *mut GError) -> *mut GdkPixbuf {
    let Some(frame) = image.first_frame() else {
        unsafe { set_error(error, "image contained no frames") };
        return ptr::null_mut();
    };
    unsafe { texture_to_pixbuf(frame.texture(), error) }
}

unsafe fn texture_to_pixbuf(
    texture: &glycin_ng::Texture,
    error: *mut *mut GError,
) -> *mut GdkPixbuf {
    let width = texture.width() as c_int;
    let height = texture.height() as c_int;
    let (rgba, rowstride) = texture_to_rgba8(texture);

    let gbytes = unsafe { crate::ffi::g_bytes_new(rgba.as_ptr() as *const c_void, rgba.len()) };
    if gbytes.is_null() {
        unsafe { set_error(error, "g_bytes_new returned NULL") };
        return ptr::null_mut();
    }

    let pixbuf = unsafe {
        crate::ffi::gdk_pixbuf_new_from_bytes(
            gbytes,
            GDK_COLORSPACE_RGB,
            1,
            8,
            width,
            height,
            rowstride as c_int,
        )
    };

    unsafe { crate::ffi::g_bytes_unref(gbytes) };

    if pixbuf.is_null() {
        unsafe { set_error(error, "gdk_pixbuf_new_from_bytes returned NULL") };
    }
    pixbuf
}

/// `GdkPixbufModuleLoadAnimationFunc`: decode every frame and return
/// a `GdkPixbufSimpleAnim`.
///
/// `GdkPixbufSimpleAnim` holds a single frame rate, so animations
/// with non-uniform per-frame delays are flattened to the average
/// delay across all frames. Per-frame timing requires a custom
/// `GdkPixbufAnimation` subclass and is not yet implemented.
unsafe extern "C" fn load_animation(file: *mut libc::FILE, error: *mut *mut GError) -> *mut c_void {
    let bytes = match unsafe { read_file_to_vec(file) } {
        Ok(b) => b,
        Err(msg) => {
            unsafe { set_error(error, &msg) };
            return ptr::null_mut();
        }
    };

    let image = match Loader::new_bytes(bytes).load() {
        Ok(img) => img,
        Err(e) => {
            unsafe { set_error(error, &e.to_string()) };
            return ptr::null_mut();
        }
    };

    let frames = image.frames();
    if frames.is_empty() {
        unsafe { set_error(error, "image contained no frames") };
        return ptr::null_mut();
    }

    let first = &frames[0];
    let width = first.texture().width() as c_int;
    let height = first.texture().height() as c_int;
    let rate = average_rate_fps(frames);

    let anim = unsafe { crate::ffi::gdk_pixbuf_simple_anim_new(width, height, rate) };
    if anim.is_null() {
        unsafe { set_error(error, "gdk_pixbuf_simple_anim_new returned NULL") };
        return ptr::null_mut();
    }
    unsafe { crate::ffi::gdk_pixbuf_simple_anim_set_loop(anim, 1) };

    let mut added = 0_usize;
    for frame in frames {
        let pixbuf = unsafe { texture_to_pixbuf(frame.texture(), ptr::null_mut()) };
        if pixbuf.is_null() {
            continue;
        }
        unsafe { crate::ffi::gdk_pixbuf_simple_anim_add_frame(anim, pixbuf) };
        unsafe { crate::ffi::g_object_unref(pixbuf as *mut c_void) };
        added += 1;
    }

    if added == 0 {
        unsafe { crate::ffi::g_object_unref(anim as *mut c_void) };
        unsafe { set_error(error, "no frames could be converted to pixbuf") };
        return ptr::null_mut();
    }

    anim as *mut c_void
}

fn average_rate_fps(frames: &[glycin_ng::Frame]) -> f32 {
    let mut total_ms: u64 = 0;
    let mut counted: u64 = 0;
    for f in frames {
        if let Some(d) = f.delay() {
            total_ms = total_ms.saturating_add(d.as_millis() as u64);
            counted += 1;
        }
    }
    if counted == 0 {
        return 10.0;
    }
    let avg_ms = (total_ms / counted).max(1);
    (1000.0 / avg_ms as f32).clamp(0.1, 120.0)
}

unsafe fn set_error(error: *mut *mut GError, msg: &str) {
    if error.is_null() {
        return;
    }
    let cmsg = match CString::new(msg) {
        Ok(c) => c,
        Err(_) => CString::new("error").unwrap(),
    };
    let domain = unsafe { crate::ffi::g_quark_from_static_string(c"glycin_ng".as_ptr()) };
    unsafe {
        crate::ffi::g_set_error_literal(error, domain, 0, cmsg.as_ptr());
    }
}

#[cfg(test)]
mod incremental_tests {
    //! Verify that the vtable wiring routes begin/increment/stop to
    //! the right functions, that the accumulator concatenates chunks
    //! in order, and that null/empty inputs are handled.
    //!
    //! These tests stop short of calling `stop_load` because the
    //! final decode reaches into `gdk_pixbuf_new_from_bytes` and
    //! friends, which require libgdk_pixbuf-2.0 at link time. Real
    //! end-to-end coverage lives in the manual install path.

    use super::*;
    use std::ffi::c_void;

    #[test]
    fn begin_load_returns_non_null_context() {
        let ctx = unsafe { begin_load(None, None, None, ptr::null_mut(), ptr::null_mut()) };
        assert!(!ctx.is_null());
        // Free the leaked LoadContext to satisfy miri / leak sanitizer.
        let _ = unsafe { Box::from_raw(ctx as *mut LoadContext) };
    }

    #[test]
    fn load_increment_appends_in_order() {
        let ctx = unsafe { begin_load(None, None, None, ptr::null_mut(), ptr::null_mut()) };
        let chunk_a: &[u8] = b"hello ";
        let chunk_b: &[u8] = b"world";
        let rc1 = unsafe {
            load_increment(
                ctx,
                chunk_a.as_ptr(),
                chunk_a.len() as c_uint,
                ptr::null_mut(),
            )
        };
        let rc2 = unsafe {
            load_increment(
                ctx,
                chunk_b.as_ptr(),
                chunk_b.len() as c_uint,
                ptr::null_mut(),
            )
        };
        assert_eq!(rc1, 1);
        assert_eq!(rc2, 1);

        let restored = unsafe { Box::from_raw(ctx as *mut LoadContext) };
        assert_eq!(restored.buffer, b"hello world");
    }

    #[test]
    fn load_increment_ignores_zero_size_and_null_buf() {
        let ctx = unsafe { begin_load(None, None, None, ptr::null_mut(), ptr::null_mut()) };
        let rc_zero = unsafe { load_increment(ctx, b"x".as_ptr(), 0, ptr::null_mut()) };
        let rc_null = unsafe { load_increment(ctx, ptr::null(), 4, ptr::null_mut()) };
        assert_eq!(rc_zero, 1);
        assert_eq!(rc_null, 1);
        let restored = unsafe { Box::from_raw(ctx as *mut LoadContext) };
        assert!(restored.buffer.is_empty());
    }

    #[test]
    fn load_increment_rejects_null_context() {
        let rc = unsafe { load_increment(ptr::null_mut(), b"x".as_ptr(), 1, ptr::null_mut()) };
        assert_eq!(rc, 0);
    }

    #[test]
    fn fill_vtable_wires_incremental_callbacks() {
        let mut module = GdkPixbufModule {
            module_name: ptr::null_mut(),
            module_path: ptr::null_mut(),
            module: ptr::null_mut(),
            info: ptr::null_mut(),
            load: None,
            load_xpm_data: None,
            begin_load: None,
            stop_load: None,
            load_increment: None,
            load_animation: None,
            save: None,
            save_to_callback: None,
            is_save_option_supported: None,
            _reserved1: None,
            _reserved2: None,
            _reserved3: None,
            _reserved4: None,
        };
        unsafe { fill_vtable(&mut module) };
        assert!(module.load.is_some());
        assert!(module.begin_load.is_some());
        assert!(module.load_increment.is_some());
        assert!(module.stop_load.is_some());
        assert!(module.load_animation.is_some());
        assert!(module.save.is_none());
    }

    #[test]
    fn fill_vtable_is_safe_on_null_module() {
        unsafe { fill_vtable(ptr::null_mut()) };
    }

    extern "C" fn never_called_size(_: *mut c_int, _: *mut c_int, _: *mut c_void) {}
    extern "C" fn never_called_prepared(_: *mut GdkPixbuf, _: *mut c_void, _: *mut c_void) {}
    extern "C" fn never_called_updated(
        _: *mut GdkPixbuf,
        _: c_int,
        _: c_int,
        _: c_int,
        _: c_int,
        _: *mut c_void,
    ) {
    }

    #[test]
    fn average_rate_handles_uniform_frames() {
        use glycin_ng::{Frame, MemoryFormat, Texture};
        use std::time::Duration;
        let tex = Texture::from_parts(
            1,
            1,
            4,
            MemoryFormat::R8g8b8a8,
            vec![0u8; 4].into_boxed_slice(),
        )
        .unwrap();
        let frames = vec![
            Frame::new(tex.clone(), Some(Duration::from_millis(100))),
            Frame::new(tex.clone(), Some(Duration::from_millis(100))),
            Frame::new(tex, Some(Duration::from_millis(100))),
        ];
        assert!((average_rate_fps(&frames) - 10.0).abs() < 0.01);
    }

    #[test]
    fn average_rate_falls_back_when_no_delays() {
        use glycin_ng::{Frame, MemoryFormat, Texture};
        let tex = Texture::from_parts(
            1,
            1,
            4,
            MemoryFormat::R8g8b8a8,
            vec![0u8; 4].into_boxed_slice(),
        )
        .unwrap();
        let frames = vec![Frame::new(tex, None)];
        assert!((average_rate_fps(&frames) - 10.0).abs() < 0.01);
    }

    #[test]
    fn average_rate_clamps_extreme_delays() {
        use glycin_ng::{Frame, MemoryFormat, Texture};
        use std::time::Duration;
        let tex = Texture::from_parts(
            1,
            1,
            4,
            MemoryFormat::R8g8b8a8,
            vec![0u8; 4].into_boxed_slice(),
        )
        .unwrap();
        let too_fast = vec![Frame::new(tex.clone(), Some(Duration::from_micros(100)))];
        let too_slow = vec![Frame::new(tex, Some(Duration::from_secs(3600)))];
        assert!(average_rate_fps(&too_fast) <= 120.0);
        assert!(average_rate_fps(&too_slow) >= 0.1);
    }

    #[test]
    fn callbacks_are_remembered_across_begin_and_stop() {
        let ctx = unsafe {
            begin_load(
                Some(never_called_size),
                Some(never_called_prepared),
                Some(never_called_updated),
                0xdead_beef as *mut c_void,
                ptr::null_mut(),
            )
        };
        let restored = unsafe { Box::from_raw(ctx as *mut LoadContext) };
        assert!(restored.size_func.is_some());
        assert!(restored.prepared_func.is_some());
        assert!(restored.updated_func.is_some());
        assert_eq!(restored.user_data as usize, 0xdead_beef);
    }
}
