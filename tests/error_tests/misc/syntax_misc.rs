//! Purpose:
//! Integration or regression tests for diagnostic coverage of misc, including bitwise compound assignment requires ints, duplicate use alias is rejected, and has line number.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

/// Tests that `&=` compound assignment rejects a string left-hand operand.
/// The error message is "Bitwise operators require integer operands".
#[test]
fn test_error_bitwise_compound_assignment_requires_ints() {
    expect_error(
        "<?php $x = \"flags\"; $x &= 1;",
        "Bitwise operators require integer operands",
    );
}

/// Tests that `declare()` with no directive is rejected with a clear diagnostic.
#[test]
fn test_error_declare_requires_directive_name() {
    expect_error(
        "<?php declare();",
        "Expected a directive name in 'declare(...)'",
    );
}

/// Tests that declare values cannot be variables because PHP requires literals.
#[test]
fn test_error_declare_value_must_be_literal() {
    expect_error(
        "<?php declare(ticks=$ticks);",
        "declare(ticks) value must be a literal",
    );
}

/// Tests that a compound expression is not accepted as a declare literal.
#[test]
fn test_error_declare_rejects_literal_expression() {
    expect_error(
        "<?php declare(ticks=1 + 0);",
        "declare(ticks) value must be a literal",
    );
}

/// Tests that callable expressions are rejected instead of being parsed and silently discarded.
#[test]
fn test_error_declare_rejects_call_value() {
    expect_error(
        "<?php declare(ticks=side_effect());",
        "declare(ticks) value must be a literal",
    );
}

/// Tests that strict_types accepts only the integer literals zero and one.
#[test]
fn test_error_declare_strict_types_requires_zero_or_one() {
    expect_error(
        "<?php declare(strict_types=2);",
        "strict_types declaration must have 0 or 1 as its value",
    );
}

/// Tests that strict_types must precede every executable or declaration statement.
#[test]
fn test_error_declare_strict_types_must_be_first() {
    expect_error(
        "<?php echo 1; declare(strict_types=1);",
        "strict_types declaration must be the very first statement in the script",
    );
}

/// Tests that strict_types cannot control a braced body.
#[test]
fn test_error_declare_strict_types_rejects_block_mode() {
    expect_error(
        "<?php declare(strict_types=1) { echo 1; }",
        "strict_types declaration must not use block mode",
    );
}

/// Tests that alternative declare syntax requires its `enddeclare` terminator.
#[test]
fn test_error_declare_alternative_syntax_requires_enddeclare() {
    expect_error(
        "<?php declare(ticks=1): echo 1;",
        "Expected 'enddeclare' after declare block",
    );
}

/// Tests that direct reference assignment rejects a non-variable source.
#[test]
fn test_error_reference_assignment_requires_variable_source() {
    expect_error(
        "<?php $a = 1; $b =& 1;",
        "Reference assignment source must be a variable",
    );
}

/// Tests that a reference assignment rejects a computed (non-lvalue) source such as
/// `$a + 1`, which is neither a variable, an array/property element, nor a call.
#[test]
fn test_error_reference_assignment_rejects_computed_source() {
    expect_error(
        "<?php $a = 1; $b =& $a + 1;",
        "Reference assignment source must be a variable",
    );
}

/// Tests that two `use` statements with the same alias name produce a
/// "Duplicate import alias" error.
#[test]
fn test_error_duplicate_use_alias_is_rejected() {
    expect_error(
        "<?php namespace App; use Lib\\One as Tool; use Lib\\Two as Tool; echo 1;",
        "Duplicate import alias: Tool",
    );
}

/// Verifies that lexer errors report the correct line number in the span.
/// The input has two newlines before the unterminated string, so the error
/// should be on line 3.
#[test]
fn test_error_has_line_number() {
    let result = tokenize("<?php\n\n\"unterminated");
    let err = result.unwrap_err();
    assert_eq!(err.span.line, 3, "Error should be on line 3");
}

/// Verifies that lexer errors carry a column number greater than zero.
#[test]
fn test_error_has_column() {
    let result = tokenize("<?php `");
    let err = result.unwrap_err();
    assert!(err.span.col > 0, "Error should have a column number");
}

