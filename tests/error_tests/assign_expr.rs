//! Purpose:
//! Diagnostic coverage for assignments used as operands and call-result dimension writes: the
//! shapes PHP itself refuses at compile time and the ones elephc refuses instead of guessing.
//!
//! Called from:
//! - `cargo test` through Rust's test harness (`tests/error_tests.rs`).
//!
//! Key details:
//! - PHP 8.5 reports `Can't use nullsafe operator in write context` at compile time for every
//!   write through `?->`, in statement and operand position alike.

use super::*;

/// Verifies a nullsafe property write is PHP's compile-time error, as a statement.
#[test]
fn test_error_nullsafe_property_write_statement() {
    expect_error(
        "<?php class O { public $p; } $o = new O; $o?->p = 1;",
        "Can't use nullsafe operator in write context",
    );
}

/// Verifies a nullsafe property write is PHP's compile-time error as an operator operand.
#[test]
fn test_error_nullsafe_property_write_operand() {
    expect_error(
        "<?php class O { public $p; } $o = new O; false || $o?->p = 1;",
        "Can't use nullsafe operator in write context",
    );
}

/// Verifies a nullsafe chain ending in an append is refused too.
#[test]
fn test_error_nullsafe_append_operand() {
    expect_error(
        "<?php class O { public $a = []; } $o = new O; $x = true && $o?->a[] = 1;",
        "Can't use nullsafe operator in write context",
    );
}

/// Verifies a compound update of a call-result dimension is refused clearly (statement).
#[test]
fn test_error_compound_call_dimension_statement() {
    expect_error(
        "<?php function f() { return [1]; } f()[0] += 1;",
        "Compound assignment to a dimension of a call result",
    );
}

/// Verifies a compound update of a call-result dimension is refused clearly (expression).
#[test]
fn test_error_compound_call_dimension_expression() {
    expect_error(
        "<?php function f() { return ['a']; } $x = f()[0] .= 'b';",
        "Compound assignment to a dimension of a call result",
    );
}

/// Verifies a reference assignment operand needs a plain-variable target.
#[test]
fn test_error_reference_assignment_operand_property_target() {
    expect_error(
        "<?php class O { public $p; } $o = new O; $y = 1; $x = true || $o->p = &$y;",
        "Reference assignment inside an expression is only supported for a plain variable target",
    );
}

/// Verifies an operand append through a dynamic property name is refused clearly.
#[test]
fn test_error_dynamic_property_append_operand() {
    expect_error(
        "<?php $o = new stdClass; $p = 'q'; $x = ($o->$p[] = 5);",
        "only supported on a variable, property, static property or call result",
    );
}

/// Verifies a `??=` update of a call-result dimension is refused clearly (it would need
/// `offsetGet` on the temporary).
#[test]
fn test_error_null_coalesce_assign_call_dimension() {
    expect_error(
        "<?php function mkArr(): array { return [1, 2]; } mkArr()[2] ??= 5;",
        "Compound assignment to a dimension of a call result",
    );
}

// Capturing a possibly undefined local (`use ($r)`, `use (&$r)`, an arrow function) compiles
// with PHP's semantics now: see
// `codegen::assign_expr::test_maybe_undefined_closure_captures`.
