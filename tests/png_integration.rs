//! End-to-end tests: encode a PNG in memory, decode through the
//! public `Loader` API, and verify the result plus the sandbox
//! posture.

#![cfg(feature = "png")]

use glycin_ng::{LandlockPosture, Limits, Loader, MemoryFormat, SandboxSelector};

fn encode_rgba_png(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().unwrap();
        let data = vec![0x80; (width * height * 4) as usize];
        writer.write_image_data(&data).unwrap();
    }
    out
}

#[test]
fn loader_decodes_png_from_bytes() {
    let bytes = encode_rgba_png(16, 16);
    let image = Loader::new_bytes(bytes).load().unwrap();
    assert_eq!(image.width(), 16);
    assert_eq!(image.height(), 16);
    assert_eq!(image.format_name(), "png");
    assert_eq!(image.frames().len(), 1);

    let frame = image.first_frame().unwrap();
    assert_eq!(frame.texture().format(), MemoryFormat::R8g8b8a8);
    assert_eq!(frame.texture().data().len(), 16 * 16 * 4);
}

#[test]
fn loader_decodes_png_from_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.png");
    let bytes = encode_rgba_png(8, 8);
    std::fs::write(&path, &bytes).unwrap();

    let image = Loader::new_path(&path).load().unwrap();
    assert_eq!(image.width(), 8);
    assert_eq!(image.height(), 8);
}

#[test]
#[cfg(target_os = "linux")]
fn sandbox_is_active_during_decode_by_default() {
    let bytes = encode_rgba_png(4, 4);
    let image = Loader::new_bytes(bytes).load().unwrap();
    let posture = image.sandbox_posture();
    // The default selector enables landlock + seccomp on Linux.
    // We assert that at least one of them landed; on kernels without
    // landlock the test only asserts seccomp.
    let landlock_enforced = matches!(posture.landlock, LandlockPosture::Enforced { .. });
    let seccomp_enforced = matches!(posture.seccomp, glycin_ng::SeccompPosture::Enforced);
    assert!(
        landlock_enforced || seccomp_enforced,
        "expected landlock or seccomp to be enforced; got {posture:?}"
    );
}

#[test]
fn loader_can_disable_sandbox() {
    let bytes = encode_rgba_png(4, 4);
    let image = Loader::new_bytes(bytes)
        .sandbox_selector(SandboxSelector::none())
        .load()
        .unwrap();
    let posture = image.sandbox_posture();
    assert!(matches!(posture.landlock, LandlockPosture::Disabled));
    assert!(matches!(
        posture.seccomp,
        glycin_ng::SeccompPosture::Disabled
    ));
}

#[test]
fn loader_honors_limits() {
    let bytes = encode_rgba_png(100, 100);
    let limits = Limits {
        max_width: 50,
        ..Limits::default()
    };
    let err = Loader::new_bytes(bytes).limits(limits).load().unwrap_err();
    assert!(matches!(err, glycin_ng::Error::LimitExceeded("max_width")));
}

#[test]
fn loader_rejects_garbage_bytes_with_unsupported_format() {
    let err = Loader::new_bytes(b"not an image".to_vec())
        .load()
        .unwrap_err();
    assert!(matches!(err, glycin_ng::Error::UnsupportedFormat));
}
