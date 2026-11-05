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

/// Verifies weak-mode union parameters choose PHP's preferred scalar member after an exact
/// match: nullable string takes a string, `float|string` takes a float, `bool|string` takes a
/// string, and `int|string` keeps the exact integer. Runtime values exercise EIR conversion
/// rather than AST literal folding.
#[test]
fn test_scalars_bind_to_union_parameters() {
    let out = compile_and_run(
        r#"<?php
        function nullableString(?string $value) { var_dump($value); }
        function floatOrString(float|string $value) { var_dump($value); }
        function boolOrString(bool|string $value) { var_dump($value); }
        function intOrString(int|string $value) { var_dump($value); }
        $n = intdiv(40 + $argc, 1);
        nullableString($n);
        floatOrString($n);
        boolOrString($n);
        intOrString($n);
        "#,
    );
    assert_eq!(
        out,
        "string(2) \"41\"\nfloat(41)\nstring(2) \"41\"\nint(41)\n"
    );
}

/// A boxed `Mixed` source (an untyped parameter) still selects the right union member at
/// runtime, including array/string/null unions and the catchable `TypeError` fallback.
#[test]
fn test_mixed_source_binds_to_union_parameters_at_runtime() {
    let out = compile_and_run(
        r#"<?php
        function nullableString(?string $value) { var_dump($value); }
        function arrayStringNull(array|string|null $value) { var_dump($value); }
        function forwardNullable($value) { nullableString($value); }
        function forwardArray($value) { arrayStringNull($value); }
        class Box { public mixed $value = 5; }
        forwardNullable(5);
        forwardNullable(true);
        forwardNullable((new Box)->value);
        forwardArray(5);
        forwardArray([1]);
        forwardArray(null);
        try { forwardNullable([]); } catch (TypeError $e) { echo $e::class, "\n"; }
        try { forwardArray(new stdClass); } catch (TypeError $e) { echo $e::class, "\n"; }
        "#,
    );
    assert_eq!(
        out,
        "string(1) \"5\"\nstring(1) \"1\"\nstring(1) \"5\"\nstring(1) \"5\"\narray(1) {\n  [0]=>\n  int(1)\n}\nNULL\nTypeError\nTypeError\n"
    );
}

/// Strict mode keeps exact union members and PHP's sole `int`→`float` widening, including a
/// union that contains `float` and `string`.
#[test]
fn test_strict_union_parameters_keep_exact_members_and_int_to_float() {
    let out = compile_and_run(
        r#"<?php declare(strict_types=1);
        function intOrString(int|string $value) { var_dump($value); }
        function floatOrString(float|string $value) { var_dump($value); }
        intOrString(5);
        floatOrString(5);
        "#,
    );
    assert_eq!(out, "int(5)\nfloat(5)\n");
}

/// Verifies the union binding also runs on the named-argument path.
#[test]
fn test_scalars_bind_to_union_named_arguments() {
    let out = compile_and_run(
        r#"<?php
        function nullableString(?string $value) { var_dump($value); }
        function floatOrString(float|string $value) { var_dump($value); }
        nullableString(value: 5);
        floatOrString(value: 5);
        "#,
    );
    assert_eq!(out, "string(1) \"5\"\nfloat(5)\n");
}
