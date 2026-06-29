//! Purpose:
//! Regression tests for M1: dead-code elimination must retain a bare-statement
//! builtin call whose result is unused when the builtin can raise a PHP fatal on
//! bad input. PHP evaluates the statement and throws there, so dropping it would
//! silently skip the fatal and let the program exit cleanly.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - These builtins are otherwise pure; they are modeled as `may_throw` (not
//!   side-effecting), which keeps the bare statement while still allowing pruning
//!   of genuinely unevaluated subexpressions.
//! - `str_repeat` is used as the representative builtin because its runtime fatal
//!   on a negative count is already exercised elsewhere, so the test isolates the
//!   DCE keep/drop decision rather than the builtin's argument validation.

use super::*;

/// Verifies that a bare-statement `str_repeat()` with a negative count is retained
/// by DCE and fatals at runtime. Before M1 the call was classified pure-non-throwing,
/// so DCE dropped the unused statement and the program exited 0 with no fatal.
#[test]
fn test_dead_code_elimination_keeps_bare_throwing_builtin() {
    let err = compile_and_run_expect_failure(r#"<?php str_repeat("ab", -1);"#);
    assert!(
        err.contains("str_repeat") && err.contains("greater than or equal to 0"),
        "bare str_repeat(neg) must fatal (statement retained), got: {err}"
    );
}

/// Verifies the may-throw classification does not break valid calls: a bare
/// `str_repeat()` with a good count runs cleanly and the following statement executes.
#[test]
fn test_dead_code_elimination_valid_bare_may_throw_builtin_runs() {
    let out = compile_and_run(r#"<?php str_repeat("ab", 2); echo "ok";"#);
    assert_eq!(out, "ok");
}

/// Verifies a pure builtin is still pruned after M1: a bare `strlen()` remains
/// eliminable, so a following throwing statement is the one that fatals (i.e. the
/// pure statement did not become a spurious runtime call). The program still fatals
/// on the negative `str_repeat`, confirming `strlen` did not alter control flow.
#[test]
fn test_dead_code_elimination_pure_builtin_still_pruned_alongside_throwing() {
    let err = compile_and_run_expect_failure(r#"<?php strlen("xyz"); str_repeat("ab", -1);"#);
    assert!(
        err.contains("str_repeat"),
        "pure strlen should be pruned and the throwing str_repeat should fatal, got: {err}"
    );
}

/// Verifies that retaining a bare heap-returning may-throw builtin does not leak:
/// the discarded temporary (here `explode()`'s array of strings) must be freed like
/// any other unused owned result. Before M1 the call was eliminated and never
/// allocated; now it is retained and executed, so its temporary must be released.
#[test]
fn test_dead_code_elimination_kept_may_throw_builtin_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$s = "a,b,c";
explode(",", $s);
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "bare explode() temporary must be freed, got: {}",
        out.stderr
    );
}
