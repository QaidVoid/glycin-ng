//! Concurrent decodes from multiple threads must succeed without
//! data races or sandbox cross-talk.

#![cfg(feature = "png")]

use std::thread;

use glycin_ng::Loader;

fn encode_rgba_png(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().unwrap();
        writer
            .write_image_data(&vec![0x40; (width * height * 4) as usize])
            .unwrap();
    }
    out
}

#[test]
fn many_threads_can_decode_in_parallel() {
    let bytes = std::sync::Arc::new(encode_rgba_png(16, 16));
    let handles: Vec<_> = (0..16)
        .map(|i| {
            let bytes = bytes.clone();
            thread::spawn(move || {
                for _ in 0..8 {
                    let img = Loader::new_bytes(bytes.as_ref().clone()).load().unwrap();
                    assert_eq!(img.width(), 16, "worker {i}");
                    assert_eq!(img.height(), 16, "worker {i}");
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
}
