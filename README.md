# glycin-ng

[![Crates.io](https://img.shields.io/crates/v/glycin-ng.svg?logo=rust)](https://crates.io/crates/glycin-ng)
[![Docs.rs](https://img.shields.io/docsrs/glycin-ng.svg?logo=docs.rs)](https://docs.rs/glycin-ng)
[![License](https://img.shields.io/crates/l/glycin-ng.svg)](#license)
[![cargo-deny](https://github.com/QaidVoid/glycin-ng/actions/workflows/deny.yml/badge.svg)](https://github.com/QaidVoid/glycin-ng/actions/workflows/deny.yml)

glycin-ng is an image decoding library for Linux, written in Rust. It
can stand in for [glycin](https://gitlab.gnome.org/GNOME/glycin), the
loader that gdk-pixbuf, GTK and many GNOME apps use, through a
`libglycin-2.so.0` compatibility shim.

Decoders run inside the calling process, on a short-lived worker
thread restricted with Landlock and seccomp. Every decoder is a Rust
crate under a permissive license.

## How it compares to glycin

Image files often come from untrusted sources, and decoders are a
common place for security bugs. glycin and glycin-ng handle that risk
in different ways.

glycin decodes common formats with Rust crates, and JPEG XL, HEIF,
AVIF and some others with libraries written in C and C++, such as
libjxl, libheif and libdav1d. It runs each format's loader in a separate
process under bubblewrap and talks to it over D-Bus, so a memory bug
in a decoder stays inside that process. This needs bubblewrap and
user namespaces at runtime. Where those are unavailable, as in some
containers, glycin cannot decode images at all.

glycin-ng uses Rust decoders for every format, which rules out most
of the memory bugs found in C and C++ decoders. Decoding runs in-process on a
worker thread restricted with Landlock and seccomp, so it works
anywhere the library loads, without helper binaries, D-Bus or
bubblewrap. Because there is no process boundary, a bug in a
decoder's `unsafe` code would affect the host application.

|                       | glycin                                 | glycin-ng                            |
|-----------------------|----------------------------------------|--------------------------------------|
| Decoder language      | Rust, plus C and C++ for some formats  | Rust                                 |
| Decoder isolation     | separate process per format, bubblewrap | worker thread, Landlock and seccomp |
| Works without bubblewrap | no                                  | yes                                  |
| Communication         | peer-to-peer D-Bus                     | function call                        |
| Install size on Arch  | about 37 MiB with its codec libraries  | about 6 MiB                          |
| Dependency licenses   | includes LGPL and MPL                  | permissive only                      |
| Format coverage       | broader (HEIF, camera RAW and more)    | see [Formats](#formats)              |

## Using it from Rust

```rust
use glycin_ng::Loader;

let image = Loader::new_path("photo.png").load()?;
let texture = image.first_frame().expect("no frames").texture();

println!(
    "{}x{} {:?}",
    texture.width(),
    texture.height(),
    texture.format(),
);
```

By default a decode still runs if the kernel cannot apply a sandbox
layer. `Image::sandbox_posture()` reports what was applied. To fail
instead, use `require_sandbox()`:

```rust
let image = Loader::new_bytes(bytes).require_sandbox().load()?;
```

This returns `Error::SandboxUnavailable` naming the missing layer.

## Using it from C

Build `libglycin_ng.so` with:

```
cargo build --release -p glycin-ng-c
```

The header is `include/glycin_ng.h`:

```c
#include "glycin_ng.h"

GlycinNgLoader *loader = glycin_ng_loader_new_path("photo.png");
GlycinNgImage *image = glycin_ng_loader_load(loader);
if (!image) {
    fprintf(stderr, "%s\n", glycin_ng_last_error());
    return 1;
}

printf("%ux%u\n", glycin_ng_image_width(image), glycin_ng_image_height(image));
glycin_ng_image_free(image);
```

A complete program is in `examples/c_load.c`.

## Replacing glycin on a system

[`libglycin-shim`](libglycin-shim/) builds `libglycin-2.so.0` on top of
`libglycin_ng.so`, so programs linked against glycin use glycin-ng
without being rebuilt. The `PKGBUILD` in this repository packages both
libraries for Arch Linux.

[`librsvg-shim`](librsvg-shim/) does the same for `librsvg-2.so.2`,
rendering SVG with resvg.

## Formats

| Format     | Decoder         | Animation | Encode |
|------------|-----------------|-----------|--------|
| PNG, APNG  | png             | yes       | yes    |
| JPEG       | jpeg-decoder    | no        | yes    |
| GIF        | gif             | yes       | yes    |
| WebP       | image-webp      | yes       | yes    |
| TIFF       | tiff            | no        | yes    |
| BMP        | image           | no        | yes    |
| ICO, CUR   | image           | no        | no     |
| TGA        | image           | no        | no     |
| QOI        | qoi             | no        | no     |
| OpenEXR    | image           | no        | no     |
| PNM family | image           | no        | no     |
| DDS        | image           | no        | no     |
| JPEG XL    | jxl-oxide       | yes       | no     |
| AVIF       | glycin-ng-rav1d | no        | no     |
| SVG        | resvg           | no        | no     |

For ICO and CUR files, the largest image in the file is returned.
OpenEXR images are returned as 16 or 32-bit floating point. AVIF
supports alpha and 10 and 12-bit images. SVG includes GTK's symbolic
icons.

HEIF and camera RAW are not supported because there is no
permissively licensed Rust decoder for them yet. AVIF image grids and
animations are not decoded; an animated AVIF shows its first frame.

`glycin-ng-rav1d` is a fork of [rav1d](https://github.com/memorysafety/rav1d)
that tracks upstream `main` and keeps the AV1 decoder's C symbols out
of `libglycin_ng.so`.

## Sandbox

Each decode runs on a dedicated `glycin-ng-worker` thread that is
joined before the call returns. Up to three layers apply to that
thread:

| Layer    | Default | Effect                                                                 |
|----------|---------|------------------------------------------------------------------------|
| Landlock | on      | No filesystem access. TCP bind and connect blocked from ABI 4, abstract Unix sockets and signals scoped from ABI 6. Needs Linux 5.13 or later. |
| seccomp  | on      | System call allowlist; anything else fails with `EPERM`.               |
| rlimit   | off     | `RLIMIT_AS` and `RLIMIT_CPU` taken from `Limits`.                     |

Choose layers with `Loader::sandbox_selector`. Landlock uses the
newest ABI the kernel offers, up to 6.

The seccomp filter is built in-tree. The allowlist is checked on
x86_64, aarch64, riscv64, ppc64, ppc64le and loongarch64. On other
architectures seccomp reports `Unsupported` and the other layers still
apply.

Most of the per-decode overhead comes from installing the seccomp
filter, which the kernel compiles on every install. `cargo bench
--bench sandbox_overhead` measures it on your machine.

## Limits

| Limit                    | Default                              |
|--------------------------|--------------------------------------|
| `max_width`              | 32768                                |
| `max_height`             | 32768                                |
| `max_pixels`             | 256 Mpx                              |
| `max_frames`             | 1024                                 |
| `max_animation_duration` | 60 s                                 |
| `decode_memory_mib`      | 512, applied when rlimit is enabled  |
| `decode_cpu_seconds`     | 30, applied when rlimit is enabled   |

Change them with `Loader::limits`.

## Cargo features

| Features                                       | Default | Purpose                                      |
|------------------------------------------------|---------|----------------------------------------------|
| `decode`, `metadata`                           | on      | decoding, metadata and orientation handling  |
| `encode`                                       | off     | encoding to PNG, JPEG, GIF, WebP, TIFF, BMP  |
| `landlock`, `seccomp`                          | on      | sandbox layers (Linux)                       |
| `png`, `jpeg`, `gif`, `webp`, `tiff`, `bmp`, `ico`, `tga`, `qoi`, `exr`, `pnm`, `dds`, `jxl`, `svg`, `avif` | on | one per format |
| `c-api`                                        | off     | C bindings used by `glycin-ng-c`             |

To build with only the formats you need:

```
cargo build --no-default-features --features decode,landlock,seccomp,png,jpeg
```

## License

MIT or Apache-2.0, at your option.

CI runs `cargo deny` on every push and pull request to check that no
dependency uses a copyleft license.
