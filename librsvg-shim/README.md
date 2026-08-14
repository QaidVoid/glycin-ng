# glycin-ng-librsvg-shim

ABI-compatible `librsvg-2.so.2` that forwards every `rsvg_*` call to
[`glycin-ng`](../)'s resvg-based SVG engine. Drop-in replacement for
GNOME librsvg.

- **~2.3 MiB of SVG stack instead of ~8.5 MiB** (see below).
- **No pango, no harfbuzz, no libxml2, no fribidi, no graphite2.**
  Text is shaped in-process by the engine's pure-Rust font stack.
- **Permissively-licensed decoder.** No LGPL code of our own and no
  LGPL Rust dependencies, unlike librsvg itself. The GObject stack it
  links against (glib, cairo) stays LGPL, as it is for
  any consumer of this ABI.
- **Sandboxed parsing and rendering.** Every parse and every render
  runs on a glycin-ng worker thread under seccomp. Landlock is
  applied too, except when parsing a document that both has a
  filesystem base URI and actually references external files, which
  needs file access by definition. Everything else, including every
  icon and thumbnail, and every render without exception, gets both
  layers.

## ABI coverage

The shim exports upstream's `win32/librsvg.symbols` table: 44
`rsvg_*` functions plus `rsvg_major_version` / `rsvg_minor_version` /
`rsvg_micro_version`. The `win32/librsvg-pixbuf.symbols` entry points
are deliberately absent; see below. `RsvgHandle` is registered as a
real GObject subtype with the ABI-mandated instance/class sizes
(`_abi_padding` in `rsvg.h`) and all 11 properties, including the
construct-only `flags`, so `g_object_new (RSVG_TYPE_HANDLE, ...)`,
`g_object_get`, and `RSVG_IS_HANDLE` behave like upstream.

### No pixbuf API

`rsvg_handle_get_pixbuf*` and `rsvg_pixbuf_from_file*` are not
provided. This is upstream's own `LIBRSVG_HAVE_PIXBUF = FALSE`
configuration, not an omission: `rsvg.h` guards `rsvg-pixbuf.h`
behind that macro, so packages ship a `rsvg-features.h` with it set
to `FALSE` and consumers never see the declarations. `tests/run.sh`
compiles against exactly that header configuration.

The consequence is that the shim links no gdk-pixbuf, and neither do
its consumers on its account. On Arch that alone removes
`gdk-pixbuf2`, `shared-mime-info`, `libxml2` and `icu`, 55 MiB.

The cost is the gdk-pixbuf SVG loader module, which calls
`rsvg_handle_get_pixbuf_and_error`. That module no longer exists on
distros whose gdk-pixbuf talks to glycin directly (2.43+, e.g. current
Arch, where librsvg ships no loader at all). On distros that still use
the loader-module system, SVG decoding through gdk-pixbuf needs
`libglycin-shim` instead, which routes it into the same engine.

## Size and dependency comparison

Measured on x86_64 (Gentoo, librsvg 2.62.2, release build of this
crate). The shim itself is small because the work lives in
`libglycin_ng.so`, so the honest comparison counts both, plus the
libraries each side drags in that the other does not:

| | upstream librsvg | this shim |
|---|---|---|
| the library | 4.90 MiB | 0.28 MiB |
| SVG engine behind it | (built in) | 2.01 MiB (`libglycin_ng.so`, SVG-only build) |
| libraries only this side needs | 3.61 MiB: pango, pangocairo, pangoft2, harfbuzz, graphite2, fribidi, libxml2, cairo-gobject | none |
| **total for SVG** | **~8.5 MiB** | **~2.3 MiB** |

Shared by both, so not counted either way: glib, gobject, gio, cairo,
and (via cairo) freetype and fontconfig. These appear in the
signatures of the ABI itself, so a caller handing us a `cairo_t *` or
freeing our `GError` has them loaded by definition.

A full `libglycin_ng.so` with every format enabled is 4.37 MiB and
replaces far more than librsvg, so if the engine is already installed
for other formats, the incremental cost of SVG support is just the
0.28 MiB shim.

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
real-ABI harness compiled against the librsvg headers as this repo
ships them (`LIBRSVG_HAVE_PIXBUF FALSE`) with real GLib and cairo:
GType introspection, property round-trips, pixel-exact renders,
SVGZ, and misuse guards. It needs a C compiler, pkg-config, and
development headers for gobject-2.0, gio-2.0, cairo and librsvg, so
it is a manual pre-release check rather than part of `cargo test`.

## Install (drop-in)

```sh
install -Dm755 target/release/libglycin_ng.so /usr/lib/libglycin_ng.so
install -Dm755 target/release/librsvg_2.so /usr/lib/librsvg-2.so.2
ln -sf librsvg-2.so.2 /usr/lib/librsvg-2.so
```

Headers are upstream's own: ship `rsvg.h` and `rsvg-cairo.h`
verbatim into `/usr/include/librsvg-2.0/librsvg/`, and
`pkgconfig/librsvg-2.0.pc.in` from this repo as `librsvg-2.0.pc`.
Generate `rsvg-features.h` from upstream's `.in` with
`LIBRSVG_HAVE_PIXBUF` set to `FALSE`, and do not ship
`rsvg-pixbuf.h`. See `pkgbuilds/glycin-ng-librsvg/PKGBUILD`.

Depending on the distro, a full replacement package may also need
pieces the librsvg package traditionally carried, all of which keep
working against the shim:

- on distros whose gdk-pixbuf still uses the loader-module system
  (Debian, Ubuntu, Gentoo, ...), `libpixbufloader-svg.so` and
  `librsvg.thumbnailer` will not work against this shim, because the
  loader calls `rsvg_handle_get_pixbuf_and_error`. Install
  `libglycin-shim` there so gdk-pixbuf reaches the same engine
  directly. Glycin-enabled gdk-pixbuf (2.43+, e.g. current Arch) has
  dropped loaders entirely and Arch's librsvg ships neither file, so
  nothing is lost there. Either way this shim serves the direct
  librsvg linkers: GTK4, ffmpeg, GIMP, Emacs, and friends.
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
  ...) map to Cantarell for deterministic output. The database is
  built once per process and only for documents that draw text, so
  icons never pay for it; text inside an XInclude payload is not
  detected and falls back to the bundled font.
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
