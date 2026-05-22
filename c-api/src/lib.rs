//! `libglycin_ng.so` — cdylib that exports glycin-ng's C ABI.
//!
//! This crate exists so the cdylib build can drop the `rlib` output
//! that `glycin-ng` itself produces for Rust consumers. With only a
//! cdylib target here, rustc treats every Rust symbol not listed in
//! the linker version script as local and lets LTO drop the code
//! reachable only through it. The C ABI surface stays exported.

// Pull every `#[no_mangle]` function from `glycin_ng::c_api` into
// the cdylib output. The `use` keeps rustc from dead-coding them
// out before the linker sees them; the linker then promotes them
// to global symbols per `glycin_ng.ld`.
#[allow(unused_imports)]
use glycin_ng::c_api::*;
