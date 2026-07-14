//! Purpose:
//! Regression coverage for feature-gated runtime and synthetic builtin reachability.
//!
//! Called from:
//! - `cargo test` through the codegen integration-test harness.
//!
//! Key details:
//! - Plain native programs must not carry the optional eval Reflection surface.

use crate::support::{compile_source_to_asm_with_options, fs, make_cli_test_dir};

/// Verifies a program without eval or Reflection omits their synthetic methods and metadata.
#[test]
fn test_plain_program_omits_unreferenced_reflection_surface() {
    let dir = make_cli_test_dir("elephc_plain_runtime_reachability");
    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options("<?php echo 1;", &dir, 8_388_608, false, false);

    assert!(
        !user_asm.contains("@fn name=Reflection"),
        "plain program unexpectedly lowered synthetic Reflection methods"
    );
    assert!(
        !user_asm.contains("_eval_reflection_"),
        "plain program unexpectedly emitted eval Reflection metadata"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|library| library == "elephc_magician"),
        "plain program unexpectedly requested the Magician bridge"
    );

    let _ = fs::remove_dir_all(&dir);
}
