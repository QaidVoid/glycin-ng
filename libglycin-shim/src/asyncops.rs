//! Asynchronous variants of the blocking load, frame, and encode
//! entry points.
//!
//! Each `_async` function wraps the matching synchronous call in a
//! `GTask` and runs it on a GLib thread-pool thread, so the calling
//! thread's main loop is never blocked. The paired `_finish` function
//! propagates the task result or its error. This mirrors upstream
//! glycin, where the same operations run off the caller's thread.

use std::ffi::c_void;
use std::ptr;

use crate::ffi::{
    self, GAsyncReadyCallback, GAsyncResult, GCancellable, GError, GObject, GTask, gpointer,
};

/// Return a finished task's pointer result, or NULL with `error` set.
unsafe fn finish_pointer(result: *mut GAsyncResult, error: *mut *mut GError) -> *mut GObject {
    unsafe { ffi::g_task_propagate_pointer(result as *mut GTask, error) as *mut GObject }
}

/// Complete a worker thread with either the produced handle or a
/// synthesized error when the synchronous call returned NULL without
/// filling one in.
unsafe fn return_handle(task: *mut GTask, handle: *mut GObject, mut error: *mut GError) {
    if handle.is_null() {
        if error.is_null() {
            unsafe { crate::set_error(&mut error, 0, "async operation failed") };
        }
        unsafe { ffi::g_task_return_error(task, error) };
    } else {
        unsafe {
            ffi::g_task_return_pointer(task, handle as gpointer, Some(ffi::g_object_unref));
        }
    }
}

// ----- gly_loader_load_async / _finish -----

/// # Safety
/// `loader` must be valid. `cancellable` may be NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_loader_load_async(
    loader: *mut GObject,
    cancellable: *mut GCancellable,
    callback: GAsyncReadyCallback,
    user_data: gpointer,
) {
    let task = unsafe { ffi::g_task_new(loader as gpointer, cancellable, callback, user_data) };
    unsafe { ffi::g_task_run_in_thread(task, Some(load_thread)) };
    unsafe { ffi::g_object_unref(task as *mut c_void) };
}

unsafe extern "C" fn load_thread(
    task: *mut GTask,
    source: *mut GObject,
    _task_data: gpointer,
    _cancellable: *mut GCancellable,
) {
    let mut error: *mut GError = ptr::null_mut();
    let image = unsafe { crate::gly_loader_load(source, &mut error) };
    unsafe { return_handle(task, image, error) };
}

/// # Safety
/// `result` must be the `GAsyncResult` handed to the callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_loader_load_finish(
    _loader: *mut GObject,
    result: *mut GAsyncResult,
    error: *mut *mut GError,
) -> *mut GObject {
    unsafe { finish_pointer(result, error) }
}

// ----- gly_image_next_frame_async / _finish -----

/// # Safety
/// `image` must be valid. `cancellable` may be NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_image_next_frame_async(
    image: *mut GObject,
    cancellable: *mut GCancellable,
    callback: GAsyncReadyCallback,
    user_data: gpointer,
) {
    let task = unsafe { ffi::g_task_new(image as gpointer, cancellable, callback, user_data) };
    unsafe { ffi::g_task_run_in_thread(task, Some(next_frame_thread)) };
    unsafe { ffi::g_object_unref(task as *mut c_void) };
}

unsafe extern "C" fn next_frame_thread(
    task: *mut GTask,
    source: *mut GObject,
    _task_data: gpointer,
    _cancellable: *mut GCancellable,
) {
    let mut error: *mut GError = ptr::null_mut();
    let frame = unsafe { crate::gly_image_next_frame(source, &mut error) };
    unsafe { return_handle(task, frame, error) };
}

/// # Safety
/// `result` must be the `GAsyncResult` handed to the callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_image_next_frame_finish(
    _image: *mut GObject,
    result: *mut GAsyncResult,
    error: *mut *mut GError,
) -> *mut GObject {
    unsafe { finish_pointer(result, error) }
}

// ----- gly_image_get_specific_frame_async / _finish -----

/// # Safety
/// `image` must be valid. `frame_request` and `cancellable` may be NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_image_get_specific_frame_async(
    image: *mut GObject,
    frame_request: *mut GObject,
    cancellable: *mut GCancellable,
    callback: GAsyncReadyCallback,
    user_data: gpointer,
) {
    let task = unsafe { ffi::g_task_new(image as gpointer, cancellable, callback, user_data) };
    if !frame_request.is_null() {
        unsafe {
            ffi::g_object_ref(frame_request as *mut c_void);
            ffi::g_task_set_task_data(task, frame_request as gpointer, Some(ffi::g_object_unref));
        }
    }
    unsafe { ffi::g_task_run_in_thread(task, Some(specific_frame_thread)) };
    unsafe { ffi::g_object_unref(task as *mut c_void) };
}

unsafe extern "C" fn specific_frame_thread(
    task: *mut GTask,
    source: *mut GObject,
    task_data: gpointer,
    _cancellable: *mut GCancellable,
) {
    let frame_request = task_data as *mut GObject;
    let mut error: *mut GError = ptr::null_mut();
    let frame = unsafe { crate::gly_image_get_specific_frame(source, frame_request, &mut error) };
    unsafe { return_handle(task, frame, error) };
}

/// # Safety
/// `result` must be the `GAsyncResult` handed to the callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_image_get_specific_frame_finish(
    _image: *mut GObject,
    result: *mut GAsyncResult,
    error: *mut *mut GError,
) -> *mut GObject {
    unsafe { finish_pointer(result, error) }
}

// ----- gly_creator_create_async / _finish -----

/// # Safety
/// `creator` must be valid. `cancellable` may be NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_creator_create_async(
    creator: *mut GObject,
    cancellable: *mut GCancellable,
    callback: GAsyncReadyCallback,
    user_data: gpointer,
) {
    let task = unsafe { ffi::g_task_new(creator as gpointer, cancellable, callback, user_data) };
    unsafe { ffi::g_task_run_in_thread(task, Some(create_thread)) };
    unsafe { ffi::g_object_unref(task as *mut c_void) };
}

unsafe extern "C" fn create_thread(
    task: *mut GTask,
    source: *mut GObject,
    _task_data: gpointer,
    _cancellable: *mut GCancellable,
) {
    let mut error: *mut GError = ptr::null_mut();
    let encoded = unsafe { crate::gly_creator_create(source, &mut error) };
    unsafe { return_handle(task, encoded, error) };
}

/// # Safety
/// `result` must be the `GAsyncResult` handed to the callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gly_creator_create_finish(
    _creator: *mut GObject,
    result: *mut GAsyncResult,
    error: *mut *mut GError,
) -> *mut GObject {
    unsafe { finish_pointer(result, error) }
}
