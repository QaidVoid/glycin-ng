# glycin-ng-librsvg-shim

ABI-compatible `librsvg-2.so.2` that forwards every `rsvg_*` call to
[`glycin-ng`](../)'s resvg-based SVG engine. Drop-in replacement for
GNOME librsvg.

- **~300 KiB translation layer** instead of a 4.6 MiB library.
- **No pango, no harfbuzz, no fontconfig, no freetype, no libxml2.**
  Text is shaped in-process by the engine's pure-Rust font stack.
- **Permissive licensing only.** No LGPL transitive code.
- **Sandboxed parsing and rendering.** Every parse and every render
  runs on a glycin-ng worker thread under seccomp. Landlock is
  applied too, except when parsing a document that both has a
  filesystem base URI and actually references external files, which
  needs file access by definition. Everything else, including every
  icon and thumbnail, and every render without exception, gets both
  layers.

## ABI coverage

The shim exports the exact public librsvg export table: 52 `rsvg_*`
functions plus `rsvg_major_version` / `rsvg_minor_version` /
`rsvg_micro_version` (the union of upstream's `win32/librsvg.symbols`
and `win32/librsvg-pixbuf.symbols`). `RsvgHandle` is registered as a
real GObject subtype with the ABI-mandated instance/class sizes
(`_abi_padding` in `rsvg.h`) and all 11 properties, including the
construct-only `flags`, so `g_object_new (RSVG_TYPE_HANDLE, ...)`,
`g_object_get`, and `RSVG_IS_HANDLE` behave like upstream. The
unmodified gdk-pixbuf SVG loader module loads and renders through the
shim without recompilation.

## NEEDED comparison

| | upstream librsvg | this shim |
|---|---|---|
| glib / gobject / gio | yes | yes |
| cairo | yes (+ cairo-gobject) | yes |
| gdk-pixbuf | yes | yes |
| libxml2 | yes | no |
| pango + pangocairo | yes | no |
| (transitive) harfbuzz, fontconfig, freetype, fribidi | yes | no |
| decode engine | (built in, 4.6 MiB) | `libglycin_ng.so` |

## Build

```sh
cargo build --release -p glycin-ng-c
cargo build --release -p glycin-ng-librsvg-shim
```

The first command produces `target/release/libglycin_ng.so` (the
engine). The second produces `target/release/librsvg_2.so`, which
`build.rs` pins to `SONAME librsvg-2.so.2` and links dynamically
against the engine. Set `GLYCIN_NG_LIB_DIR` to point the link step at
an installed engine instead of the workspace build.

`tests/run.sh` builds everything and runs `tests/smoke.c`, a
real-ABI harness compiled against the system librsvg headers with
real GLib/cairo/gdk-pixbuf: GType introspection, property
round-trips, pixel-exact renders, SVGZ, and misuse guards. It needs
system C headers, so it is a manual pre-release check rather than
part of `cargo test`.

## Install (drop-in)

```sh
install -Dm755 target/release/libglycin_ng.so /usr/lib/libglycin_ng.so
install -Dm755 target/release/librsvg_2.so /usr/lib/librsvg-2.so.2
ln -sf librsvg-2.so.2 /usr/lib/librsvg-2.so
```

Headers are upstream's own: ship `rsvg.h`, `rsvg-cairo.h`, and
`rsvg-pixbuf.h` verbatim into `/usr/include/librsvg-2.0/librsvg/`,
and `pkgconfig/librsvg-2.0.pc.in` from this repo as
`librsvg-2.0.pc`.

Depending on the distro, a full replacement package may also need
pieces the librsvg package traditionally carried, all of which keep
working against the shim:

- the gdk-pixbuf loader module (`libpixbufloader-svg.so`) and
  `librsvg.thumbnailer`, but only on distros whose gdk-pixbuf still
  uses the loader-module system (Debian, Ubuntu, Gentoo, ...).
  Glycin-enabled gdk-pixbuf (2.43+, e.g. current Arch) has dropped
  loaders entirely and Arch's librsvg no longer ships either file;
  there gdk-pixbuf SVG decoding already flows through libglycin
  (and with `libglycin-shim`, straight into this same engine), so
  this shim serves the direct librsvg linkers: GTK4, ffmpeg, GIMP,
  Emacs, and friends.
- `Rsvg-2.0.typelib` if GObject Introspection consumers (Python,
  JavaScript) matter; the registered GTypes are compatible with the
  upstream typelib.

## Behavior differences

Documented, deliberate trade-offs against upstream:

- **Raster into vector surfaces.** Rendering targets a pixmap at the
  device resolution of the cairo context (reading its CTM, so zoomed
  screen output stays sharp). Rendering into PDF/PS/SVG cairo
  surfaces embeds that raster instead of vectors.
- **Flags are accepted but inert.** `RSVG_HANDLE_FLAG_UNLIMITED` and
  `RSVG_HANDLE_FLAG_KEEP_IMAGE_DATA` have no engine equivalent.
- **Render cancellation is a no-op.**
  `rsvg_handle_set_cancellable_for_rendering` stores the cancellable
  but renders run to completion; cancellation during loading works.
- **External references.** `data:` URIs always work. File references
  resolve relative to the base URI's directory; only when a document
  actually contains such references does its parse run without the
  landlock layer (seccomp stays on). librsvg's full URL-scheme
  acceptance policy is not replicated.
- **Fonts.** Text uses the system font database plus a bundled
  Cantarell fallback; the generic families (`sans-serif`, `serif`,
  ...) map to Cantarell for deterministic output.
- **Viewport fitting** uses uniform `xMidYMid meet` scaling, the
  dominant case; exotic `preserveAspectRatio` values on the root
  element are not honored per-value.
- **Single DPI axis.** `dpi-x` and `dpi-y` are stored and reported
  back separately, but the engine carries one DPI value, so unit
  resolution uses `dpi-x` (or `dpi-y` when only that one was set).
  Asymmetric DPI is not supported.
- **Lenient where upstream is strict.** Element ids are accepted with
  or without the leading `#`, and loading into an already-loaded
  handle replaces the document instead of being refused.
- **No `g_critical` diagnostics.** Upstream logs a critical when an
  entry point is misused (NULL argument, wrong handle type, calls out
  of order); the shim returns the documented failure value quietly.
