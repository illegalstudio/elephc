//! Purpose:
//! Integration or regression tests for diagnostic coverage of never, including never function cannot return value, never function cannot return void, and never method cannot return.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

// Verifies a function declared with `never` return type cannot return a value.
// PHP: `function fail(): never { return 42; }`
// Expected: "Function 'fail' declared never must not return"
#[test]
fn test_error_never_function_cannot_return_value() {
    expect_error(
        "<?php function fail(): never { return 42; }",
        "Function 'fail' declared never must not return",
    );
}

// Verifies a function declared with `never` return type cannot return void.
// PHP: `function fail(): never { return; }`
// Expected: "Function 'fail' declared never must not return"
#[test]
fn test_error_never_function_cannot_return_void() {
    expect_error(
        "<?php function fail(): never { return; }",
        "Function 'fail' declared never must not return",
    );
}

// Verifies a method declared with `never` return type cannot return.
// PHP: class method with `public function fail(): never { return; }`
// Expected: "Method 'Failer::fail' declared never must not return"
#[test]
fn test_error_never_method_cannot_return() {
    expect_error(
        "<?php class Failer { public function fail(): never { return; } }",
        "Method 'Failer::fail' declared never must not return",
    );
}

// Verifies `never` is rejected in a union return type.
// PHP: `function fail(): int|never { return 1; }`
// Expected: "never can only be used as a standalone return type"
#[test]
fn test_error_never_rejected_in_union_return_type() {
    expect_error(
        "<?php function fail(): int|never { return 1; }",
        "never can only be used as a standalone return type",
    );
}

// Verifies `never` is rejected in a nullable return type.
// PHP: `function fail(): ?never { throw new \Exception(); }`
// Expected: "never can only be used as a standalone return type"
#[test]
fn test_error_never_rejected_in_nullable_return_type() {
    expect_error(
        "<?php function fail(): ?never { throw new \\Exception(); }",
        "never can only be used as a standalone return type",
    );
}

// Verifies `never` cannot be used as a parameter type.
// PHP: `function take(never $x) {}`
// Expected: "cannot use type never"
#[test]
fn test_error_never_rejected_as_parameter_type() {
    expect_error(
        "<?php function take(never $x) {} take(1);",
        "cannot use type never",
    );
}

// Verifies `never` is rejected in a union parameter type.
// PHP: `function take(int|never $x) {}`
// Expected: "cannot use type never"
#[test]
fn test_error_never_rejected_in_union_parameter_type() {
    expect_error(
        "<?php function take(int|never $x) {} take(1);",
        "cannot use type never",
    );
}

// Verifies `never` cannot be used as a property type.
// PHP: `class Box { public never $value; }`
// Expected: "cannot use type never"
#[test]
fn test_error_never_rejected_as_property_type() {
    expect_error(
        "<?php class Box { public never $value; }",
        "cannot use type never",
    );
}

// Verifies `never` cannot be used as a typed local variable.
// PHP: `never $x = 1;`
// Expected: "cannot use type never"
#[test]
fn test_error_never_rejected_as_typed_local() {
    expect_error(
        "<?php never $x = 1;",
        "cannot use type never",
    );
}

// Verifies a child class cannot widen `never` return type to `void` in override.
// PHP: Parent declares `f(): never`, Child declares `f(): void`
// Expected: "incompatible return type"
#[test]
fn test_error_never_override_widening_to_void_rejected() {
    expect_error(
        "<?php class Parent_ { public function f(): never { throw new \\Exception(); } } class Child_ extends Parent_ { public function f(): void {} }",
        "incompatible return type",
    );
}

// Verifies a child class cannot omit return type when parent declares `never`.
// PHP: Parent declares `f(): never`, Child declares `f()` without return type
// Expected: "without declaring a compatible return type"
#[test]
fn test_error_never_override_requires_declared_return_type() {
    expect_error(
        "<?php class Parent_ { public function f(): never { throw new \\Exception(); } } class Child_ extends Parent_ { public function f() { throw new \\Exception(); } }",
        "without declaring a compatible return type",
    );
}

// Verifies an implementation cannot widen `never` return type to `int` in interface implementation.
// PHP: Interface declares `fail(): never`, class implements with `fail(): int`
// Expected: "incompatible return type"
#[test]
fn test_error_never_interface_implementation_widening_rejected() {
    expect_error(
        "<?php interface Failer { public function fail(): never; } class Bad implements Failer { public function fail(): int { return 1; } }",
        "incompatible return type",
    );
}

// Verifies a class cannot omit return type when interface declares `never`.
// PHP: Interface declares `fail(): never`, class implements without return type
// Expected: "without declaring a compatible return type"
#[test]
fn test_error_never_interface_requires_declared_return_type() {
    expect_error(
        "<?php interface Failer { public function fail(): never; } class Bad implements Failer { public function fail() { throw new \\Exception(); } }",
        "without declaring a compatible return type",
    );
}
