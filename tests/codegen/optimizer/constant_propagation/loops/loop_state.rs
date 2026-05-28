//! Purpose:
//! Integration or regression tests for optimizer-sensitive codegen coverage of optimizer, constant propagation, loops loop-carried state, including constant propagation preserves scalar across loop with switch, constant propagation preserves scalar across loop with try, and constant propagation preserves scalar across nested loops.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled and run so folding, propagation, or pruning stays behavior-preserving.

use super::*;

/// Verifies that a `switch` inside a `for` loop does not kill a scalar (`$base = 2`)
/// assigned before the loop; `$base ** 3` still folds to `8`.
#[test]
fn test_constant_propagation_preserves_scalar_across_loop_with_switch() {
    let dir = make_cli_test_dir("elephc_constant_propagation_loop_switch");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$base = 2;
for ($i = 0; $i < 3; $i++) {
    switch ($i) {
        case 1:
            echo $i;
            break;
        default:
            echo $i;
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
        "loop-local switch should not kill unrelated scalar constants in user assembly:\n{}",
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
    assert_eq!(out, "0128");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies that a `try/catch/finally` inside a `for` loop does not kill a scalar (`$base = 2`)
/// assigned before the loop; `$base ** 3` still folds to `8`.
#[test]
fn test_constant_propagation_preserves_scalar_across_loop_with_try() {
    let dir = make_cli_test_dir("elephc_constant_propagation_loop_try");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$base = 2;
for ($i = 0; $i < 3; $i++) {
    try {
        echo $i;
    } catch (Exception $e) {
        echo 9;
    } finally {
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
        "loop-local try/catch/finally should not kill unrelated scalar constants in user assembly:\n{}",
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
    assert_eq!(out, "0128");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies that nested `for`/`while` loops do not kill a scalar (`$base = 2`) assigned before
/// the loops; `$base ** 3` still folds to `8`.
#[test]
fn test_constant_propagation_preserves_scalar_across_nested_loops() {
    let dir = make_cli_test_dir("elephc_constant_propagation_nested_loops");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$base = 2;
$i = 0;
for (; $i < 2; $i++) {
    $j = 0;
    while ($j < 2) {
        echo $j;
        $j++;
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
        "nested simple loops should preserve unrelated scalar constants in user assembly:\n{}",
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
    assert_eq!(out, "01018");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies that array writes inside a `for` loop (`$items[] = $i`, `$items[0] = $i`) do not
/// poison a scalar (`$base = 2`) assigned before the loop; `$base ** 3` still folds to `8`.
#[test]
fn test_constant_propagation_preserves_scalar_across_loop_local_array_writes() {
    let dir = make_cli_test_dir("elephc_constant_propagation_loop_array_writes");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$base = 2;
$items = [];
$i = 0;
for (; $i < 3; $i++) {
    $items[] = $i;
    $items[0] = $i;
    echo $i;
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
        "local array writes inside the loop should not poison unrelated scalar constants:\n{}",
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
    assert_eq!(out, "0128");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies that object property writes inside a `for` loop (`$box->last`, `$box->items[]`)
/// do not poison a scalar (`$base = 2`) assigned before the loop; `$base ** 3` still folds to `8`.
#[test]
fn test_constant_propagation_preserves_scalar_across_loop_property_writes() {
    let dir = make_cli_test_dir("elephc_constant_propagation_loop_property_writes");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
class Box {
    public $last = 0;
    public $items = [];
}

$box = new Box();
$base = 2;
$i = 0;
for (; $i < 3; $i++) {
    $box->last = $i;
    $box->items[] = $i;
    $box->items[0] = $i;
    echo $i;
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
        "local property writes inside the loop should not poison unrelated scalar constants:\n{}",
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
    assert_eq!(out, "0128");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies that `unset($tmp)` inside a `for` loop does not kill a scalar (`$base = 2`)
/// assigned before the loop; `$base ** 3` still folds to `8`.
#[test]
fn test_constant_propagation_preserves_scalar_across_unset_and_loop() {
    let dir = make_cli_test_dir("elephc_constant_propagation_unset");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$base = 2;
$i = 0;
for (; $i < 3; $i++) {
    $tmp = 9;
    unset($tmp);
    echo $i;
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
        "unsetting an unrelated local inside the loop should not poison scalar constants:\n{}",
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
    assert_eq!(out, "0128");

    let _ = fs::remove_dir_all(&dir);
}
