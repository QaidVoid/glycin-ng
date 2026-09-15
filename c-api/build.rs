//! Linker glue for the cdylib that exposes glycin-ng's C ABI.
//!
//! rustc already narrows the exported symbol set to the `glycin_ng_*` C
//! API (the win that justifies splitting this package out from the
//! rlib), so `--gc-sections` finishes the job by dropping sections only
//! the hidden symbols reach.
//!
//! No SONAME is set yet: the C ABI is unstable pre-1.0 and committing
//! to `libglycin_ng.so.0` now would lie about compatibility. A future
//! release will pin the soname once the surface settles.

fn main() {
    println!("cargo:rustc-cdylib-link-arg=-Wl,--gc-sections");
}
