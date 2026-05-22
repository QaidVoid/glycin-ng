//! Verify `include/glycin_ng.h` is syntactically valid C and exposes
//! every symbol our public C ABI declares.
//!
//! The test writes a tiny `.c` file that takes the address of every
//! function declared in the header. `cc` is invoked to compile (not
//! link) it. If a symbol disappears from the header, this test
//! catches it before consumers do.

#![cfg(feature = "c-api")]

use std::process::Command;

fn probe_compiler() -> Option<String> {
    // Prefer $CC if set, otherwise try cc / gcc / clang in that order.
    if let Ok(cc) = std::env::var("CC")
        && Command::new(&cc).arg("--version").output().is_ok()
    {
        return Some(cc);
    }
    for name in ["cc", "gcc", "clang"] {
        if Command::new(name).arg("--version").output().is_ok() {
            return Some(name.into());
        }
    }
    None
}

#[test]
fn header_compiles_against_all_declared_symbols() {
    let Some(compiler) = probe_compiler() else {
        eprintln!("skipping: no C compiler available on PATH");
        return;
    };
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let header_dir = format!("{manifest_dir}/include");

    let src_path = std::env::temp_dir().join("glycin_ng_header_probe.c");
    let obj_path = std::env::temp_dir().join("glycin_ng_header_probe.o");
    std::fs::write(&src_path, PROBE_SRC).expect("write probe");

    let output = Command::new(&compiler)
        .arg("-Wall")
        .arg("-Werror")
        .arg("-I")
        .arg(&header_dir)
        .arg("-c")
        .arg(&src_path)
        .arg("-o")
        .arg(&obj_path)
        .output()
        .expect("invoke C compiler");

    let _ = std::fs::remove_file(&src_path);
    let _ = std::fs::remove_file(&obj_path);

    if !output.status.success() {
        panic!(
            "header probe failed to compile:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
}

const PROBE_SRC: &str = r#"
#include "glycin_ng.h"

/* Take the address of every declared function so the compiler
 * resolves and type-checks the declarations. We do not link, so
 * unresolved symbols at runtime are out of scope here. */
void glycin_ng_header_probe(void) {
    (void) (void*) &glycin_ng_last_error;
    (void) (void*) &glycin_ng_clear_last_error;

    (void) (void*) &glycin_ng_loader_new_path;
    (void) (void*) &glycin_ng_loader_new_bytes;
    (void) (void*) &glycin_ng_loader_free;
    (void) (void*) &glycin_ng_loader_sandbox;
    (void) (void*) &glycin_ng_loader_format_hint;
    (void) (void*) &glycin_ng_loader_apply_transformations;
    (void) (void*) &glycin_ng_loader_render_size_hint;
    (void) (void*) &glycin_ng_loader_set_max_width;
    (void) (void*) &glycin_ng_loader_set_max_height;
    (void) (void*) &glycin_ng_loader_set_max_pixels;
    (void) (void*) &glycin_ng_loader_set_max_frames;
    (void) (void*) &glycin_ng_loader_set_max_animation_seconds;
    (void) (void*) &glycin_ng_loader_set_decode_memory_mib;
    (void) (void*) &glycin_ng_loader_set_decode_cpu_seconds;
    (void) (void*) &glycin_ng_loader_load;

    (void) (void*) &glycin_ng_image_free;
    (void) (void*) &glycin_ng_image_width;
    (void) (void*) &glycin_ng_image_height;
    (void) (void*) &glycin_ng_image_frame_count;
    (void) (void*) &glycin_ng_image_is_animated;
    (void) (void*) &glycin_ng_image_orientation;
    (void) (void*) &glycin_ng_image_format_name;
    (void) (void*) &glycin_ng_image_texture;
    (void) (void*) &glycin_ng_image_frame_delay_ms;

    (void) (void*) &glycin_ng_texture_width;
    (void) (void*) &glycin_ng_texture_height;
    (void) (void*) &glycin_ng_texture_stride;
    (void) (void*) &glycin_ng_texture_format;
    (void) (void*) &glycin_ng_texture_data;
    (void) (void*) &glycin_ng_texture_data_len;

    (void) (void*) &glycin_ng_known_format_from_mime;
    (void) (void*) &glycin_ng_known_format_from_extension;

    (void) (void*) &glycin_ng_encoder_new;
    (void) (void*) &glycin_ng_encoder_free;
    (void) (void*) &glycin_ng_encoder_set_quality;
    (void) (void*) &glycin_ng_encoder_set_compression;
    (void) (void*) &glycin_ng_encoder_set_icc_profile;
    (void) (void*) &glycin_ng_encoder_add_metadata;
    (void) (void*) &glycin_ng_encoder_add_frame;
    (void) (void*) &glycin_ng_encoder_encode;
    (void) (void*) &glycin_ng_encoded_image_free;
    (void) (void*) &glycin_ng_encoded_image_data;
    (void) (void*) &glycin_ng_encoded_image_len;

    /* Sample of constants to ensure they remain integer-typed and
     * resolvable. */
    (void) GLYCIN_NG_FORMAT_R8G8B8A8;
    (void) GLYCIN_NG_FORMAT_R8G8B8A8_PRE;
    (void) GLYCIN_NG_KFMT_PNG;
    (void) GLYCIN_NG_KFMT_SVG;
}
"#;
