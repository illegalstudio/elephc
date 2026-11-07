//! Purpose:
//! Integration or regression tests for optimizer-sensitive codegen coverage of optimizer, constant propagation collections, including constant propagation tracks scalar list unpack, constant propagation tracks scalar array literal access, and constant propagation tracks scalar assoc array literal access.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled and run so folding, propagation, or pruning stays behavior-preserving.

use super::*;

/// Function, method and closure reference parameters may all identify the same caller variable.
#[test]
fn test_constant_propagation_preserves_aliased_reference_parameters_bundle() {
    let source = r#"<?php
function referenceParameterPair(array &$first, array &$second): void {
    $first = [2]; echo $second[0], '|';
    $second = [3]; echo $first[0], '|';
}
class ReferenceParameterPairMethod {
    public function run(array &$first, array &$second): void {
        $first = [2]; echo $second[0], '|';
        $second = [3]; echo $first[0], '|';
    }
}
$items = [$argc]; referenceParameterPair($items, $items);
$method = new ReferenceParameterPairMethod(); $method->run($items, $items);
$closure = function (array &$first, array &$second): void {
    $first = [2]; echo $second[0], '|';
    $second = [3]; echo $first[0], '|';
};
$closure($items, $items);
function referenceParameterList(&$first, &$second): void {
    [$first] = [2]; $second = 3; echo $first, '|';
}
$value = $argc; referenceParameterList($value, $value);
"#;
    assert_eq!(crate::support::compile_and_run(source), "2|3|2|3|2|3|3|");
}

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
