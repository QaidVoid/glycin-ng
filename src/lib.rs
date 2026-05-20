//! Permissively-licensed Rust image decoder library with in-process
//! sandboxing.
//!
//! `glycin-ng` decodes images in-process via pure-Rust decoder crates
//! (`png`, `jpeg-decoder`, `gif`, `image-webp`, ...) and applies a
//! layered Linux sandbox (landlock, seccomp, setrlimit) to the
//! decoding thread. The actually-applied sandbox posture is reported
//! back so callers can audit or refuse a degraded posture.
//!
//! # Public API style
//!
//! The public API is strictly synchronous. Decoding is short-lived
//! and CPU-bound; callers that need async should wrap calls in
//! `spawn_blocking` or equivalent.
//!
//! # Feature flags
//!
//! Capability groups (independent, all default on except `encode`):
//! `decode`, `encode`, `metadata`.
//!
//! Sandbox layers (default on, Linux only): `landlock`, `seccomp`.
//!
//! Per-format gates (default on): `png`, `jpeg`, `gif`, `webp`,
//! `tiff`, `bmp`, `ico`, `tga`, `qoi`, `exr`, `pnm`, `dds`, `jxl`.
//!
//! C ABI surface (off by default): `c-api`. Enables the `cdylib`
//! and the C bindings.

#![deny(missing_docs)]
#![forbid(unsafe_op_in_unsafe_fn)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

mod error;
mod formats;
mod image;
mod limits;
mod loader;
mod memory;
pub mod sandbox;
pub mod sniff;

pub use crate::error::{Error, Result};
pub use crate::image::{Frame, Image, Orientation, Texture};
pub use crate::limits::Limits;
pub use crate::loader::Loader;
pub use crate::memory::MemoryFormat;
pub use crate::sandbox::{
    LandlockPosture, RlimitPosture, SandboxPosture, SandboxSelector, SeccompPosture,
};
pub use crate::sniff::KnownFormat;
