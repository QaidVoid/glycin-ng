//! ABI-compatible reimplementation of `librsvg-2.so.2` on top of
//! `libglycin_ng.so`.
//!
//! Exports the public librsvg C ABI without pixbuf support: the 44
//! `rsvg_*` functions of upstream's `win32/librsvg.symbols` plus the
//! three version variables. SVG parsing and rasterization are
//! delegated to glycin-ng's `glycin_ng_svg_*` engine API; this crate
//! is purely a GObject / GIO / cairo translation layer, resolved
//! against the host process like `libglycin-shim`.
//!
//! Documented differences from upstream librsvg:
//! - The `rsvg_*pixbuf*` entry points of
//!   `win32/librsvg-pixbuf.symbols` are not provided, which is
//!   upstream's own `LIBRSVG_HAVE_PIXBUF = FALSE` configuration.
//!   Packages must ship `rsvg-features.h` accordingly so consumers
//!   never see the declarations.
//! - Rendering into vector cairo surfaces (PDF/PS/SVG) embeds a
//!   raster at device resolution instead of vectors.
//! - `RSVG_HANDLE_FLAG_UNLIMITED` and `KEEP_IMAGE_DATA` are accepted
//!   and ignored.
//! - `rsvg_handle_set_cancellable_for_rendering` is accepted, but
//!   renders run to completion; loading cancellation works.
//! - External references resolve relative to the base URI's
//!   directory; librsvg's full URL-scheme policy is not replicated.

#![allow(clippy::missing_safety_doc)]

mod cairo_ffi;
mod ffi;
mod gobject;
mod handle;
mod ngapi;
mod render;
mod state;

pub use gobject::{
    RsvgHandle, rsvg_error_get_type, rsvg_error_quark, rsvg_handle_flags_get_type,
    rsvg_handle_get_type,
};
pub use handle::*;
pub use render::*;
pub use state::{RsvgDimensionData, RsvgLength, RsvgPositionData, RsvgRectangle};

/// Major component of the librsvg ABI version this shim provides.
#[unsafe(no_mangle)]
#[allow(non_upper_case_globals)]
pub static rsvg_major_version: std::ffi::c_uint = 2;

/// Minor component of the librsvg ABI version this shim provides.
#[unsafe(no_mangle)]
#[allow(non_upper_case_globals)]
pub static rsvg_minor_version: std::ffi::c_uint = 62;

/// Micro component of the librsvg ABI version this shim provides.
#[unsafe(no_mangle)]
#[allow(non_upper_case_globals)]
pub static rsvg_micro_version: std::ffi::c_uint = 0;

/// Deprecated no-op, matching upstream.
#[unsafe(no_mangle)]
pub extern "C" fn rsvg_init() {}

/// Deprecated no-op, matching upstream.
#[unsafe(no_mangle)]
pub extern "C" fn rsvg_term() {}

/// Deprecated no-op, matching upstream.
#[unsafe(no_mangle)]
pub extern "C" fn rsvg_cleanup() {}

/// Deprecated global-DPI setter; a no-op like upstream's, which only
/// ever worked before any handle existed.
#[unsafe(no_mangle)]
pub extern "C" fn rsvg_set_default_dpi(_dpi: f64) {}

/// Deprecated global-DPI setter; a no-op like upstream's.
#[unsafe(no_mangle)]
pub extern "C" fn rsvg_set_default_dpi_x_y(_dpi_x: f64, _dpi_y: f64) {}
