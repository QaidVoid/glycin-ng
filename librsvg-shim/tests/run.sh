#!/bin/sh
# Real-ABI integration test for the librsvg shim.
#
# Builds the engine and the shim in release mode, compiles smoke.c
# against the system librsvg headers with real GLib/GIO/cairo/
# gdk-pixbuf, and runs it against the freshly built librsvg-2.so.2.
# Requires: a C compiler, pkg-config, the development headers for
# gobject-2.0, gio-2.0, cairo and gdk-pixbuf-2.0, and the librsvg
# headers (only the headers; the system librsvg library is not used).
#
# Not wired into `cargo test`: it needs system C libraries and
# headers that CI containers may not carry. Run it manually before
# release.
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/librsvg-shim-smoke"
mkdir -p "$work/libs"

cargo build --manifest-path "$root/Cargo.toml" --release -p glycin-ng-c
cargo build --manifest-path "$root/Cargo.toml" --release -p glycin-ng-librsvg-shim

ln -sf "$root/target/release/librsvg_2.so" "$work/libs/librsvg-2.so.2"
ln -sf "$root/target/release/libglycin_ng.so" "$work/libs/libglycin_ng.so"

rsvg_inc=$(pkg-config --variable=includedir librsvg-2.0 2>/dev/null || echo /usr/include)/librsvg-2.0
${CC:-cc} "$root/librsvg-shim/tests/smoke.c" -o "$work/smoke" \
    -I "${rsvg_inc%/librsvg-2.0}/librsvg-2.0" \
    $(pkg-config --cflags --libs gobject-2.0 gio-2.0 cairo gdk-pixbuf-2.0) \
    -L "$work/libs" -l:librsvg-2.so.2 \
    -DRSVG_DISABLE_DEPRECATION_WARNINGS

LD_LIBRARY_PATH="$work/libs" "$work/smoke" "$work"
