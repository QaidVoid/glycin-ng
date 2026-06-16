# glycin-ng-libglycin-shim

ABI-compatible `libglycin-2.so.0` that forwards every `gly_*` call to
[`glycin-ng`](../). Drop-in replacement for upstream `libglycin`.

- **~9x smaller install.** ~4 MiB vs ~37 MiB on Arch.
- **No bubblewrap. No D-Bus. No helper binaries.**
- **Permissive licensing only.** No LGPL or MPL transitive code.

## When you want this

Newer `gdk-pixbuf` (2.42.10+) ships with optional glycin support
compiled directly into `libgdk_pixbuf-2.0.so.0`. Arch's `gdk-pixbuf2`
package turns it on, so its `libgdk_pixbuf-2.0.so.0` carries a hard
`NEEDED libglycin-2.so.0`. Without the shim you either:

- Keep upstream `libglycin` and pull in `bwrap` plus four per-format
  loader binaries (`glycin-image-rs`, `glycin-svg`, `glycin-jxl`,
  `glycin-heif`) plus the LGPL/MPL libraries those link against
  (`librsvg`, `libjxl`, `libheif`, `libopenraw`, ...), or
- Don't ship gdk-pixbuf at all and find another image stack.

With this shim, `libgdk_pixbuf-2.0.so.0` links cleanly, every decode
happens in-process inside `glycin_ng`'s sandbox, and the helpers can
be removed.

## Size comparison

The shim is a thin forwarder. The decoders live in `libglycin_ng.so`,
which the shim dynamically links to via `NEEDED libglycin_ng.so`. So
a complete install is two files. Numbers below are from `pacman -Qi`
on Arch x86_64 against locally-built release artifacts:

| Component               | Upstream libglycin                                                                   | This shim                |
|-------------------------|--------------------------------------------------------------------------------------|--------------------------|
| `libglycin-2.so.0`      | ~3.2 MiB                                                                             | ~290 KiB (thin forwarder)|
| Decode engine           | (in separate loader binaries)                                                        | `libglycin_ng.so` ~3.7 MiB |
| Sandbox helper          | `bwrap` (~97 KiB)                                                                    | (in-process)             |
| Per-format loaders      | `glycin-image-rs`, `glycin-svg`, `glycin-jxl`, `glycin-heif` (shipped via `glycin-loaders`) | (in-process)       |
| Transitive libraries    | `librsvg` ~10.3 MiB, `libjxl` ~9.7 MiB, `libheif`, `libopenraw`, `libdav1d`, ...     | none added               |
| **Install weight**      | **~37 MiB** (measured: `glycin` + `librsvg` + `libjxl` + `bubblewrap`)               | **~4 MiB**               |

## Build

```
cargo build --release -p glycin-ng-c
cargo build --release -p glycin-ng-libglycin-shim
```

The first command produces `target/release/libglycin_ng.so` (the
decode engine, ~3.7 MiB). The second produces
`target/release/libglycin_2.so` (~290 KiB), which `build.rs` pins to
`SONAME libglycin-2.so.0` and links dynamically against
`libglycin_ng.so`. The shim resolves any caller's
`NEEDED libglycin-2.so.0` cleanly.

## Install (drop-in)

```
LIBDIR=$(pkg-config --variable=libdir gdk-pixbuf-2.0)
sudo install -Dm755 target/release/libglycin_ng.so \
    "$LIBDIR/libglycin_ng.so"
sudo install -Dm755 target/release/libglycin_2.so \
    "$LIBDIR/libglycin-2.so.0"
sudo ln -sf libglycin-2.so.0 "$LIBDIR/libglycin-2.so"
```

Both shared objects must land in a directory the dynamic linker
searches, since the shim resolves `libglycin_ng.so` at load time.

Verify the swap took:

```
ldd "$(pkg-config --variable=libdir gdk-pixbuf-2.0)/libgdk_pixbuf-2.0.so.0" \
    | grep libglycin
```

The output should point at the file you just installed. The original
`${libdir}/glycin-loaders/` directory and the `bwrap` binary are
unused once the shim is in place and can be removed.

## Symbol coverage

### Load path

