//! Purpose:
//! Regression coverage for initialization facts on switch dispatch edges.
//!
//! Called from:
//! - `tests/codegen/control_flow.rs` through the codegen test harness.
//!
//! Key details:
//! - Later case-label assignments must not appear in earlier case-body scopes.
//! - Runtime parameters preserve source-ordered dynamic dispatch through optimization.

use super::*;

/// A single dispatch edge restores its own initialized locals, not the final label's locals.
#[test]
fn test_switch_initialization_single_dispatch_edge() {
    let out = compile_and_run(
        r#"<?php
function inspect_single(int $n): void {
    switch (true) {
        case $n === 1:
            echo array_key_exists("x", get_defined_vars()) ? "bad|" : "clean|";
            break;
        case ($x = $n + 6) > 0 && $n === 2:
            echo array_key_exists("x", get_defined_vars()) ? "set|" : "bad|";
            break;
        default:
            echo array_key_exists("x", get_defined_vars()) ? "default-set|" : "bad|";
    }
}
inspect_single(1);
inspect_single(2);
inspect_single(3);
"#,
    );
    assert_eq!(out, "clean|set|default-set|");
}

/// Multiple dispatch edges and fallthrough join initialization facts before building a scope.
#[test]
fn test_switch_initialization_joined_dispatch_edges() {
    let out = compile_and_run(
        r#"<?php
function inspect_joined(int $n): void {
    switch ($n) {
        case 1:
        case 2:
            echo array_key_exists("x", get_defined_vars()) ? "bad|" : "clean|";
            break;
        case ($x = 7) - 4:
            echo array_key_exists("x", get_defined_vars()) ? "set|" : "bad|";
            break;
        default:
            echo array_key_exists("x", get_defined_vars()) ? "default-set|" : "bad|";
    }
}
inspect_joined(1);
inspect_joined(2);
inspect_joined(3);
inspect_joined(4);
"#,
    );
    assert_eq!(out, "clean|clean|set|default-set|");
}
