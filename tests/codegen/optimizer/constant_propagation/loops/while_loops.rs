//! Purpose:
//! Integration or regression tests for optimizer-sensitive codegen coverage of optimizer, constant propagation loops, including constant propagation preserves scalar across while false body writes, constant propagation tracks do while false assignment, and constant propagation tracks while true break assignment.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled and run so folding, propagation, or pruning stays behavior-preserving.

use super::*;

/// Verifies that a scalar assigned before a `while (false)` loop (`$base = 2`) is preserved
/// because the body never executes, so `$base ** 3` folds to `8`.
#[test]
fn test_constant_propagation_preserves_scalar_across_while_false_body_writes() {
    let dir = make_cli_test_dir("elephc_constant_propagation_while_false");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$base = 2;
while (false) {
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
        "while(false) body writes should not poison incoming scalar constants:\n{}",
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

/// Verifies that a `do/while(false)` loop body assigns `$base = 2` which is used after,
/// so `$base ** 3` folds to `8`.
#[test]
fn test_constant_propagation_tracks_do_while_false_assignment() {
    let dir = make_cli_test_dir("elephc_constant_propagation_do_while_false");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
do {
    $base = 2;
} while (false);
echo $base ** 3;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "do/while(false) body assignment should feed the post-loop env:\n{}",
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

/// Verifies that `while (true) { $base = 2; break; }` produces a constant `$base` after the loop,
/// so `$base ** 3` folds to `8`.
#[test]
fn test_constant_propagation_tracks_while_true_break_assignment() {
    let dir = make_cli_test_dir("elephc_constant_propagation_while_true_break");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
while (true) {
    $base = 2;
    break;
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
        "while(true) break assignment should feed the post-loop env:\n{}",
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

/// Verifies that when both branches of an `if` inside `while (true)` `break` with the same
/// constant (`$base = 2`), they merge and `$base ** 3` folds to `8`.
#[test]
fn test_constant_propagation_merges_branch_breaks_through_while_true() {
    let dir = make_cli_test_dir("elephc_constant_propagation_while_branch_breaks");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
while (true) {
    if ($argc > 1) {
        $base = 2;
        break;
    } else {
        $base = 2;
        break;
    }
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
        "agreeing branch breaks should merge into the post-loop env:\n{}",
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

/// Verifies that `continue` in a `do/while(false)` loop still assigns `$base = 2` which is
/// used after the loop, so `$base ** 3` folds to `8`.
#[test]
fn test_constant_propagation_tracks_do_while_false_continue_assignment() {
    let dir = make_cli_test_dir("elephc_constant_propagation_do_while_false_continue");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
do {
    $base = 2;
    continue;
} while (false);
echo $base ** 3;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "continue in do/while(false) should feed the post-loop env:\n{}",
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
