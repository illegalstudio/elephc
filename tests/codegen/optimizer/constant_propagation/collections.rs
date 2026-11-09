//! Purpose:
//! Integration or regression tests for optimizer-sensitive codegen coverage of optimizer, constant propagation collections, including constant propagation tracks scalar list unpack, constant propagation tracks scalar array literal access, and constant propagation tracks scalar assoc array literal access.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled and run so folding, propagation, or pruning stays behavior-preserving.

use super::*;

/// Verifies that list unpacking of a scalar literal array (`[$base, $exp] = [2, 3]`)
/// propagates the constants so `$base ** $exp` folds to `8` with no `pow` call.
#[test]
fn test_constant_propagation_tracks_scalar_list_unpack() {
    let dir = make_cli_test_dir("elephc_constant_propagation_list_unpack");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
[$base, $exp] = [2, 3];
echo $base ** $exp;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "scalar list unpack should let pow disappear from user assembly:\n{}",
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

/// Verifies that a literal indexed array access (`$base = [2, 9][0]`) is folded to `2`
/// before propagation, so `$base ** 3` folds to `8` with no `pow` call.
#[test]
fn test_constant_propagation_tracks_scalar_array_literal_access() {
    let dir = make_cli_test_dir("elephc_constant_propagation_array_literal_access");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$base = [2, 9][0];
echo $base ** 3;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "scalar array literal access should fold before assignment propagation:\n{}",
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

/// Verifies that a literal associative array access (`$base = ["left" => 2, "right" => 9]["left"]`)
/// is folded to `2` before propagation, so `$base ** 3` folds to `8` with no `pow` call.
#[test]
fn test_constant_propagation_tracks_scalar_assoc_array_literal_access() {
    let dir = make_cli_test_dir("elephc_constant_propagation_assoc_array_literal_access");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$base = ["left" => 2, "right" => 9]["left"];
echo $base ** 3;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "scalar assoc-array literal access should fold before assignment propagation:\n{}",
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

/// A literal `unset($GLOBALS['x'])` must invalidate a propagated top-level fact:
/// the global slot is empty afterward, so `$x ?? "gone"` is `"gone"`, not the
/// stale constant.
#[test]
fn test_unset_globals_literal_invalidates_propagated_global() {
    let out = compile_and_run(
        r#"<?php
$x = 5;
unset($GLOBALS["x"]);
echo $x ?? "gone";
"#,
    );
    assert_eq!(out, "gone");
}

/// A computed `$GLOBALS` key can name any global, so its unset invalidates every
/// propagated top-level fact.
#[test]
fn test_unset_globals_computed_invalidates_propagated_global() {
    let out = compile_and_run(
        r#"<?php
$x = 5;
$name = "x";
unset($GLOBALS[$name]);
echo $x ?? "gone";
"#,
    );
    assert_eq!(out, "gone");
}

/// A `$GLOBALS['x'] = ...` assignment expression invalidates the propagated top-level fact.
#[test]
fn test_globals_assignment_expression_invalidates_propagated_global() {
    let out = compile_and_run(
        r#"<?php
$x = 5;
$assigned = ($GLOBALS["x"] = 6);
echo $x, ":", $assigned;
"#,
    );
    assert_eq!(out, "6:6");
}
