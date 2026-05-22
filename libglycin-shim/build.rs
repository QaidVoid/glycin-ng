//! Build the shim against `libglycin_ng.so` dynamically.
//!
//! Set `GLYCIN_NG_LIB_DIR` to override the link search path (distro
//! builds point it at `/usr/lib`). Two rpath entries are emitted so
//! the shim resolves the engine both in the installed layout
//! (`$ORIGIN`) and in the cargo workspace layout where test binaries
//! live one level down from the cdylib (`$ORIGIN/..`).

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=GLYCIN_NG_LIB_DIR");

    println!("cargo:rustc-cdylib-link-arg=-Wl,-soname,libglycin-2.so.0");
    println!("cargo:rustc-link-lib=dylib=glycin_ng");

    let search = if let Ok(dir) = env::var("GLYCIN_NG_LIB_DIR") {
        PathBuf::from(dir)
    } else {
        let manifest = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
        let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".into());
        PathBuf::from(manifest)
            .join("..")
            .join("target")
            .join(profile)
    };
    println!("cargo:rustc-link-search=native={}", search.display());

    println!("cargo:rustc-cdylib-link-arg=-Wl,-rpath,$ORIGIN");
    println!("cargo:rustc-cdylib-link-arg=-Wl,-rpath,$ORIGIN/..");
}
