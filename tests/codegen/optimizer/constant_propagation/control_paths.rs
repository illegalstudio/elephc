//! Purpose:
//! Integration or regression tests for optimizer-sensitive codegen coverage of optimizer, constant propagation branching control paths, including constant propagation merges identical switch constants, constant propagation uses known switch subject merge, and constant propagation merges identical try catch constants.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled and run so folding, propagation, or pruning stays behavior-preserving.

use super::*;

#[test]
fn test_constant_propagation_merges_identical_switch_constants() {
    let dir = make_cli_test_dir("elephc_constant_propagation_switch_merge");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
switch ($argc) {
    case 1:
        $base = 2;
        break;
    default:
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
        "merged switch constants should let pow disappear from user assembly:\n{}",
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
fn test_constant_propagation_uses_known_switch_subject_merge() {
    let dir = make_cli_test_dir("elephc_constant_propagation_switch_known_subject");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$mode = 1;
switch ($mode) {
    case 1:
        $base = 2;
        break;
    default:
        $base = 9;
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
        "known switch subject should limit constant propagation merge paths:\n{}",
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
fn test_constant_propagation_merges_identical_try_catch_constants() {
    let dir = make_cli_test_dir("elephc_constant_propagation_try_merge");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
try {
    $base = 2;
} catch (Exception $e) {
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
        "merged try/catch constants should let pow disappear from user assembly:\n{}",
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
fn test_constant_propagation_ignores_unreachable_catch_constants() {
    let dir = make_cli_test_dir("elephc_constant_propagation_non_throwing_try");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
try {
    $base = 2;
} catch (Exception $e) {
    $base = 9;
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
        "non-throwing try body should keep unreachable catch constants out of the merge:\n{}",
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
