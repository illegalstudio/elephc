//! Purpose:
//! End-to-end coverage for PHP's coercive parameter binding on declared user-defined
//! parameters: scalars widening into `string`/`bool` parameters, and compile-time-constant
//! numeric arguments binding to `int`/`float` parameters.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected value is verbatim `LC_ALL=C php` 8.4.20 stdout.
//! - `$argc` keeps an argument runtime-valued so the binding is exercised on a real value
//!   rather than being decided by AST constant folding.
//! - The bindings elephc deliberately refuses (lossy or non-numeric conversions, which PHP
//!   signals with a runtime `Deprecated:` notice or `TypeError`) are pinned in
//!   `tests/error_tests/type_system.rs`.

use crate::support::*;

/// Verifies the parameter-typing audit repro: a float and a numeric string binding to `int`,
/// and an int binding to `string`.
#[test]
fn test_coercive_binding_audit_repro() {
    let out = compile_and_run(
        r#"<?php
        function takesInt(int $i) { return $i; }
        function takesString(string $s) { return $s; }
        echo takesInt(5.0), " ", takesInt("42"), " ", takesString(42);
        "#,
    );
    assert_eq!(out, "5 42 42");
}

/// Verifies every scalar binds to a `string` parameter exactly as PHP's `(string)` cast does,
/// including `false` becoming the empty string and `1.0` losing its fractional part.
#[test]
fn test_scalars_bind_to_string_parameter() {
    let out = compile_and_run(
        r#"<?php
        function fmt(string $s) { return "[" . $s . "]"; }
        echo fmt(42), fmt(4.5), fmt(true), fmt(false), fmt(1.0);
        "#,
    );
    assert_eq!(out, "[42][4.5][1][][1]");
}

/// Verifies every scalar binds to a `bool` parameter using PHP's truthiness, including the
/// `"0"` and `0.0` falsy cases.
#[test]
fn test_scalars_bind_to_bool_parameter() {
    let out = compile_and_run(
        r#"<?php
        function flag(bool $b) { return $b ? "T" : "F"; }
        echo flag(1), flag(0), flag("a"), flag(""), flag("0"), flag(0.0), flag(-0.5);
        "#,
    );
    assert_eq!(out, "TFTFFFT");
}

/// Verifies the `string` binding also fires for runtime values, not only literals. `$argc`
/// keeps each argument opaque to constant folding.
#[test]
fn test_runtime_scalars_bind_to_string_parameter() {
    let out = compile_and_run(
        r#"<?php
        function fmt(string $s) { return "[" . $s . "]"; }
        $n = 40 + $argc;
        $f = 4.5 + $argc;
        $b = $argc > 0;
        echo fmt($n), fmt($f), fmt($b);
        "#,
    );
    assert_eq!(out, "[41][5.5][1]");
}

/// Verifies numeric-string constants bind to `int` and `float` parameters, covering PHP's
/// surrounding-whitespace allowance, exponent spelling, and a negative value.
#[test]
fn test_numeric_string_constants_bind_to_numeric_parameters() {
    let out = compile_and_run(
        r#"<?php
        function takesInt(int $i) { return $i; }
        function takesFloat(float $f) { return $f; }
        echo takesInt(" 42 "), " ", takesFloat("4.5"), " ", takesFloat("1e3"), " ", takesInt("-7");
        "#,
    );
    assert_eq!(out, "42 4.5 1000 -7");
}

/// Verifies the binding covers declared parameters on constructors, instance methods, and
/// static methods, not just plain functions.
#[test]
fn test_coercive_binding_applies_to_methods_and_constructors() {
    let out = compile_and_run(
        r#"<?php
        class Box {
            public function __construct(public string $label) {}
            public function tag(string $s): string { return $this->label . ":" . $s; }
            public static function of(int $n): string { return "n=" . $n; }
        }
        echo (new Box(1.5))->tag(42), " ", Box::of(7.0);
        "#,
    );
    assert_eq!(out, "1.5:42 n=7");
}

/// Verifies the binding fires for named arguments, which reach EIR through the reordered
/// named-argument path rather than the positional one.
#[test]
fn test_coercive_binding_applies_to_named_arguments() {
    let out = compile_and_run(
        r#"<?php
        function pair(string $a, string $b) { return $a . "|" . $b; }
        echo pair(b: 7, a: 4.5);
        "#,
    );
    assert_eq!(out, "4.5|7");
}
