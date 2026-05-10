//! Purpose:
//! Integration or regression tests for diagnostic coverage of exception, enum, and magic-constant diagnostics, including magic method contracts collect multiple errors, try requires catch or finally, and throw requires object.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

#[test]
fn test_error_magic_method_contracts_collect_multiple_errors() {
    let error = check_source_full(
        "<?php class A { private function __toString() { return \"x\"; } } class B { public static function __toString() { return \"y\"; } }",
    )
    .unwrap_err();
    let all = error.flatten();
    assert!(
        all.len() >= 2,
        "expected multiple magic method contract errors, got {:?}",
        all.iter().map(|error| error.message.clone()).collect::<Vec<_>>(),
    );
}

#[test]
fn test_error_try_requires_catch_or_finally() {
    expect_error(
        "<?php try { echo 1; }",
        "Expected at least one catch or a finally block after try",
    );
}

#[test]
fn test_error_throw_requires_object() {
    expect_error("<?php throw 123;", "throw requires an object value");
}

#[test]
fn test_error_enum_cannot_be_instantiated() {
    expect_error(
        "<?php enum Color: int { case Red = 1; } $x = new Color();",
        "Cannot instantiate enum: Color",
    );
}

#[test]
fn test_error_backed_enum_case_requires_value() {
    expect_error(
        "<?php enum Color: int { case Red; }",
        "Backed enum cases must declare a value",
    );
}

#[test]
fn test_error_pure_enum_case_cannot_have_backing_value() {
    expect_error(
        "<?php enum Suit { case Hearts = 1; }",
        "Pure enum cases cannot declare a backing value",
    );
}

#[test]
fn test_error_enum_duplicate_backing_value() {
    expect_error(
        "<?php enum Color: int { case Red = 1; case Crimson = 1; }",
        "Duplicate enum backing value",
    );
}

#[test]
fn test_error_pure_enum_has_no_from_method() {
    expect_error(
        "<?php enum Suit { case Hearts; } Suit::from(1);",
        "Undefined method: Suit::from",
    );
}

#[test]
fn test_error_throw_requires_throwable() {
    expect_error(
        "<?php class PlainObject {} throw new PlainObject();",
        "throw requires an object implementing Throwable",
    );
}

#[test]
fn test_error_throw_expression_requires_object() {
    expect_error(
        "<?php $value = null ?? throw 123;",
        "throw requires an object value",
    );
}

#[test]
fn test_error_magic_tostring_must_be_public() {
    expect_error(
        "<?php class User { private function __toString() { return \"x\"; } }",
        "Magic method must be public: User::__toString",
    );
}

#[test]
fn test_error_magic_tostring_must_take_zero_arguments() {
    expect_error(
        "<?php class User { public function __toString($x) { return \"x\"; } }",
        "Magic method must take 0 arguments: User::__toString",
    );
}

#[test]
fn test_error_magic_tostring_must_return_string() {
    expect_error(
        "<?php class User { public function __toString() { return 123; } }",
        "Magic method must return string: User::__toString",
    );
}

#[test]
fn test_error_magic_get_must_take_one_argument() {
    expect_error(
        "<?php class Bag { public function __get() { return 1; } }",
        "Magic method must take 1 argument: Bag::__get",
    );
}

#[test]
fn test_error_magic_set_must_be_public() {
    expect_error(
        "<?php class Bag { private function __set($name, $value) { } }",
        "Magic method must be public: Bag::__set",
    );
}

#[test]
fn test_error_magic_set_must_take_two_arguments() {
    expect_error(
        "<?php class Bag { public function __set($name) { } }",
        "Magic method must take 2 arguments: Bag::__set",
    );
}

#[test]
fn test_error_catch_requires_defined_class() {
    expect_error(
        "<?php try { echo 1; } catch (MissingException $e) { echo 2; }",
        "Undefined class: MissingException",
    );
}

#[test]
fn test_error_catch_requires_throwable_type() {
    expect_error(
        "<?php class PlainObject {} try { throw new Exception(); } catch (PlainObject $e) { echo 2; }",
        "Catch type must extend or implement Throwable: PlainObject",
    );
}

#[test]
fn test_error_cannot_redeclare_builtin_exception_type() {
    expect_error(
        "<?php class Exception {}",
        "Cannot redeclare built-in type: Exception",
    );
}

#[test]
fn test_error_cannot_instantiate_throwable_interface() {
    expect_error(
        "<?php $e = new Throwable();",
        "Cannot instantiate interface: Throwable",
    );
}
