
## 0.1.0 — 2026-05-21

### Bug Fixes

- **svg:** Expand xi:include data uris before parsing
- **libglycin-shim:** Honor accepted memory formats from caller
- **sandbox:** Gate fadvise64 behind x86_64 cfg
- **sandbox:** Silence worker thread panics on stderr
- **sandbox:** Allow clone3 and align seccomp allowlist with upstream
- Correct webp fuzz target enum spelling

### Documentation

- Drop stale per-decode microsecond table
- Rewrite crate readmes with stronger framing and tables
- Mark svg as supported in README and crate-level docs

### Features

- **svg:** Render at caller-requested scale instead of intrinsic
- **sandbox:** Negotiate landlock up to v6 (fs + net + scope)
- Move svg decoder into glycin-ng main crate (always-on)
- **libglycin-shim:** Add opt-in svg feature backed by resvg (mpl-2.0)
- Scaffold libglycin-shim with gly_ forwarding to glycin_ng
- **pixbuf-loader:** Add masked signatures for ico, cur, and jxl container
- **pixbuf-loader:** Convert f16 channels to u8 instead of zeroing
- **pixbuf-loader:** Wire load_animation through GdkPixbufSimpleAnim
- **pixbuf-loader:** Implement incremental begin/increment/stop load
- Scaffold gdk-pixbuf loader module backed by glycin-ng
- Add benchmarks, expanded orientation and concurrent tests, readme
- Add c abi with opaque handles, header, and example program
- Parse exif orientation and bake into pixels
- Wire jpeg, gif, webp, tiff, bmp, ico, tga, qoi, exr, pnm, dds, jxl decoders
- Decode png and apng with limits and sandbox
- Add landlock, seccomp, and rlimit sandbox layers with worker thread
- Add format sniffer and builder-style image loader
- Add core image, frame, texture, limits, and sandbox posture types

### Performance

- Tune release profile for size (opt-level=z, lto, strip, panic=abort)

### Refactor

- **libglycin-shim:** Ignore gly_loader_set_sandbox_selector


