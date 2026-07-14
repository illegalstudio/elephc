//! Purpose:
//! Integration or regression tests for optimizer-sensitive codegen coverage of optimizer, constant propagation straight-line programs, including constant propagation removes pow call from user assembly, constant propagation merges identical if constants, and constant propagation tracks uniform ternary assignment.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled and run so folding, propagation, or pruning stays behavior-preserving.

use super::*;

/// Verifies that `2 ** 3` is constant-folded when both operands are assigned literals,
/// producing `8` with no `pow` call in the generated assembly.
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

/// Verifies that constant propagation does not substitute a scalar fact across
/// `eval`, because the fragment may replace the caller's visible local.
#[test]
fn test_constant_propagation_stops_at_eval_barrier() {
    let dir = make_cli_test_dir("elephc_constant_propagation_eval_barrier");
    let (user_asm, runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$x = 2;
eval('$x = 5;');
echo $x + 1;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    // The scalar-store fragment now compiles through the literal-eval AOT
    // path (no interpreter bridge); the barrier semantics are proven by the
    // runtime output below: propagation of `$x = 2` across the eval would
    // print 3 instead of 6.
    assert!(
        user_asm.contains("eval literal AOT"),
        "literal eval should compile through the AOT path:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        &runtime_obj_for_asm(&runtime_asm),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "6");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies that when both branches of an `if` assign the same constant (`$base = 2`)
/// the optimizer merges them and folds `$base ** 3` to `8` with no `pow` call.
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

/// Verifies that when both arms of a ternary assign the same constant (`$base = 2`)
/// the optimizer recognizes uniform assignment and folds `$base ** 3` to `8`.
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

/// Verifies that when all arms of a `match` assign the same constant (`$base = 2`)
/// the optimizer merges them and folds `$base ** 3` to `8`.
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

/// Verifies that when the `match` subject is a known constant (`$mode = 1`) the optimizer
/// can fold the selected arm (`$base = 2`) and then fold `$base ** 3` to `8`.
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

/// Regression for #384: a by-reference call used as a `match` subject mutates the
/// argument variable while evaluating the subject. Constant propagation must not
/// keep the pre-call constant (`$i = 0`) for a read of `$i` sequenced after the
/// `match`, so `echo match(bump($i)) {..} . "|" . $i` reports the post-call value.
#[test]
fn test_constant_propagation_match_subject_byref_writeback() {
    let out = compile_and_run(
        r#"<?php
function bump(&$i) { $i++; return $i; }
$i = 0;
echo match (bump($i)) {
    1 => "one",
    default => "other",
} . "|" . $i;
"#,
    );
    assert_eq!(out, "one|1");
}
