//! Purpose:
//! Integration or regression tests for optimizer-sensitive codegen coverage of optimizer, dead-code elimination, tries try pruning, including dead code elimination prunes after exhaustive try catch, dead code elimination collapses empty try shell after branch dce, and dead code elimination keeps unknown truthy switch entry before matching case.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled and run so folding, propagation, or pruning stays behavior-preserving.

use super::*;

/// Verifies that code after a `try/catch` that always returns is pruned. Confirms "7" and
/// that `pow` (2**8) does not appear in user assembly.
#[test]
fn test_dead_code_elimination_prunes_after_exhaustive_try_catch() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_try_catch");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function answer() {
    try {
        return 7;
    } catch (Exception $e) {
        return 8;
    }
    echo 2 ** 8;
}
echo answer();
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "dead statements after exhaustive try/catch should not remain in user assembly:\n{}",
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
    assert_eq!(out, "7");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies that an empty `try/catch/finally` with pure bodies in try and catch collapses
/// to just the finally block. Confirms "f!".
#[test]
fn test_dead_code_elimination_collapses_empty_try_shell_after_branch_dce() {
    let out = compile_and_run(
        r#"<?php
try {
    strlen("abc");
} catch (Exception $e) {
    strlen("def");
} finally {
    echo "f";
}
echo "!";
"#,
    );

    assert_eq!(out, "f!");
}

/// Verifies that a switch case guarded by `$other` (unknown at compile time) is kept when
/// preceded by a truthy guard on `$flag`. Confirms "A" with "maybe-first" present and
/// "dead-default" absent from assembly.
#[test]
fn test_dead_code_elimination_keeps_unknown_truthy_switch_entry_before_matching_case() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_truthy_switch_unknown_entry");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($flag, $other) {
    if ($flag) {
        switch ($flag) {
            case $other:
            case false:
                echo "maybe-first";
                break;
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
    assert!(user_asm.contains("maybe-first"));
    assert!(!user_asm.contains("dead-default"));
}

/// Verifies that a guard invalidation inside a try body propagates to the catch guard,
/// pruning the "bad" path. Confirms "a".
#[test]
fn test_dead_code_elimination_invalidates_outer_guard_before_catch_body() {
    let out = compile_and_run(
        r#"<?php
function run($flag) {
    if ($flag) {
        try {
            $flag = false;
            throw new Exception("boom");
        } catch (Exception $e) {
            if ($flag) {
                echo "bad";
            } else {
                echo "a";
            }
        }
    }
}

run(true);
"#,
    );

    assert_eq!(out, "a");
}

/// Verifies that guard invalidation via a switch throw path propagates to the catch guard.
/// Confirms "a".
#[test]
fn test_dead_code_elimination_invalidates_outer_guard_before_catch_body_from_switch_throw_path() {
    let out = compile_and_run(
        r#"<?php
function run($flag, $value) {
    if ($flag) {
        try {
            switch ($value) {
                case 1:
                    $flag = false;
                    throw new Exception("boom");
                default:
                    echo "default";
            }
        } catch (Exception $e) {
            if ($flag) {
                echo "bad";
            } else {
                echo "a";
            }
        }
    }
}

run(true, 1);
"#,
    );

    assert_eq!(out, "a");
}

/// Verifies that an unreachable switch throw path whose guard is already invalidated does
/// not affect the catch guard analysis. Confirms "a" with "dead-switch-unreachable" absent.
#[test]
fn test_dead_code_elimination_ignores_unreachable_switch_throw_path_writes_before_catch_body() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_throw_path_cfg_prune");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($flag, $value) {
    if ($value === 1) {
        if ($flag) {
            try {
                switch ($value) {
                    case 2:
                        $flag = false;
                        throw new Exception("dead-case");
                    case 1:
                        throw new Exception("boom");
                }
            } catch (Exception $e) {
                if ($flag) {
                    echo "a";
                } else {
                    echo "dead-switch-unreachable";
                }
            }
        }
    }
}

run(true, 1);
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

    assert_eq!(out, "a");
    assert!(!user_asm.contains("dead-switch-unreachable"));
}

/// Verifies that a guard is preserved when only other locals change inside the try.
/// Confirms "a".
#[test]
fn test_dead_code_elimination_preserves_outer_guard_for_catch_when_only_non_throw_path_writes() {
    let out = compile_and_run(
        r#"<?php
function run($flag, $other) {
    if ($flag) {
        try {
            if ($other) {
                $flag = false;
            } else {
                throw new Exception("boom");
            }
        } catch (Exception $e) {
            if ($flag) {
                echo "a";
            } else {
                echo "bad";
            }
        }
    }
}

run(true, false);
"#,
    );

    assert_eq!(out, "a");
}
