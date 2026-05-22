# glycin-ng

[![Crates.io](https://img.shields.io/crates/v/glycin-ng.svg?logo=rust)](https://crates.io/crates/glycin-ng)
[![Docs.rs](https://img.shields.io/docsrs/glycin-ng.svg?logo=docs.rs)](https://docs.rs/glycin-ng)
[![License](https://img.shields.io/crates/l/glycin-ng.svg)](#license)
[![cargo-deny](https://github.com/QaidVoid/glycin-ng/actions/workflows/deny.yml/badge.svg)](https://github.com/QaidVoid/glycin-ng/actions/workflows/deny.yml)

Drop-in replacement for
[upstream glycin](https://gitlab.gnome.org/GNOME/glycin). One
in-process Rust shared library.

- **~9x smaller install.** ~4 MiB vs ~37 MiB on Arch.
- **No bubblewrap. No D-Bus. No helper binaries.**
- **Permissive licensing only.** No LGPL or MPL transitive code.
- **Per-decode sandbox.** Landlock + seccomp + rlimit on the worker
  thread.

```
                  +-----------------+
                  |  Caller thread  |
                  +--------+--------+
                           |
                           | Loader::load(bytes_or_path)
                           v
        +------------------+------------------+
        |   glycin-ng-worker thread           |
        |  +-------------------------------+  |
        |  | rlimit   (RLIMIT_AS, _CPU)    |  |
        |  +-------------------------------+  |
        |  | landlock (FS + net + scope)   |  |
        |  +-------------------------------+  |
        |  | seccomp  (BPF allowlist)      |  |
        |  +-------------------------------+  |
        |  |   Decoder  (pure Rust crate)  |  |
        |  +-------------------------------+  |
        +------------------+------------------+
                           |
                           | join, return frames + posture
                           v
                  +--------+--------+
                  |  Image, frames  |
                  +-----------------+
```

## Quickstart

### Rust

```rust
use glycin_ng::Loader;

let image = Loader::new_path("photo.png").load()?;
let frame = image.first_frame().expect("at least one frame");
let texture = frame.texture();

println!(
    "{}x{} {:?}, {} bytes",
    texture.width(),
    texture.height(),
    texture.format(),
    texture.data().len(),
);

if let glycin_ng::LandlockPosture::Enforced { abi } =
    image.sandbox_posture().landlock
{
    println!("decoded under landlock abi v{abi}");
}
```

Refuse degraded sandbox:

```rust
let image = Loader::new_bytes(bytes)
    .require_sandbox()
    .load()?;
```

`require_sandbox()` returns `Error::SandboxUnavailable("landlock")`
(or `"seccomp"`, `"rlimit"`) on any kernel that cannot enforce a
selected layer.

### C

```c
#include "glycin_ng.h"

GlycinNgLoader *loader = glycin_ng_loader_new_path("photo.png");
GlycinNgImage *image = glycin_ng_loader_load(loader);
if (!image) {
    fprintf(stderr, "%s\n", glycin_ng_last_error());
    return 1;
}

printf("%ux%u\n",
    glycin_ng_image_width(image),
    glycin_ng_image_height(image));

glycin_ng_image_free(image);
```

Build `libglycin_ng.so` plus `include/glycin_ng.h`:

```
cargo build --release --features c-api
```

Worked example in `examples/c_load.c`.

## How it differs from upstream

Upstream glycin sits in the same position in the stack: it is the
loader library new versions of `gdk-pixbuf` and GNOME apps depend on.
It spawns one helper process per format under `bwrap`, talks to it
over peer-to-peer D-Bus, and inherits LGPL / MPL transitive code from
the codec libraries those helpers link against (`librsvg`, `libjxl`,
`libheif`, `libopenraw`, ...).

|                              | upstream glycin                              | glycin-ng                                          |
|------------------------------|----------------------------------------------|----------------------------------------------------|
| Install footprint            | ~37 MiB (`glycin` + `librsvg` + `libjxl` + `bubblewrap`; grows with `glycin-loaders`, `libheif`, `libopenraw`) | ~4 MiB (`libglycin_ng.so` + shim) |
| Decoder license surface      | mixed (LGPL, MPL, BSD)                       | permissive only (MIT, Apache, BSD, ISC, Zlib)      |
| Decode boundary              | separate process per format                  | in-process worker thread                           |
| Sandbox mechanism            | bwrap (mount / PID / user ns)                | landlock + seccomp + rlimit                        |
| IPC                          | peer-to-peer D-Bus                           | direct function call                               |
| Per-decode cost              | process spawn + namespace + IPC              | thread spawn + prctl                               |
| Helper binaries shipped      | one per format                               | none                                               |
| Behaves under Flatpak / AppImage / distrobox | needs a sandbox helper to nest | nests cleanly (layers only narrow further) |

If you want every available codec including the LGPL ones, you want
upstream glycin. If you want permissive licensing, a small install,
or you're packaging into something already sandboxed where bwrap
nesting is awkward, you want this.

## Supported formats

| Format          | Backing crate   | Decode | Encode | Notes                                |
|-----------------|-----------------|--------|--------|--------------------------------------|
| PNG / APNG      | png             | yes    | yes    | animation                            |
| JPEG            | jpeg-decoder    | yes    | yes    |                                      |
| GIF             | gif             | yes    | yes    | animation                            |
| WebP            | image-webp      | yes    | yes    | animation                            |
| TIFF            | tiff            | yes    | yes    |                                      |
| BMP             | image           | yes    | yes    |                                      |
| ICO / CUR       | image           | yes    | -      | picks largest entry                  |
| TGA             | image           | yes    | -      |                                      |
| QOI             | qoi             | yes    | -      |                                      |
| OpenEXR         | image (exr)     | yes    | -      | 16 / 32-bit float, HDR-aware         |
| PNM family      | image           | yes    | -      |                                      |
| DDS             | image           | yes    | -      |                                      |
| JPEG XL         | jxl-oxide       | yes    | -      |                                      |
| SVG             | resvg / usvg    | yes    | -      | GTK symbolic-icon wrappers expanded  |

Deferred because no permissive decoder exists yet: HEIF, AVIF, RAW.

## Sandbox

Each decode runs on a dedicated `glycin-ng-worker` thread, joined
before the call returns. Three layers stack on that thread:

| Layer      | Default | What it does                                | Failure surface                |
|------------|---------|---------------------------------------------|---------------------------------|
| landlock   | on      | denies all FS paths to the worker; on V4+ also TCP bind/connect; on V6+ scopes abstract-unix-socket and signals | `Unsupported` on pre-5.13 kernels |
| seccomp    | on      | BPF allowlist; everything else returns `EPERM` | `Unsupported` if `prctl` fails |
| rlimit     | off     | `RLIMIT_AS` and `RLIMIT_CPU` from `Limits`  | `PartiallyApplied` per limit   |

Toggle layers with `Loader::sandbox_selector(SandboxSelector { ... })`.
Inspect the result with `Image::sandbox_posture()` and decide whether
to log, audit, or refuse a degraded posture.

Landlock negotiates up to ABI V6 at runtime and degrades cleanly. The
crate ships built-in regression tests asserting both that an unlisted
syscall (`socket`) is denied under seccomp, and that the worker
spawns a rayon pool for JPEG / JXL without tripping `clone3`.

The dominant cost is the seccomp install: the BPF program is
JIT-compiled into the kernel on every `prctl(PR_SET_SECCOMP)`, so its
overhead scales with the size of the allowlist. Landlock adds a
single-digit microsecond cost on top. Run `cargo bench --bench
sandbox_overhead` for the numbers on your specific hardware.

## Limits

Every decode is bounded:

| Field                    | Default                          |
|--------------------------|----------------------------------|
| `max_width`              | 32768                            |
| `max_height`             | 32768                            |
| `max_pixels`             | 256 Mpx                          |
| `max_frames`             | 1024                             |
| `max_animation_duration` | 60s                              |
| `decode_memory_mib`      | 512 (`RLIMIT_AS` if rlimit on)   |
| `decode_cpu_seconds`     | 30 (`RLIMIT_CPU` if rlimit on)   |

Override via `Loader::limits(Limits { ... })`.

## Feature flags

| Group          | Default                          | Notes                                   |
|----------------|----------------------------------|-----------------------------------------|
| Capability     | `decode`, `metadata`             | enable `encode` for PNG, JPEG, GIF, WebP, TIFF, BMP |
| Sandbox        | `landlock`, `seccomp` (Linux)    | toggling off is supported for portability testing, not as a production posture |
| Per-format     | `png`, `jpeg`, `gif`, `webp`, `tiff`, `bmp`, `ico`, `tga`, `qoi`, `exr`, `pnm`, `dds`, `jxl`, `svg` | trim individually     |
| ABI            | (off) `c-api`                    | enables the `cdylib` build and `cbindgen` header |

Minimum build:

```
cargo build --no-default-features
```

Trim individual formats:

```
cargo build --no-default-features --features decode,png,jpeg
```

## Related crates

- [`glycin-ng-libglycin-shim`](libglycin-shim/) - `libglycin-2.so.0`
  drop-in for systems that have hard-linked against upstream's
  libglycin (Arch's gdk-pixbuf2 is the canonical case).

## License

MIT OR Apache-2.0.

CI runs `cargo deny check` on every push and PR, enforcing that no
transitive dependency carries an MPL, LGPL, GPL, or other copyleft
license. A failing audit is a blocker.
