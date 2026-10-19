//! Purpose:
//! Diagnostic coverage for runtime-named static properties (`self::${$p}`): the forms
//! elephc refuses at compile time instead of guessing, and malformed syntax.
//!
//! Called from:
//! - `cargo test` through Rust's test harness (`tests/error_tests.rs`).
//!
//! Key details:
//! - Supported forms are covered end to end in `tests/codegen/static_member.rs`.

use super::*;

/// Verifies an assignment to a runtime-named static property inside an expression is refused.
#[test]
fn test_dynamic_static_property_assignment_in_expression_is_refused() {
    expect_error(
        "<?php class C { static $a = []; static function f($p) { $x = (self::${$p} = 1); } }",
        "Assigning to a dynamic static property",
    );
}

/// Verifies `self::${$p}` outside a class is refused.
#[test]
fn test_dynamic_static_property_outside_class_is_refused() {
    expect_error(
        "<?php function f($p) { return self::${$p}; } echo f('a');",
        "outside a class is not supported",
    );
}

/// Verifies a runtime name that itself reads a runtime-named static property is refused.
#[test]
fn test_nested_dynamic_static_property_name_is_refused() {
    expect_error(
        "<?php class C { static $a = []; static function f($p) { return self::${self::${$p}}; } } C::f('a');",
        "cannot itself read a dynamic static property",
    );
}

/// Verifies a class whose declaration the compiler cannot see is refused.
#[test]
fn test_dynamic_static_property_on_unknown_class_is_refused() {
    expect_error(
        "<?php class C { static function f($p) { return Nope::${$p}; } } C::f('a');",
        "whose declaration is not visible to elephc",
    );
}

/// Verifies `self::$` without a name is a syntax error.
#[test]
fn test_dynamic_static_property_without_name_is_syntax_error() {
    expect_error(
        "<?php class C { static $a; static function f() { return self::$; } }",
        "Expected variable name after '$'",
    );
}

/// Verifies `[]` cannot be read through a runtime-named static property.
#[test]
fn test_dynamic_static_property_append_read_is_refused() {
    expect_error(
        "<?php class C { static $a = []; static function f($p) { echo self::${$p}[]; } }",
        "Cannot use [] for reading",
    );
}

/// Verifies a `mixed` value assigned to a typed static property is still refused at compile time
/// (documented gap: PHP checks it at run time; Carbon `Language::$languagesNames`).
#[test]
fn test_mixed_value_into_typed_static_property_is_refused() {
    expect_error(
        "<?php class L { protected static ?array $n = null; static function f() { static::$n ??= json_decode('[1]', true); return static::$n; } } L::f();",
        "Static property L::$n expects",
    );
}
