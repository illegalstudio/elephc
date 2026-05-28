//! Purpose:
//! Integration or regression tests for optimizer-sensitive codegen coverage of optimizer, dead-code elimination guards, including dead code elimination prunes nested if region from excluded zero guard, dead code elimination prunes nested if region from excluded null guard, and dead code elimination prunes nested if region from excluded empty string guard.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled and run so folding, propagation, or pruning stays behavior-preserving.

use super::*;

/// Verifies that a nested `if` contradicting an outer `=== 0` exclusion guard is pruned.
/// Confirms "ab".
#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_excluded_zero_guard() {
    let out = compile_and_run(
        r#"<?php
function run($value) {
    if ($value === 0) {
        echo "b";
    } else {
        if ($value === 0) {
            echo "bad";
        } else {
            echo "a";
        }
    }
}

run(1);
run(0);
"#,
    );

    assert_eq!(out, "ab");
}

/// Verifies that a nested `if` contradicting an outer `!== null` exclusion guard is pruned.
/// Tests both the non-null case and the null case. Confirms "ab".
#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_excluded_null_guard() {
    let out = compile_and_run(
        r#"<?php
function runNotNull() {
    $value = 1;
    if ($value !== null) {
        if ($value === null) {
            echo "bad";
        } else {
            echo "a";
        }
    } else {
        echo "b";
    }
}

function runNull() {
    $value = null;
    if ($value !== null) {
        echo "bad";
    } else {
        echo "b";
    }
}

runNotNull();
runNull();
"#,
    );

    assert_eq!(out, "ab");
}

/// Verifies that a nested `if` contradicting an outer `=== ""` exclusion guard is pruned.
/// Confirms "ab".
#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_excluded_empty_string_guard() {
    let out = compile_and_run(
        r#"<?php
function run($value) {
    if ($value === "") {
        echo "b";
    } else {
        if ($value === "") {
            echo "bad";
        } else {
            echo "a";
        }
    }
}

run("x");
run("");
"#,
    );

    assert_eq!(out, "ab");
}

/// Verifies that a nested `if` contradicting an outer `=== "0"` exclusion guard is pruned.
/// Confirms "ab".
#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_excluded_string_zero_guard() {
    let out = compile_and_run(
        r#"<?php
function run($value) {
    if ($value === "0") {
        echo "b";
    } else {
        if ($value === "0") {
            echo "bad";
        } else {
            echo "a";
        }
    }
}

run("1");
run("0");
"#,
    );

    assert_eq!(out, "ab");
}

/// Verifies that a nested `if` contradicting an outer `=== 1.5` exclusion guard is pruned.
/// Confirms "ab".
#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_excluded_float_guard() {
    let out = compile_and_run(
        r#"<?php
function run($value) {
    if ($value === 1.5) {
        echo "b";
    } else {
        if ($value === 1.5) {
            echo "bad";
        } else {
            echo "a";
        }
    }
}

run(2.5);
run(1.5);
"#,
    );

    assert_eq!(out, "ab");
}
