//! Purpose:
//! Integration or regression tests for optimizer-sensitive codegen coverage of optimizer, dead-code elimination, switches guarded switch cases, including dead code elimination prunes nested if region from switch boolean guard case, dead code elimination drops impossible switch cases from outer guards, and dead code elimination drops exhaustive switch true default from cumulative guards.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled and run so folding, propagation, or pruning stays behavior-preserving.

use super::*;

/// Verifies that a nested `if` region contradicting a switch boolean guard is pruned.
/// Confirms "ab".
#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_switch_bool_guard_case() {
    let out = compile_and_run(
        r#"<?php
function run($flag) {
    switch (true) {
        case $flag === true:
            if ($flag === false) {
                echo "bad";
            } else {
                echo "a";
            }
            break;
        default:
            echo "b";
    }
}

run(true);
run(false);
"#,
    );

    assert_eq!(out, "ab");
}

/// Verifies that impossible switch cases ruled out by outer guards are dropped from assembly.
/// Confirms "ab" with "dead-int" and "dead-bool" absent.
#[test]
fn test_dead_code_elimination_drops_impossible_switch_cases_from_outer_guards() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_guard_prune");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($value, $flag) {
    if ($value === 0) {
        switch ($value) {
            case 1:
                echo "dead-int";
                break;
            case 0:
                echo "a";
                break;
        }
    }

    if ($flag === true) {
        switch (true) {
            case $flag === false:
                echo "dead-bool";
                break;
            case $flag === true:
                echo "b";
                break;
        }
    }
}

run(0, true);
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

    assert_eq!(out, "ab");
    assert!(!user_asm.contains("dead-int"));
    assert!(!user_asm.contains("dead-bool"));
}
/// Verifies dead code elimination drops exhaustive switch true default from cumulative guards.
#[test]
fn test_dead_code_elimination_drops_exhaustive_switch_true_default_from_cumulative_guards() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_true_exhaustive");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$flag = $argc > 1;
switch (true) {
    case $flag:
        echo "A";
        break;
    case !$flag:
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
/// Verifies dead code elimination uses cumulative switch true guards inside case body.
#[test]
fn test_dead_code_elimination_uses_cumulative_switch_true_guards_inside_case_body() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_true_cumulative_body");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($a, $b, $c, $d) {
    if ($d) {
        switch (true) {
            case (($a && $b) || $c) && $d:
                echo "A";
                break;
            case !$c:
                if ($a && $b) {
                    echo "dead-ab";
                } else {
                    echo "B";
                }
                break;
            default:
                echo "dead-default";
        }
    }
}
run(true, true, true, true);
run(false, false, false, true);
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
    assert_eq!(out, "AB");
    assert!(!user_asm.contains("dead-ab"));
    assert!(!user_asm.contains("dead-default"));
}

/// Verifies that an excluded scalar switch case is dropped from assembly when an outer guard
/// rules it out. Confirms "A" with "dead-case" absent.
#[test]
fn test_dead_code_elimination_drops_excluded_scalar_switch_case_from_outer_guard() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_excluded_scalar_case");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($value) {
    if ($value !== 1) {
        switch ($value) {
            case 1:
                echo "dead-case";
                break;
            case 2:
                echo "A";
                break;
            default:
                echo "live-default";
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
    assert!(user_asm.contains("live-default"));
}

/// Verifies that falsy switch cases and default are pruned when the switch subject is truthy
/// and a guard case covers the truthy path. Confirms "A" with dead labels absent.
#[test]
fn test_dead_code_elimination_prunes_truthy_switch_cases_and_default() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_truthy_switch_cases");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($flag) {
    if ($flag) {
        switch ($flag) {
            case false:
                echo "dead-false";
                break;
            case true:
                if ($flag) {
                    echo "A";
                } else {
                    echo "bad";
                }
                break;
            default:
                echo "dead-default";
        }
    }
}

run(true);
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
    assert!(!user_asm.contains("dead-false"));
    assert!(!user_asm.contains("dead-default"));
    assert!(!user_asm.contains("bad"));
}

/// Verifies that a switch boolean guard is invalidated after a local write inside the case body.
/// Confirms "a".
#[test]
fn test_dead_code_elimination_prunes_falsy_scalar_labels_from_truthy_switch_subject() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_truthy_switch_scalar_labels");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($flag, $other) {
    if ($flag) {
        switch ($flag) {
            case 0:
            case "":
                echo "dead-falsy-case";
                break;
            case $other:
            case true:
                echo "A";
                break;
            default:
                echo "dead-default";
        }
    }
}
run(true, false);
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
    assert!(!user_asm.contains("dead-falsy-case"));
    assert!(!user_asm.contains("dead-default"));
}
/// Verifies dead code elimination combines exclusion and truthy switch guards.
#[test]
fn test_dead_code_elimination_combines_exclusion_and_truthy_switch_guards() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_mixed_truthy_exclusion");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($value) {
    if ($value) {
        if ($value !== 1) {
            switch ($value) {
                case 1:
                case 0:
                    echo "dead-mixed-case";
                    break;
                case 2:
                case true:
                    echo "A";
                    break;
                default:
                    echo "dead-default";
            }
        }
    }
}
run(true);
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
    assert!(!user_asm.contains("dead-mixed-case"));
    assert!(!user_asm.contains("dead-default"));
}
/// Verifies dead code elimination invalidates switch bool guard after local write.
#[test]
fn test_dead_code_elimination_invalidates_switch_bool_guard_after_local_write() {
    let out = compile_and_run(
        r#"<?php
function run($flag) {
    switch (true) {
        case $flag === true:
            $flag = false;
            if ($flag === true) {
                echo "bad";
            } else {
                echo "a";
            }
            break;
    }
}

run(true);
"#,
    );

    assert_eq!(out, "a");
}
