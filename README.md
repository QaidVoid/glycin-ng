# glycin-ng

Permissively-licensed Rust image decoder library with in-process
sandboxing.

`glycin-ng` decodes images via pure-Rust decoder crates (`png`,
`jpeg-decoder`, `gif`, `image-webp`, `tiff`, `jxl-oxide`, ...) and
applies a layered Linux sandbox (landlock, seccomp, setrlimit) to the
decoding worker thread. The actually-applied sandbox posture is
reported back so callers can audit or refuse a degraded posture.

Versus upstream
[glycin](https://gitlab.gnome.org/GNOME/glycin), every decode runs
in-process via a function call rather than across a `bwrap` boundary
and a D-Bus IPC, removing the per-format loader binaries.

## Status

Pre-release. The public surface is stable enough to integrate against
but is not yet on crates.io. Targets Linux (sandbox is a no-op on
other platforms).

## Features

| Format               | Crate           | License        |
|----------------------|-----------------|----------------|
| PNG / APNG           | png             | MIT / Apache-2 |
| JPEG                 | jpeg-decoder    | MIT / Apache-2 |
| GIF (animated)       | gif             | MIT / Apache-2 |
| WebP (animated)      | image-webp      | MIT / Apache-2 |
| TIFF                 | tiff            | MIT / Apache-2 |
| BMP                  | image           | MIT / Apache-2 |
| ICO / CUR            | image           | MIT / Apache-2 |
| TGA                  | image           | MIT / Apache-2 |
| QOI                  | qoi             | MIT / Apache-2 |
| OpenEXR              | image (exr)     | BSD-3-Clause   |
| PNM / PBM / PGM / PPM| image           | MIT / Apache-2 |
| DDS                  | image           | MIT / Apache-2 |
| JPEG XL              | jxl-oxide       | BSD-3-Clause   |

Deferred (no permissive decoder available): SVG, HEIF, AVIF, RAW.

## Quick start (Rust)

```rust
use glycin_ng::Loader;

let image = Loader::new_path("photo.png").load()?;
let frame = image.first_frame().expect("at least one frame");
let texture = frame.texture();
println!(
    "{}x{} {:?} format, {} bytes",
    texture.width(),
    texture.height(),
    texture.format(),
    texture.data().len()
);
```

The default loader enables landlock and seccomp on Linux; query the
applied posture with `image.sandbox_posture()`. Callers that require
a particular posture can opt into strict mode:

```rust
let image = Loader::new_bytes(bytes)
    .require_sandbox()
    .load()?;
```

`require_sandbox()` returns `Error::SandboxUnavailable` if any
requested layer cannot be enforced.

## Quick start (C)

```c
#include "glycin_ng.h"

GlycinNgLoader* loader = glycin_ng_loader_new_path("photo.png");
GlycinNgImage* image = glycin_ng_loader_load(loader);
if (!image) {
    fprintf(stderr, "decode failed: %s\n", glycin_ng_last_error());
    return 1;
}
printf("%ux%u\n",
       glycin_ng_image_width(image),
       glycin_ng_image_height(image));
glycin_ng_image_free(image);
```

Build the shared library with `cargo build --release --features
c-api`. The generated `libglycin_ng.so` is loadable via `dlopen` and
linkable against the bundled `include/glycin_ng.h`.

A worked example is in `examples/c_load.c`.

## Sandboxing

Three independent layers, applied per decode on a dedicated worker
thread that is joined before the call returns:

- **landlock** (default on, Linux 5.13+): denies all filesystem
  access from the decoder thread. Image bytes are read on the main
  thread before the sandbox is applied.
- **seccomp** (default on): installs a BPF allowlist of the syscalls
  Rust stdlib + decoders need. Denied syscalls return `EPERM`.
- **setrlimit** (off by default): caps `RLIMIT_AS` and `RLIMIT_CPU`
  per `Limits`. Process-wide; opt in only when the host is sized for
  it.

Disable layers via `Loader::sandbox_selector(...)`. Inspect what
landed with `Image::sandbox_posture()`.

Measured per-decode overhead on Linux 6.x x86_64 (1x1 PNG, criterion):

| Posture                  | Overhead |
|--------------------------|----------|
| No sandbox               | ~ 30 us  |
| Landlock only            | ~ 32 us  |
| Seccomp only             | ~130 us  |
| Landlock + seccomp       | ~128 us  |

## Feature flags

Capability groups: `decode` (default), `encode` (off), `metadata`
(default).

Sandbox layers: `landlock`, `seccomp` (both default, Linux only).

Per-format: `png`, `jpeg`, `gif`, `webp`, `tiff`, `bmp`, `ico`,
`tga`, `qoi`, `exr`, `pnm`, `dds`, `jxl` (all default).

C ABI: `c-api` (off by default; turn on to expose
`libglycin_ng.so`).

Build with no defaults to get just the library types and dispatch
shell:

```bash
cargo build --no-default-features
```

## Limits

Every decode is bounded by a `Limits` struct, applied pre-decode for
the cheap header check and inside the decoder for buffer caps:

| Field                  | Default     |
|------------------------|-------------|
| `max_width`            | 32768       |
| `max_height`           | 32768       |
| `max_pixels`           | 256 MiPx    |
| `max_frames`           | 1024        |
| `max_animation_duration` | 60 s      |
| `decode_memory_mib`    | 512 (RLIMIT_AS when rlimit is on) |
| `decode_cpu_seconds`   | 30 (RLIMIT_CPU when rlimit is on) |

## License

MIT OR Apache-2.0.

`cargo deny check` enforces that no transitive dependency carries a
non-permissive license. Any addition of an MPL, LGPL, GPL, or other
copyleft dependency fails CI.
