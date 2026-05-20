# glycin-ng-libglycin-shim

A `libglycin-2.so.0` ABI-compatible shared library that re-exports
the `gly_*` C surface from upstream glycin and routes the load path
through `glycin-ng`. The encode path is stubbed and returns `NULL` /
`FALSE`.

The point: distributions that compile a glycin-backed loader
directly into `libgdk_pixbuf-2.0.so.0` (Arch is the visible example)
list `libglycin-2.so.0` as a `NEEDED` dynamic dependency. Dropping
this shim in place lets those builds resolve the link, while every
decode actually happens inside `glycin-ng` (in-process, landlock +
seccomp sandbox, permissive-only deps).

## Build

```
cargo build --release -p glycin-ng-libglycin-shim
```

Output: `target/release/libglycin_2.so`. The build script pins the
SONAME to `libglycin-2.so.0` regardless of the file name.

## Install (drop-in replacement)

```
sudo install -Dm755 target/release/libglycin_2.so \
    ${libdir}/libglycin-2.so.0
sudo ln -sf libglycin-2.so.0 ${libdir}/libglycin-2.so
```

After install, any consumer that resolves `libglycin-2.so.0` (such
as Arch's patched `libgdk_pixbuf-2.0.so.0`) picks up the shim. The
matching `glycin-loaders/` directory and `bwrap` binary are not
required and can be removed.

## What works

Load path:
- `gly_loader_new` (path-backed `GFile`)
- `gly_loader_new_for_bytes`
- `gly_loader_new_for_stream` (whole stream read into memory)
- `gly_loader_set_sandbox_selector` (maps the four upstream values
  onto `glycin_ng`'s `SandboxSelector::default()` /
  `SandboxSelector::none()`; bwrap / flatpak-spawn selections fall
  back to the in-process layered sandbox)
- `gly_loader_set_apply_transformations`
- `gly_loader_set_accepted_memory_formats` (accepted but currently
  ignored - decoders return their native format)
- `gly_loader_load`
- `gly_image_get_width` / `gly_image_get_height` /
  `gly_image_get_transformation_orientation`
- `gly_image_get_specific_frame` (advances a cursor each call)
- `gly_frame_request_new` /
  `gly_frame_request_set_loop_animation` /
  `gly_frame_request_set_scale` (stored; scaling is not applied yet)
- `gly_frame_get_width` / `_height` / `_stride` /
  `_memory_format` / `_buf_bytes` / `_delay`
- `gly_memory_format_has_alpha` / `_is_premultiplied`
- `gly_loader_error_quark`

## What stubs out

Encode path - `glycin_ng` has no encoder yet, so all of these
return `NULL` or `FALSE`. Callers can detect that and fall back to
their own encoder.

- `gly_creator_new`
- `gly_creator_add_frame` / `_with_stride`
- `gly_creator_add_metadata_key_value`
- `gly_creator_set_encoding_quality` / `_encoding_compression` /
  `_sandbox_selector`
- `gly_creator_create`
- `gly_new_frame_set_color_icc_profile`
- `gly_encoded_image_get_data`

Metadata key-value access (`gly_image_get_metadata_keys` /
`gly_image_get_metadata_key_value`) also returns `NULL`; the
underlying EXIF/ICC blobs are available on the `Image` but are not
yet projected into the gly_* key/value surface.

## Object lifetimes

`GlyLoader`, `GlyImage`, `GlyFrame`, and `GlyFrameRequest` are
plain base-class `GObject`s with our Rust state attached via
`g_object_set_data_full`. The attached destroy notify frees the
state when the host calls `g_object_unref` and the refcount hits
zero. This bypasses full `GType` registration and is enough for
every caller that uses these handles strictly through the `gly_*`
C surface; consumers that introspect via `G_TYPE_CHECK_INSTANCE_*`
will not see a `GLY_TYPE_LOADER` derivation.

## License

MIT OR Apache-2.0, matching the parent crate.
