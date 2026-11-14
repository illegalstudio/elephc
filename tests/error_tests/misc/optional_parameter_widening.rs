//! Purpose:
//! Diagnostics for override and interface shapes that only look like optional-parameter
//! widening: PHP rejects each of them, and so must elephc.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Appending trailing optional parameters is accepted (see the codegen suite
//!   `oop/optional_parameter_widening.rs`); every inherited position must still match exactly.
//! - Static overrides and appended variadics stay rejected: elephc does not bridge them yet.

use super::*;

/// Removing an inherited parameter is still an incompatible override.
#[test]
fn test_error_widening_override_cannot_remove_a_parameter() {
    expect_error(
        "<?php class P { public function f($a, $b) {} } class C extends P { public function f($a) {} }",
        "Cannot change parameter count when overriding method: C::f",
    );
}

/// Appending a REQUIRED parameter is not a widening: a caller through the parent omits it.
#[test]
fn test_error_widening_override_cannot_append_a_required_parameter() {
    expect_error(
        "<?php class P { public function f($a) {} } class C extends P { public function f($a, $b) {} }",
        "Cannot change parameter count when overriding method: C::f",
    );
}

/// Making an inherited optional parameter required is rejected even with an appended tail.
#[test]
fn test_error_widening_override_cannot_make_an_optional_parameter_required() {
    expect_error(
        "<?php class P { public function f($a = 1) {} } class C extends P { public function f($a, $b = 2) {} }",
        "Cannot change optional parameter layout when overriding method: C::f",
    );
}

/// Changing an inherited parameter's by-reference mode is rejected in both directions.
#[test]
fn test_error_widening_override_cannot_change_by_reference_mode() {
    expect_error(
        "<?php class P { public function f($a) {} } class C extends P { public function f(&$a, $b = 1) {} }",
        "Cannot change pass-by-reference parameters when overriding method: C::f",
    );
    expect_error(
        "<?php class P { public function f(&$a) {} } class C extends P { public function f($a, $b = 1) {} }",
        "Cannot change pass-by-reference parameters when overriding method: C::f",
    );
}

/// The appended tail does not relax the inherited parameters' type checks.
#[test]
fn test_error_widening_override_cannot_narrow_an_inherited_untyped_parameter() {
    expect_error(
        "<?php class P { public function f($a) {} } class C extends P { public function f(int $a, $b = 1) {} }",
        "Cannot narrow untyped parameter $a when overriding method: C::f",
    );
}

/// Interface implementations and redeclarations follow the same rules.
#[test]
fn test_error_widening_interface_shapes_stay_strict() {
    expect_error(
        "<?php interface I { public function f($a); } class C implements I { public function f($a, $b) {} }",
        "Cannot change parameter count when implementing interface method: C::f",
    );
    expect_error(
        "<?php interface I { public function f($a); } interface J extends I { public function f(&$a, $b = 2); }",
        "Cannot change pass-by-reference parameters when redeclaring interface method: J::f",
    );
}

/// A narrow caller fills its own omitted defaults before the adapter forwards them, so a
/// widened body using `func_num_args()` behind an optional inherited prefix would count those
/// defaults as passed; it keeps the exact-shape `func_args` diagnostic instead.
#[test]
fn test_error_widening_counting_override_behind_optional_prefix() {
    expect_error(
        "<?php class P { public function f($a = 1) {} } class C extends P { public function f($a = 1, $b = 2) { return func_num_args(); } }",
        "func_num_args()/func_get_args()/func_get_arg() are not supported in C::f when overriding method",
    );
    expect_error(
        "<?php interface I { public function f($a = 1); } class C implements I { public function f($a = 1, $b = 2) { return func_num_args(); } }",
        "func_num_args()/func_get_args()/func_get_arg() are not supported in C::f when implementing interface method",
    );
}

/// Static overrides keep their exact shape: late static binding has no adapter slot yet.
#[test]
fn test_error_widening_static_override_is_not_supported() {
    expect_error(
        "<?php class P { public static function f($a) {} } class C extends P { public static function f($a, $b = 1) {} }",
        "Cannot change parameter count when overriding static method: C::f",
    );
}
