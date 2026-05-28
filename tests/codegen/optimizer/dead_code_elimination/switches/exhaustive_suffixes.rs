//! Purpose:
//! Integration or regression tests for optimizer-sensitive codegen coverage of optimizer, dead-code elimination, switches exhaustive switch suffixes, including dead code elimination prunes negated strict switch true case, dead code elimination prunes exhaustive negated and switch true default, and dead code elimination prunes exhaustive negated or switch true default.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled and run so folding, propagation, or pruning stays behavior-preserving.

use super::*;

/// Verifies that a negated strict comparison case (`!($value === 1)`) combined with an outer
/// guard (`$value !== 1`) makes the first case exhaustive and prunes it. Confirms "A".
#[test]
fn test_dead_code_elimination_prunes_negated_strict_switch_true_case() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_true_negated_strict");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($value) {
    if ($value !== 1) {
        switch (true) {
            case $value === 1:
                echo "dead-case";
                break;
            case !($value === 1):
                echo "A";
                break;
            default:
                echo "dead-default";
        }
    }
}

run(2);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "A");
    assert!(!user_asm.contains("dead-case"));
    assert!(!user_asm.contains("dead-default"));
}

/// Verifies that `case $a && $b` and `case !($a && $b)` make the default exhaustive, pruning it.
/// Confirms "B".
#[test]
fn test_dead_code_elimination_prunes_exhaustive_negated_and_switch_true_default() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_true_negated_and");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$a = $argc > 1;
$b = $argc > 2;
switch (true) {
    case $a && $b:
        echo "A";
        break;
    case !($a && $b):
        echo "B";
        break;
    default:
        echo "dead-default";
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "B");
    assert!(!user_asm.contains("dead-default"));
}

/// Verifies that `case $a || $b` and `case !($a || $b)` make the default exhaustive, pruning it.
/// Confirms "B".
#[test]
fn test_dead_code_elimination_prunes_exhaustive_negated_or_switch_true_default() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_true_negated_or");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$a = $argc > 1;
$b = $argc > 2;
switch (true) {
    case $a || $b:
        echo "A";
        break;
    case !($a || $b):
        echo "B";
        break;
    default:
        echo "dead-default";
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "B");
    assert!(!user_asm.contains("dead-default"));
}

/// Verifies that a multi-pattern case (`$flag` and `!$flag`) followed by `$other` (false) and
/// default prunes both suffixes. Confirms "A".
#[test]
fn test_dead_code_elimination_drops_switch_true_suffix_after_exhaustive_multi_pattern_case() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_true_multi_pattern");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$flag = $argc > 1;
$other = false;
switch (true) {
    case $flag:
    case !$flag:
        echo "A";
        break;
    case $other:
        echo "dead-case";
        break;
    default:
        echo "dead-default";
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "A");
    assert!(!user_asm.contains("dead-case"));
    assert!(!user_asm.contains("dead-default"));
}

/// Verifies that a scalar multi-pattern (case 1, 2) followed by case 3 and default prunes both
/// suffixes when an outer guard fixes `$x === 2`. Confirms "A".
#[test]
fn test_dead_code_elimination_drops_scalar_switch_suffix_after_exhaustive_multi_pattern_case() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_scalar_multi_pattern");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$x = 2;
if ($x === 2) {
    switch ($x) {
        case 1:
        case 2:
            echo "A";
            break;
        case 3:
            echo "dead-case";
            break;
        default:
            echo "dead-default";
    }
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "A");
    assert!(!user_asm.contains("dead-case"));
    assert!(!user_asm.contains("dead-default"));
}
