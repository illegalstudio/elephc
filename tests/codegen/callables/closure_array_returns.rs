//! Purpose:
//! Regression tests for closures and arrow functions that return an array literal built
//! directly out of their own parameters or captured variables.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected value is verbatim `LC_ALL=C php` output from PHP 8.4.20.
//! - A closure with no declared return type infers one from a single `return <expr>;` body
//!   (`direct_closure_return_type` in `src/ir_lower/function.rs`). That inference used to fall
//!   back to the syntactic `int` default for an array literal, so
//!   `function (mixed $a, mixed $b) { return [$a, $b]; }` was stamped `array<int>` and
//!   `$f(1, "z")` returned `[1, 0]` — the boxed `Mixed` argument was cast to an integer on the
//!   way into the array. Typed `string`/`float`/`bool`/`array` parameters were mis-stamped the
//!   same way.
//! - The shapes that always worked (named function, method, literal assigned to a local before
//!   returning, explicit `: array`) are pinned here too so the fix keeps them working.
//! - `$argc` seeds the by-value capture fixture so constant propagation cannot fold the captured
//!   string into the literal and bypass the capture path entirely.

use crate::support::*;

/// Verifies the reported repro: a closure returning `[$a, $b]` from two `mixed` parameters
/// keeps the second argument's string type instead of casting it to `int(0)`.
#[test]
fn test_closure_returns_array_literal_of_mixed_params() {
    let out = compile_and_run(
        r#"<?php
$f = function (mixed $a, mixed $b) { return [$a, $b]; };
var_dump($f(1, "z"));
"#,
    );
    assert_eq!(
        out,
        "array(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  string(1) \"z\"\n}\n"
    );
}

/// Verifies the same closure shape with the argument order reversed, so the mis-stamped slot
/// is the first element rather than the second.
#[test]
fn test_closure_returns_array_literal_string_first_int_second() {
    let out = compile_and_run(
        r#"<?php
$f = function (mixed $a, mixed $b) { return [$a, $b]; };
var_dump($f("z", 1));
"#,
    );
    assert_eq!(
        out,
        "array(2) {\n  [0]=>\n  string(1) \"z\"\n  [1]=>\n  int(1)\n}\n"
    );
}

/// Verifies a three-element literal of `mixed` parameters preserves int, string, and float
/// elements together.
#[test]
fn test_closure_returns_three_element_array_literal_of_mixed_params() {
    let out = compile_and_run(
        r#"<?php
$f = function (mixed $a, mixed $b, mixed $c) { return [$a, $b, $c]; };
var_dump($f(1, "z", 2.5));
"#,
    );
    assert_eq!(
        out,
        "array(3) {\n  [0]=>\n  int(1)\n  [1]=>\n  string(1) \"z\"\n  [2]=>\n  float(2.5)\n}\n"
    );
}

/// Verifies an arrow function body, which is the same single-`return` shape, gets the same
/// element typing as the closure form.
#[test]
fn test_arrow_function_returns_array_literal_of_mixed_params() {
    let out = compile_and_run(
        r#"<?php
$f = fn(mixed $a, mixed $b) => [$a, $b];
var_dump($f(1, "z"));
"#,
    );
    assert_eq!(
        out,
        "array(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  string(1) \"z\"\n}\n"
    );
}

/// Pins the named-function form, which reads its return type from checker metadata and was
/// always correct, so the closure fix cannot regress it.
#[test]
fn test_named_function_returns_array_literal_of_mixed_params() {
    let out = compile_and_run(
        r#"<?php
function pair(mixed $a, mixed $b) { return [$a, $b]; }
var_dump(pair(1, "z"));
"#,
    );
    assert_eq!(
        out,
        "array(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  string(1) \"z\"\n}\n"
    );
}

/// Pins the instance-method and static-method forms of the same literal, which also read
/// checker metadata rather than the closure inference.
#[test]
fn test_methods_return_array_literal_of_mixed_params() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public function pair(mixed $a, mixed $b) { return [$a, $b]; }
    public static function spair(mixed $a, mixed $b) { return [$a, $b]; }
}
var_dump((new Box())->pair(1, "z"));
var_dump(Box::spair("z", 1));
"#,
    );
    assert_eq!(
        out,
        "array(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  string(1) \"z\"\n}\n\
         array(2) {\n  [0]=>\n  string(1) \"z\"\n  [1]=>\n  int(1)\n}\n"
    );
}

/// Verifies a by-value captured string survives the returned literal. `$argc` keeps the
/// capture runtime-unknown so constant propagation cannot fold it into the literal.
#[test]
fn test_closure_returns_array_literal_with_by_value_capture() {
    let out = compile_and_run(
        r#"<?php
$tag = str_repeat("v", $argc);
$f = function (mixed $a) use ($tag) { return [$a, $tag]; };
var_dump($f(1));
"#,
    );
    assert_eq!(
        out,
        "array(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  string(1) \"v\"\n}\n"
    );
}

/// Verifies a by-reference captured string survives the returned literal and that a later
/// write through the reference is observed by a second call.
#[test]
fn test_closure_returns_array_literal_with_by_reference_capture() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$tag = "byref";
$f = function (mixed $a) use (&$tag) { return [$a, $tag]; };
var_dump($f(1));
$tag = "changed";
var_dump($f(2));
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "array(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  string(5) \"byref\"\n}\n\
         array(2) {\n  [0]=>\n  int(2)\n  [1]=>\n  string(7) \"changed\"\n}\n",
        "{}",
        out.stderr,
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr,
    );
}

/// Pins the two shapes that already worked: the literal assigned to a local before it is
/// returned, and a closure with an explicit `: array` return type.
#[test]
fn test_closure_array_literal_via_local_and_declared_return_type() {
    let out = compile_and_run(
        r#"<?php
$f = function (mixed $a, mixed $b) { $r = [$a, $b]; return $r; };
var_dump($f(1, "z"));
$h = function (mixed $a, mixed $b): array { return [$a, $b]; };
var_dump($h("z", 1));
"#,
    );
    assert_eq!(
        out,
        "array(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  string(1) \"z\"\n}\n\
         array(2) {\n  [0]=>\n  string(1) \"z\"\n  [1]=>\n  int(1)\n}\n"
    );
}

/// Verifies non-`mixed` declared parameter types are stamped from the signature too: a
/// `string`, `float`, or `bool` parameter used to be coerced to the syntactic `int` default.
#[test]
fn test_closure_returns_array_literal_of_typed_scalar_params() {
    let out = compile_and_run(
        r#"<?php
$s = function (string $x) { return [$x]; };
var_dump($s("hi"));
$g = function (float $x) { return [$x]; };
var_dump($g(2.5));
$b = function (bool $x) { return [$x]; };
var_dump($b(true));
"#,
    );
    assert_eq!(
        out,
        "array(1) {\n  [0]=>\n  string(2) \"hi\"\n}\n\
         array(1) {\n  [0]=>\n  float(2.5)\n}\n\
         array(1) {\n  [0]=>\n  bool(true)\n}\n"
    );
}
