// Pin the SONAME so dynamic linkers that look up `libglycin-2.so.0`
// (Arch's `libgdk_pixbuf-2.0.so.0` has a NEEDED entry with that
// exact name) resolve to this shim. The build output is
// `libglycin_2.so`; install scripts rename or symlink it to
// `libglycin-2.so.0`.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rustc-cdylib-link-arg=-Wl,-soname,libglycin-2.so.0");
}
