//! Purpose:
//! Regression tests for the classes/reflection/closures review findings of the final round.
//!
//! Called from:
//! - `cargo test` through the codegen integration-test harness.
//!
//! Key details:
//! - The expected strings are PHP 8.5's output for the same sources.

use crate::support::*;

/// A class declared once inside a function body is usable after the function ran.
#[test]
fn test_class_declared_inside_function_body() {
    let out = compile_and_run(
        r#"<?php
function f(){ class Loc { function hi(){ return "hi"; } } return new Loc; }
echo f()->hi(), "\n";
echo (new Loc)->hi(), "\n";
"#,
    );
    assert_eq!(out, "hi\nhi\n");
}

/// The compiler's `__Elephc*` helper classes are not listed by `get_declared_classes()`.
#[test]
fn test_get_declared_classes_hides_helper_classes() {
    let out = compile_and_run(
        r#"<?php
class Q { static $s = 1; }
echo count(array_filter(get_declared_classes(), fn($c) => str_starts_with(strtolower($c), '__elephc'))), "\n";
"#,
    );
    assert_eq!(out, "0\n");
}
