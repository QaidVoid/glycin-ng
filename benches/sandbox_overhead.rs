//! Measure the per-decode cost of applying the sandbox layers.
//!
//! Compares decoding a small image with no sandbox, with landlock
//! only, with seccomp only, and with both enforced. The delta is the
//! overhead of the worker thread setup and the LSM/BPF wiring.

use criterion::{Criterion, criterion_group, criterion_main};
use glycin_ng::{Loader, SandboxSelector};

fn tiny_png_bytes() -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, 1, 1);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().unwrap();
        writer.write_image_data(&[0, 0, 0, 255]).unwrap();
    }
    out
}

fn bench_sandbox_modes(c: &mut Criterion) {
    let bytes = tiny_png_bytes();
    let mut group = c.benchmark_group("sandbox_overhead");
    group.bench_function("no_sandbox", |b| {
        b.iter(|| {
            Loader::new_bytes(bytes.clone())
                .sandbox_selector(SandboxSelector::none())
                .load()
                .unwrap();
        });
    });
    group.bench_function("landlock_only", |b| {
        b.iter(|| {
            Loader::new_bytes(bytes.clone())
                .sandbox_selector(SandboxSelector {
                    landlock: true,
                    seccomp: false,
                    rlimit: false,
                    strict: false,
                })
                .load()
                .unwrap();
        });
    });
    group.bench_function("seccomp_only", |b| {
        b.iter(|| {
            Loader::new_bytes(bytes.clone())
                .sandbox_selector(SandboxSelector {
                    landlock: false,
                    seccomp: true,
                    rlimit: false,
                    strict: false,
                })
                .load()
                .unwrap();
        });
    });
    group.bench_function("landlock_plus_seccomp", |b| {
        b.iter(|| {
            Loader::new_bytes(bytes.clone()).load().unwrap();
        });
    });
    group.finish();
}

criterion_group!(benches, bench_sandbox_modes);
criterion_main!(benches);