/// Tests that `gettype()` with no arguments produces the expected arity error.
#[test]
fn test_error_gettype_wrong_args() {
    expect_error("<?php gettype();", "gettype() takes exactly 1 argument");
}

/// Tests that `empty()` with no arguments produces the expected arity error.
#[test]
fn test_error_empty_wrong_args() {
    expect_error("<?php empty();", "empty() takes exactly 1 argument");
}

/// Tests that `unset()` with no arguments produces the expected arity error.
#[test]
fn test_error_unset_wrong_args() {
    expect_error("<?php unset();", "unset() takes at least 1 argument");
}

/// Tests that `settype()` with only one argument produces the expected arity error.
#[test]
fn test_error_settype_wrong_args() {
    expect_error("<?php settype(42);", "settype() takes exactly 2 arguments");
}

/// Tests that `&` with a string left-hand operand rejects it with the
/// "Bitwise operators require integer operands" error.
#[test]
fn test_error_bitwise_and_string() {
    expect_error(
        r#"<?php echo "hello" & 1;"#,
        "Bitwise operators require integer operands",
    );
}

/// Tests that unary `~` on a string rejects it with the
/// "Bitwise NOT requires integer operand" error.
#[test]
fn test_error_bitwise_not_string() {
    expect_error(
        r#"<?php echo ~"hello";"#,
        "Bitwise NOT requires integer operand",
    );
}

/// Tests that the spaceship operator `<=>` with string operands rejects them
/// with the "Spaceship operator requires numeric operands" error.
#[test]
fn test_error_spaceship_string() {
    expect_error(
        r#"<?php echo "a" <=> "b";"#,
        "Spaceship operator requires numeric operands",
    );
}

/// Tests that using `$this` inside a `static` method produces the expected
/// "Cannot use $this inside a static method" error.
#[test]
fn test_error_static_this() {
    expect_error(
        "<?php class Demo { public static function bad() { return $this; } } Demo::bad();",
        "Cannot use $this inside a static method",
    );
}

/// Tests that a child class method that changes the parameter count when
/// overriding a parent method produces the expected error.
#[test]
fn test_error_override_cannot_change_parameter_count() {
    expect_error(
        "<?php class Base { public function ping($x) { return $x; } } class Child extends Base { public function ping() { return 1; } }",
        "Cannot change parameter count when overriding method: Child::ping",
    );
}

/// Tests that a hex literal with no digits after `0x` produces the expected
/// "Expected hex digits after '0x'" error.
#[test]
fn test_error_hex_no_digits() {
    expect_error("<?php echo 0x;", "Expected hex digits after '0x'");
}

// --- Mixed return type errors ---

// Note: mixed return types are now widened (Str > Float > Int) instead of
// producing an error. The test_return_type_mixed_branches codegen test
// covers the widening behavior.

// --- Math trig/log error tests ---

/// Tests that `is_null()` with no arguments produces the expected arity error.
#[test]
fn test_error_is_null_wrong_args() {
    expect_error("<?php is_null();", "is_null() takes exactly 1 argument");
}

/// Tests that reassigning a nullable typed local variable (`?int`) with a
/// string produces a "cannot reassign $value" error.
#[test]
fn test_error_nullable_typed_local_rejects_invalid_reassignment() {
    expect_error(
        "<?php ?int $value = null; $value = \"x\";",
        "cannot reassign $value",
    );
}

/// Tests that `require` with a variable as the path produces a
/// "compile-time-constant string" error.
#[test]
fn test_include_path_with_variable_errors() {
    let err = resolver_error("<?php $path = 'x'; require $path;");
    assert!(
        err.message.contains("compile-time-constant string"),
        "message did not mention compile-time-constant: {}",
        err.message
    );
}

/// Tests that `require` with a function call as the path produces a
/// "compile-time-constant string" error.
#[test]
fn test_include_path_with_function_call_errors() {
    let err = resolver_error("<?php require getenv('PATH');");
    assert!(
        err.message.contains("compile-time-constant string"),
        "message did not mention compile-time-constant: {}",
        err.message
    );
}

// --- Static closures ---