| Symbol                                                                 | Behavior                                                          |
|------------------------------------------------------------------------|-------------------------------------------------------------------|
| `gly_loader_new`                                                       | path-backed `GFile` (non-native files return `NULL`)              |
| `gly_loader_new_for_bytes`                                             | in-memory buffer                                                  |
| `gly_loader_new_for_stream`                                            | whole stream read into memory before decode                       |
| `gly_loader_set_sandbox_selector`                                      | accepted, ignored (see below)                                     |
| `gly_loader_set_apply_transformations`                                 | EXIF orientation toggle                                           |
| `gly_loader_set_accepted_memory_formats`                               | honored; output converted to a format in the set                  |
| `gly_loader_load`                                                      | decode                                                            |
| `gly_image_get_width` / `_height` / `_transformation_orientation`      |                                                                   |
| `gly_image_get_specific_frame`                                         | advances a cursor through animation frames                        |
| `gly_image_next_frame`                                                 | next frame with a default request, looping at the end             |
| `gly_image_get_mime_type`                                             | detected IANA media type of the decoded image                     |
| `gly_frame_request_new` / `_set_loop_animation` / `_set_scale`         | stored; scaling is not yet applied                                |
| `gly_frame_get_width` / `_height` / `_stride` / `_memory_format` / `_buf_bytes` / `_delay` |                                                       |
| `gly_frame_get_color_cicp`                                            | returns `NULL`; engine does not surface CICP yet                  |
| `gly_memory_format_has_alpha` / `_is_premultiplied`                    |                                                                   |
| `gly_loader_get_mime_types`                                          | full set of decodable IANA media types as a `GStrv`               |
| `gly_loader_error_quark`                                               |                                                                   |

### Asynchronous variants

`gly_loader_load_async` / `_finish`, `gly_image_next_frame_async` /
`_finish`, `gly_image_get_specific_frame_async` / `_finish`,
`gly_creator_create_async` / `_finish`, and
`gly_loader_get_mime_types_async` / `_finish` wrap their synchronous
counterparts in a `GTask` and run on a GLib thread-pool thread, so the
caller's main loop is never blocked. The `_finish` functions propagate
the result or the error.

### Type registration

`gly_memory_format_get_type`, `gly_sandbox_selector_get_type`, and
`gly_loader_error_get_type` register GLib enums;
`gly_memory_format_selection_get_type` registers a flags type;
`gly_cicp_get_type` registers a boxed type with
`gly_cicp_copy` / `gly_cicp_free`; and `gly_loader_get_type`,
`gly_image_get_type`, `gly_frame_get_type`,
`gly_frame_request_get_type`, `gly_creator_get_type`,
`gly_encoded_image_get_type`, and `gly_new_frame_get_type` register
`GObject` subtypes sized from the parent. The enum and flags value
names and nicks reproduce what upstream's `#[derive(glib::Enum)]`
registers (the UpperCamelCase variant name and its kebab-case nick,
for example `NoMoreFrames` and `no-more-frames`), so
`g_enum_get_value_by_name` and `_by_nick` resolve the same strings as
upstream.

### Encode path

`gly_creator_new` selects an encoder by MIME type. Supported targets:
`image/png`, `image/jpeg`, `image/gif`, `image/webp`, `image/tiff`,
`image/bmp`. `gly_creator_add_frame` / `_add_frame_with_stride`,
`_add_metadata_key_value`, `_set_encoding_quality`,
`_set_encoding_compression`, `_create`, and the
`gly_encoded_image_*` accessors all forward to
`glycin_ng::Encoder`. `gly_creator_set_sandbox_selector` is accepted
and ignored (encode runs in-process under the same per-call sandbox
as decode).

### Metadata

`gly_image_get_metadata_keys` and `gly_image_get_metadata_key_value`
return `NULL`. EXIF and ICC blobs are available on the underlying
`glycin_ng::Image` but not yet projected through the `gly_*`
key/value surface.

## Sandbox model

Every decode goes through `glycin_ng::Loader::new_bytes(...).load()`,
which spawns a `glycin-ng-worker` thread and applies landlock plus
seccomp on it before invoking the decoder. The host process stays
unrestricted.

`gly_loader_set_sandbox_selector` is **intentionally ignored**.
Upstream supports a `GLY_SANDBOX_SELECTOR_NOT_SANDBOXED` value that
turns off the bwrap container; in upstream that's necessary because
bwrap fails to nest inside Flatpak / AppImage. We don't need bwrap to
nest (landlock and seccomp are per-thread `prctl` calls that always
work), so honoring the selector would only let `LD_PRELOAD` shims
(such as AnyLinux's `gtk-class-fix.so`) disable the in-process
sandbox post-load. The setter is kept for ABI compatibility but
performs no state change.

## Object lifetimes

`GlyLoader`, `GlyImage`, `GlyFrame`, and `GlyFrameRequest` are
base-class `GObject`s with Rust state attached via
`g_object_set_data_full`. State is freed by the attached destroy
notify when the host calls `g_object_unref` and the refcount hits
zero. The `gly_*_get_type` functions register the matching subtypes,
so `GLY_TYPE_LOADER` and friends resolve to real, named `GType`s, but
the handles are not instantiated as those subtypes. Consumers that
introspect a handle via `G_TYPE_CHECK_INSTANCE_*` therefore see a base
`GObject` rather than the registered derivation. Every gdk-pixbuf code
path that uses these handles strictly through the `gly_*` C surface is
unaffected.

## License

MIT OR Apache-2.0.
