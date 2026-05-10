//! Purpose:
//! Integration or regression tests for optimizer-sensitive codegen coverage of optimizer, dead-code elimination normalization, including dead code elimination collapses identical if branches, dead code elimination merges identical if chain bodies with short circuit, and dead code elimination recursively merges longer if chains.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled and run so folding, propagation, or pruning stays behavior-preserving.

use super::*;

#[test]
fn test_dead_code_elimination_collapses_identical_if_branches() {
    let out = compile_and_run(
        r#"<?php
function step($label, $ret) {
    echo $label;
    return $ret;
}
if (step("c", false)) {
    echo "X";
} else {
    echo "X";
}
"#,
    );

    assert_eq!(out, "cX");
}

#[test]
fn test_dead_code_elimination_merges_identical_if_chain_bodies_with_short_circuit() {
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
        echo "X";
    } else {
        echo "Y";
    }
}
"#,
    );

    assert_eq!(out, "aX");
}

#[test]
fn test_dead_code_elimination_recursively_merges_longer_if_chains() {
    let out = compile_and_run(
        r#"<?php
function step($label, $ret) {
    echo $label;
    return $ret;
}
if (step("a", false)) {
    echo "X";
} else {
    if (step("b", false)) {
        echo "X";
    } else {
        if (step("c", true)) {
            echo "X";
        } else {
            echo "Y";
        }
    }
}
echo "|";
if (step("d", false)) {
    echo "X";
} else {
    if (step("e", false)) {
        echo "Y";
    } else {
        if (step("f", true)) {
            echo "Y";
        } else {
            echo "X";
        }
    }
}
"#,
    );

    assert_eq!(out, "abcX|defY");
}

#[test]
fn test_dead_code_elimination_flattens_nested_single_path_ifs() {
    let out = compile_and_run(
        r#"<?php
$a = true;
$b = true;
if ($a) {
    if ($b) {
        echo 7;
    }
}
"#,
    );

    assert_eq!(out, "7");
}
