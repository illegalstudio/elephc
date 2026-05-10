//! Purpose:
//! Integration or regression tests for optimizer-sensitive codegen coverage of optimizer, constant propagation loops, including constant propagation preserves scalar across foreach loop.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled and run so folding, propagation, or pruning stays behavior-preserving.

use super::*;

#[test]
fn test_constant_propagation_preserves_scalar_across_foreach_loop() {
    let dir = make_cli_test_dir("elephc_constant_propagation_foreach");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$base = 2;
foreach ([1, 2, 3] as $k => $value) {
    echo $value;
}
echo $base ** 3;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "simple foreach should preserve unrelated scalar constants in user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "1238");

    let _ = fs::remove_dir_all(&dir);
}
