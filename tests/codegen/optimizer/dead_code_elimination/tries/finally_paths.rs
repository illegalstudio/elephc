//! Purpose:
//! Integration or regression tests for optimizer-sensitive codegen coverage of optimizer, dead-code elimination, tries finally paths, including dead code elimination prunes after try finally exit, dead code elimination invalidates outer guard before finally body, and dead code elimination preserves outer guard for finally when only other locals change.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled and run so folding, propagation, or pruning stays behavior-preserving.

use super::*;

/// Verifies that code after a `try/finally` with a return in the try is pruned.
/// Confirms "17" and that `pow` does not appear in user assembly.
#[test]
fn test_dead_code_elimination_prunes_after_try_finally_exit() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_try_finally");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function answer() {
    try {
        return 7;
    } finally {
        echo 1;
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
        "dead statements after try/finally exit should not remain in user assembly:\n{}",
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
    assert_eq!(out, "17");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies that a guard invalidated before a finally block is reflected inside the finally.
/// Confirms "a".
#[test]
fn test_dead_code_elimination_invalidates_outer_guard_before_finally_body() {
    let out = compile_and_run(
        r#"<?php
function run($flag) {
    if ($flag) {
        try {
            $flag = false;
        } finally {
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

/// Verifies that a guard is preserved when only other locals change inside the try body.
/// Confirms "a".
#[test]
fn test_dead_code_elimination_preserves_outer_guard_for_finally_when_only_other_locals_change() {
    let out = compile_and_run(
        r#"<?php
function run($flag) {
    if ($flag) {
        try {
            $other = 1;
        } finally {
            if ($flag) {
                echo "a";
            } else {
                echo "bad";
            }
        }
    }
}

run(true);
"#,
    );

    assert_eq!(out, "a");
}

/// Verifies that code after a `try/finally` fallthrough sinks correctly. Confirms "abc".
#[test]
fn test_dead_code_elimination_sinks_tail_into_safe_finally_path() {
    let out = compile_and_run(
        r#"<?php
try {
    echo "a";
} finally {
    echo "b";
}
echo "c";
"#,
    );

    assert_eq!(out, "abc");
}

/// Verifies that an empty `try/finally` inlines to just the finally body. Confirms "f!".
#[test]
fn test_dead_code_elimination_inlines_empty_try_finally() {
    let out = compile_and_run(
        r#"<?php
try {
} finally {
    echo "f";
}
echo "!";
"#,
    );

    assert_eq!(out, "f!");
}

/// Verifies that a nested try/catch inside a finally collapses to just the catch body with
/// finally appended. Confirms "79".
#[test]
fn test_dead_code_elimination_folds_outer_finally_into_single_inner_try() {
    let out = compile_and_run(
        r#"<?php
class A extends Exception {}
function boom() {
    throw new A("a");
}
try {
    try {
        boom();
    } catch (A $e) {
        echo 7;
    }
} finally {
    echo 9;
}
"#,
    );

    assert_eq!(out, "79");
}

/// Verifies that a non-throwing try/finally fallthrough inlines correctly. Confirms "ab".
#[test]
fn test_dead_code_elimination_inlines_non_throwing_try_finally_fallthrough() {
    let out = compile_and_run(
        r#"<?php
try {
    echo "a";
} finally {
    echo "b";
}
"#,
    );

    assert_eq!(out, "ab");
}
