#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = glycin_ng::Loader::new_bytes(data.to_vec())
        .sandbox_selector(glycin_ng::SandboxSelector::none())
        .format_hint(glycin_ng::KnownFormat::WebP)
        .load();
});
