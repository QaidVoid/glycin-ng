# glycin-ng-pixbuf-loader

A gdk-pixbuf module that decodes images through `glycin-ng`. Drop-in
replacement for gdk-pixbuf's bundled loaders and for downstream
glycin-backed loaders.

## Build

```
cargo build --release -p glycin-ng-pixbuf-loader
```

Output: `target/release/libpixbufloader_glycin_ng.so` (~5 MB,
includes every codec glycin-ng wires by default).

## Install

```
install -Dm755 target/release/libpixbufloader_glycin_ng.so \
    ${libdir}/gdk-pixbuf-2.0/2.10.0/loaders/libpixbufloader_glycin_ng.so
gdk-pixbuf-query-loaders --update-cache
```

`${libdir}` is typically `/usr/lib`, `/usr/lib64`, or
`/usr/lib/x86_64-linux-gnu`; check `pkg-config --variable=libdir
gdk-pixbuf-2.0` if unsure.

To register the loader into a non-system gdk-pixbuf installation
(packaging into an AppDir, Flatpak, container image, ...):

```
GDK_PIXBUF_MODULEDIR=/path/to/loaders \
    gdk-pixbuf-query-loaders > /path/to/loaders.cache
```

Then point the consuming process at the same prefix with
`GDK_PIXBUF_MODULE_FILE=/path/to/loaders.cache`.

## Supported MIME types

PNG (incl. APNG), JPEG, GIF (incl. animated), WebP (incl. animated),
TIFF, BMP, ICO/CUR, TGA, QOI, OpenEXR, PNM family, DDS, JPEG XL.

## Sandbox

Every decode goes through `glycin_ng::Loader::new_bytes(...).load()`
which spawns a dedicated worker thread, applies landlock + seccomp
in the worker, joins, and returns the result. The host process (and
gdk-pixbuf's calling thread) stays unrestricted.

## Limitations

- Incremental loading is wired (`begin_load` / `load_increment` /
  `stop_load`), but the worker accumulates the full buffer before
  decoding, so the `prepared` and `updated` callbacks fire only
  once at end-of-stream. Per-row progressive previews would require
  a streaming decoder behind every format.
- `load_animation` is wired through `GdkPixbufSimpleAnim`, which
  holds a single frame rate. Animations with non-uniform per-frame
  delays are flattened to the average delay across all frames.
  Per-frame timing requires a custom `GdkPixbufAnimation` subclass
  and is not yet implemented.
- Output is always RGBA8. Higher bit depths and float formats from
  EXR/JXL are downsampled. Adding a passthrough for 16-bit and float
  is straightforward once a caller needs it.
- 16-bit half-float channels (EXR, partial JXL paths) are currently
  zeroed; native f16 conversion is on the to-do list.

## License

MIT OR Apache-2.0, matching the parent crate. Permissive-only
dependency policy applies here too.
