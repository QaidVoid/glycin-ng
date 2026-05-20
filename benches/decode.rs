//! Per-format decode throughput benchmarks.

use criterion::{Criterion, criterion_group, criterion_main};
use glycin_ng::{Loader, SandboxSelector};

fn encode_rgba_png(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().unwrap();
        writer
            .write_image_data(&vec![0x80; (width * height * 4) as usize])
            .unwrap();
    }
    out
}

fn bench_png_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("png_decode");
    for &size in &[64_u32, 256, 1024] {
        let bytes = encode_rgba_png(size, size);
        let bytes_ref: &[u8] = bytes.as_slice();
        group.throughput(criterion::Throughput::Bytes(bytes.len() as u64));
        group.bench_function(format!("size_{size}_no_sandbox"), |b| {
            b.iter(|| {
                let _img = Loader::new_bytes(bytes_ref.to_vec())
                    .sandbox_selector(SandboxSelector::none())
                    .load()
                    .unwrap();
            });
        });
    }
    group.finish();
}

fn bench_qoi_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("qoi_decode");
    for &size in &[64_u32, 256, 1024] {
        let pixels = vec![0x40u8; (size * size * 4) as usize];
        let bytes = qoi::encode_to_vec(&pixels, size, size).unwrap();
        group.throughput(criterion::Throughput::Bytes(bytes.len() as u64));
        group.bench_function(format!("size_{size}_no_sandbox"), |b| {
            b.iter(|| {
                let _img = Loader::new_bytes(bytes.clone())
                    .sandbox_selector(SandboxSelector::none())
                    .load()
                    .unwrap();
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_png_decode, bench_qoi_decode);
criterion_main!(benches);
