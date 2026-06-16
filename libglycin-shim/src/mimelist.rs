//! Enumeration of the MIME types the loader can decode.
//!
//! `gly_loader_get_mime_types` and its async variant report the
//! format surface advertised by [`crate::mime::SUPPORTED_MIME_TYPES`]
//! as a freshly allocated `GStrv`. The async variant produces the
//! same list on a thread-pool thread to match upstream's signature,
//! even though building the static list never actually blocks.

use std::ffi::{CString, c_char, c_void};
use std::ptr;

use crate::ffi::{
    self, GAsyncReadyCallback, GAsyncResult, GCancellable, GError, GObject, GStrv, GTask, gpointer,
};
use crate::mime::SUPPORTED_MIME_TYPES;

/// Allocate a `g_strfreev`-compatible `GStrv` of the supported MIME
/// types.
fn build_strv() -> GStrv {
    let slots = SUPPORTED_MIME_TYPES.len() + 1;
    let arr =
        unsafe { ffi::g_malloc(slots * size_of::<*mut c_char>()) } as *mut *mut c_char;
    if arr.is_null() {
        return ptr::null_mut();
    }
    for (i, mime) in SUPPORTED_MIME_TYPES.iter().enumerate() {
        let owned = CString::new(*mime).expect("MIME literal has no interior NUL");
        unsafe { *arr.add(i) = ffi::g_strdup(owned.as_ptr()) };
    }
    unsafe { *arr.add(SUPPORTED_MIME_TYPES.len()) = ptr::null_mut() };
    arr
}

/// # Safety
/// Always safe. The returned `GStrv` is owned by the caller and freed
/// with `g_strfreev`.
#[unsafe(no_mangle)]
pub extern "C" fn gly_loader_get_mime_types() -> GStrv {
    build_strv()
}

/// # Safety
/// `cancellable` may be NULL. `callback` receives the result.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_loader_get_mime_types_async(
    cancellable: *mut GCancellable,
    callback: GAsyncReadyCallback,
    user_data: gpointer,
) {
    let task =
        unsafe { ffi::g_task_new(ptr::null_mut(), cancellable, callback, user_data) };
    unsafe { ffi::g_task_run_in_thread(task, Some(mime_types_thread)) };
    unsafe { ffi::g_object_unref(task as *mut c_void) };
}

unsafe extern "C" fn mime_types_thread(
    task: *mut GTask,
    _source: *mut GObject,
    _task_data: gpointer,
    _cancellable: *mut GCancellable,
) {
    let strv = build_strv();
    unsafe { ffi::g_task_return_pointer(task, strv as gpointer, Some(strv_free)) };
}

unsafe extern "C" fn strv_free(data: gpointer) {
    unsafe { ffi::g_strfreev(data as GStrv) };
}

/// # Safety
/// `result` must be the `GAsyncResult` handed to the callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_loader_get_mime_types_finish(
    result: *mut GAsyncResult,
    error: *mut *mut GError,
) -> GStrv {
    unsafe { ffi::g_task_propagate_pointer(result as *mut GTask, error) as GStrv }
}
