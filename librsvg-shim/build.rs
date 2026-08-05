//! Build the shim against `libglycin_ng.so` dynamically.
//!
//! Mirrors `libglycin-shim/build.rs`: `GLYCIN_NG_LIB_DIR` overrides
//! the link search path, and two rpath entries cover the installed
//! layout (`$ORIGIN`) and the cargo workspace layout where test
//! binaries live one level down from the cdylib (`$ORIGIN/..`).
//! Additionally a linker version script restricts the export table
//! to the `rsvg_*` ABI.

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=librsvg_shim.ld");
    println!("cargo:rerun-if-env-changed=GLYCIN_NG_LIB_DIR");

    let manifest = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");

    println!("cargo:rustc-cdylib-link-arg=-Wl,-soname,librsvg-2.so.2");
    println!(
        "cargo:rustc-cdylib-link-arg=-Wl,--version-script={}/librsvg_shim.ld",
        manifest
    );
    println!("cargo:rustc-link-lib=dylib=glycin_ng");

    // Real librsvg carries these NEEDED entries itself; consumers
    // (e.g. the gdk-pixbuf SVG loader module) rely on the library
    // bringing its own dependencies rather than the host process
    // having them loaded.
    for lib in [
        "cairo",
        "gobject-2.0",
        "gio-2.0",
        "glib-2.0",
        "gdk_pixbuf-2.0",
    ] {
        println!("cargo:rustc-link-lib=dylib={lib}");
    }

    let search = if let Ok(dir) = env::var("GLYCIN_NG_LIB_DIR") {
        PathBuf::from(dir)
    } else {
        let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".into());
        PathBuf::from(&manifest)
            .join("..")
            .join("target")
            .join(profile)
    };
    println!("cargo:rustc-link-search=native={}", search.display());

    println!("cargo:rustc-cdylib-link-arg=-Wl,-rpath,$ORIGIN");
    println!("cargo:rustc-cdylib-link-arg=-Wl,-rpath,$ORIGIN/..");
}
