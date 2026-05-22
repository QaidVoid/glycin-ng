//! Linker glue for the cdylib that exposes glycin-ng's C ABI.
//!
//! - Version script narrows the exported symbol set to
//!   `glycin_ng_*` so the linker can dead-code-eliminate the rest
//!   (the win that justifies splitting this package out from the
//!   rlib).
//! - `--gc-sections` finishes the job by dropping sections only the
//!   hidden symbols reach.
//!
//! No SONAME is set yet: the C ABI is unstable pre-1.0 and committing
//! to `libglycin_ng.so.0` now would lie about compatibility. A future
//! release will pin the soname once the surface settles.

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let script = format!("{manifest}/glycin_ng.ld");
    println!("cargo:rerun-if-changed=glycin_ng.ld");
    println!("cargo:rustc-cdylib-link-arg=-Wl,--version-script={script}");
    println!("cargo:rustc-cdylib-link-arg=-Wl,--gc-sections");
}
