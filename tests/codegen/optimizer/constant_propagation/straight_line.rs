//! Purpose:
//! Integration or regression tests for optimizer-sensitive codegen coverage of optimizer, constant propagation straight-line programs, including constant propagation removes pow call from user assembly, constant propagation merges identical if constants, and constant propagation tracks uniform ternary assignment.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled and run so folding, propagation, or pruning stays behavior-preserving.

use super::*;

#[test]
fn test_constant_propagation_removes_pow_call_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_constant_propagation_pow");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$x = 2;
$y = 3;
echo $x ** $y;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "constant-propagated pow expression should not leave a pow call in user assembly:\n{}",
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
    assert_eq!(out, "8");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_propagation_merges_identical_if_constants() {
    let dir = make_cli_test_dir("elephc_constant_propagation_if_merge");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
if ($argc > 0) {
    $base = 2;
} else {
    $base = 2;
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
        "merged if constants should let pow disappear from user assembly:\n{}",
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
    assert_eq!(out, "8");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_propagation_tracks_uniform_ternary_assignment() {
    let dir = make_cli_test_dir("elephc_constant_propagation_ternary_uniform");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$base = ($argc > 0) ? 2 : 2;
echo $base ** 3;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "uniform ternary assignment should let pow disappear from user assembly:\n{}",
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
    assert_eq!(out, "8");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_propagation_tracks_uniform_match_assignment() {
    let dir = make_cli_test_dir("elephc_constant_propagation_match_uniform");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$base = match ($argc) {
    1 => 2,
    default => 2,
};
echo $base ** 3;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "uniform match assignment should let pow disappear from user assembly:\n{}",
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
    assert_eq!(out, "8");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_propagation_tracks_known_match_assignment() {
    let dir = make_cli_test_dir("elephc_constant_propagation_match_known");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$mode = 1;
$base = match ($mode) {
    1 => 2,
    default => 9,
};
echo $base ** 3;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "known match subject should fold before assignment propagation:\n{}",
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
    assert_eq!(out, "8");

    let _ = fs::remove_dir_all(&dir);
}
