//! Purpose:
//! Integration or regression tests for optimizer-sensitive codegen coverage of optimizer, dead-code elimination tail sinking, including dead code elimination preserves effectful empty if condition, dead code elimination reduces empty if chain to needed condition checks, and dead code elimination sinks tail into if fallthrough branch.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled and run so folding, propagation, or pruning stays behavior-preserving.

use super::*;

/// Verifies that effectful (observable side-effect) calls in an empty `if` condition
/// are preserved and executed. Confirms "t!".
#[test]
fn test_dead_code_elimination_preserves_effectful_empty_if_condition() {
    let out = compile_and_run(
        r#"<?php
function poke() {
    echo "t";
    return true;
}
if (poke()) {
}
echo "!";
"#,
    );

    assert_eq!(out, "t!");
}

/// Verifies that an empty `if/elseif` chain where both branches are pruned still executes
/// the condition effects. Confirms "ab!" where both `poke()` and `tap()` run.
#[test]
fn test_dead_code_elimination_reduces_empty_if_chain_to_needed_condition_checks() {
    let out = compile_and_run(
        r#"<?php
function poke() {
    echo "a";
    return false;
}

function tap() {
    echo "b";
    return false;
}

if (poke()) {
    strlen("abc");
} elseif (tap()) {
    strlen("def");
}

echo "!";
"#,
    );

    assert_eq!(out, "ab!");
}

/// Verifies that code after an `if/else` with a `return` in one branch sinks into the
/// fallthrough branch. Confirms "abc!" (true→"a" return; false→"b" fallthrough + "c").
#[test]
fn test_dead_code_elimination_sinks_tail_into_if_fallthrough_branch() {
    let out = compile_and_run(
        r#"<?php
function run(bool $flag) {
    if ($flag) {
        echo "a";
        return;
    } else {
        echo "b";
    }
    echo "c";
}

run(true);
run(false);
echo "!";
"#,
    );

    assert_eq!(out, "abc!");
}

/// Verifies that equivalent `if/else` bodies joined by short-circuit guards are merged and
/// dead branches dropped. Confirms "aX|cdY" where both paths with `true` guards produce "X".
#[test]
fn test_dead_code_elimination_merges_identical_if_chain_tail_with_short_circuit() {
    let out = compile_and_run(
        r#"<?php
function step($label, $ret) {
    echo $label;
    return $ret;
}
if (step("a", true)) {
    echo "X";
} else {
    if (step("b", true)) {
        echo "Y";
    } else {
        echo "X";
    }
}
echo "|";
if (step("c", false)) {
    echo "X";
} else {
    if (step("d", true)) {
        echo "Y";
    } else {
        echo "X";
    }
}
"#,
    );

    assert_eq!(out, "aX|cdY");
}
