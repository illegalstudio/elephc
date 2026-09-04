//! Purpose:
//! Integration tests for PHP's argument-introspection constructs `func_num_args()`,
//! `func_get_args()` and `func_get_arg($position)`, which let a function reach the surplus
//! positional arguments PHP allows past its declared parameter list.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected value in this file is the verbatim stdout of `LC_ALL=C php` 8.4.20 for
//!   the same fixture.
//! - The constructs are desugared by `elephc::func_args` into a hidden
//!   `mixed ...$__elephc_func_args` parameter plus plain PHP, so these tests also cover the
//!   variadic call machinery that collects the surplus arguments (direct calls, spreads,
//!   `call_user_func`, methods, closures and generators).

use crate::support::*;

/// Verifies the motivating case: a function that declares no parameters reports how many
/// arguments the caller actually passed, for zero, one and several arguments.
#[test]
fn test_func_num_args_without_declared_params() {
    let out = compile_and_run(
        r#"<?php
function va() { return func_num_args(); }
echo va(), "|", va(1), "|", va(1, 2, 3);
"#,
    );
    assert_eq!(out, "0|1|3");
}

/// Verifies that `func_get_args()` returns every argument, preserving each one's type
/// across a heterogeneous argument list.
#[test]
fn test_func_get_args_returns_all_arguments() {
    let out = compile_and_run(
        r#"<?php
function all() { return func_get_args(); }
var_dump(all(1, "a", 1.5, null, true));
"#,
    );
    assert_eq!(
        out,
        "array(5) {\n  [0]=>\n  int(1)\n  [1]=>\n  string(1) \"a\"\n  [2]=>\n  float(1.5)\n  [3]=>\n  NULL\n  [4]=>\n  bool(true)\n}\n"
    );
}

/// Verifies that declared parameters are included in the argument list and counted, and
/// that surplus positional arguments extend both.
#[test]
fn test_func_get_args_includes_declared_params() {
    let out = compile_and_run(
        r#"<?php
function withparams($a, $b) {
    echo func_num_args(), "|";
    var_dump(func_get_args());
}
withparams(1, "x");
withparams(1, "x", 3.5, null);
"#,
    );
    assert_eq!(
        out,
        "2|array(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  string(1) \"x\"\n}\n4|array(4) {\n  [0]=>\n  int(1)\n  [1]=>\n  string(1) \"x\"\n  [2]=>\n  float(3.5)\n  [3]=>\n  NULL\n}\n"
    );
}

/// Verifies that `func_get_arg()` reads a surplus argument by zero-based position.
#[test]
fn test_func_get_arg_reads_surplus_argument() {
    let out = compile_and_run(
        r#"<?php
function pick($a) { return func_get_arg(1); }
var_dump(pick(1, "second", 3));
"#,
    );
    assert_eq!(out, "string(6) \"second\"\n");
}

/// Verifies php-src's `ValueError` for a position at or past the number of arguments
/// passed, including the exact message text.
#[test]
fn test_func_get_arg_out_of_range_throws_value_error() {
    let out = compile_and_run(
        r#"<?php
function pick() {
    try { return func_get_arg(3); } catch (\ValueError $e) { return get_class($e) . ": " . $e->getMessage(); }
}
echo pick(1, 2);
"#,
    );
    assert_eq!(
        out,
        "ValueError: func_get_arg(): Argument #1 ($position) must be less than the number of the arguments passed to the currently executed function"
    );
}

/// Verifies php-src's separate `ValueError` message for a negative position.
#[test]
fn test_func_get_arg_negative_position_throws_value_error() {
    let out = compile_and_run(
        r#"<?php
function pick() {
    try { return func_get_arg(-1); } catch (\ValueError $e) { return get_class($e) . ": " . $e->getMessage(); }
}
echo pick(1, 2);
"#,
    );
    assert_eq!(
        out,
        "ValueError: func_get_arg(): Argument #1 ($position) must be greater than or equal to 0"
    );
}

/// Verifies PHP's "current values" rule: `func_get_args()` reflects a parameter that the
/// body reassigned, and a by-reference parameter it wrote through.
#[test]
fn test_func_get_args_reports_current_parameter_values() {
    let out = compile_and_run(
        r#"<?php
function reassigned($x) { $x = 42; return func_get_args(); }
var_dump(reassigned(7));
function byref(&$z) { $z = 99; return func_get_args(); }
$q = 1;
var_dump(byref($q), $q);
"#,
    );
    assert_eq!(
        out,
        "array(1) {\n  [0]=>\n  int(42)\n}\narray(1) {\n  [0]=>\n  int(99)\n}\nint(99)\n"
    );
}

/// Verifies a source variadic keeps its original positional history and omits named tail entries.
#[test]
fn test_func_get_args_in_source_variadic_function() {
    let out = compile_and_run(
        r#"<?php
function snapshot($head, ...$rest) {
    $head = 8;
    $rest = [9];
    echo func_num_args(), "|";
    var_dump(func_get_args());
}
snapshot(1, 2, 3, extra: 4);
"#,
    );
    assert_eq!(
        out,
        "3|array(3) {\n  [0]=>\n  int(8)\n  [1]=>\n  int(2)\n  [2]=>\n  int(3)\n}\n"
    );
}

/// Verifies optional parameters distinguish omitted defaults from supplied named values.
#[test]
fn test_func_args_with_optional_parameters() {
    let out = compile_and_run(
        r#"<?php
function optional_args($a = 10, $b = 20, $c = 30) {
    $a = 99;
    echo func_num_args(), ":", implode(",", func_get_args()), ":";
    try {
        echo func_get_arg(func_num_args());
    } catch (ValueError $error) {
        echo "range";
    }
    echo "|";
}
optional_args();
optional_args(1);
optional_args(b: 2);
optional_args(c: 3);
$values = [1, 2];
optional_args(...$values);
call_user_func("optional_args", 1, 2);
"#,
    );
    assert_eq!(
        out,
        "0::range|1:99:range|2:99,2:range|3:99,20,3:range|2:99,2:range|2:99,2:range|"
    );
}

/// Verifies optional and source-variadic parameters share the exact PHP passed count.
#[test]
fn test_func_args_with_optional_and_source_variadic_parameters() {
    let out = compile_and_run(
        r#"<?php
function optional_variadic($a = 10, $b = 20, ...$rest) {
    $a = 99;
    $rest = [88];
    echo func_num_args(), ":", implode(",", func_get_args()), "|";
}
optional_variadic();
optional_variadic(1);
optional_variadic(b: 2);
optional_variadic(1, 2, 3, 4, named: 5);
$values = [1, 2, 3];
optional_variadic(...$values);
"#,
    );
    assert_eq!(
        out,
        "0:|1:99|2:99,2|4:99,2,3,4|3:99,2,3|"
    );
}

/// Verifies optional source variadics keep their hidden count in methods and closures.
#[test]
fn test_func_args_with_optional_variadics_in_methods_and_closures() {
    let out = compile_and_run(
        r#"<?php
class OptionalVariadicFrames {
    public function instance($a = 10, ...$rest): string {
        $a = 99;
        $rest = [];
        return func_num_args() . ":" . implode(",", func_get_args());
    }

    public static function staticFrame($a = 10, ...$rest): string {
        $rest = [];
        return func_num_args() . ":" . implode(",", func_get_args());
    }
}

$object = new OptionalVariadicFrames();
$closure = function ($a = 10, ...$rest): string {
    $a = 77;
    $rest = [];
    return func_num_args() . ":" . implode(",", func_get_args());
};

echo $object->instance(), "|";
echo $object->instance(1, 2, named: 3), "|";
echo OptionalVariadicFrames::staticFrame(a: 4), "|";
echo $closure(5, 6, named: 7);
"#,
    );
    assert_eq!(out, "0:|2:99,2|1:4|2:77,6");
}

/// Verifies the constructs inside instance and static methods, which have their own
/// argument frame.
#[test]
fn test_func_args_in_methods() {
    let out = compile_and_run(
        r#"<?php
class K {
    public function m($a) { return func_num_args() . ":" . count(func_get_args()); }
    public static function s() { return func_get_args(); }
}
$k = new K();
echo $k->m(1, "b", 3.5), "|";
var_dump(K::s(9, null));
"#,
    );
    assert_eq!(
        out,
        "3:3|array(2) {\n  [0]=>\n  int(9)\n  [1]=>\n  NULL\n}\n"
    );
}

/// Verifies the constructs inside closures, whose argument frame is separate from the
/// enclosing scope's.
#[test]
fn test_func_args_in_closures() {
    let out = compile_and_run(
        r#"<?php
$c = function ($a) { return func_num_args(); };
$f = function () { var_dump(func_get_args()); };
echo $c(1, 2, 3), "|";
$f(1, "z");
"#,
    );
    assert_eq!(
        out,
        "3|array(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  string(1) \"z\"\n}\n"
    );
}

/// Verifies that the surplus arguments are collected the same way whether they arrive
/// through an argument unpack or through `call_user_func`.
#[test]
fn test_func_num_args_counts_spread_and_call_user_func() {
    let out = compile_and_run(
        r#"<?php
function spread() { return func_num_args(); }
echo spread(...[1, 2, 3, 4]), "|", call_user_func('spread', 1, 2);
"#,
    );
    assert_eq!(out, "4|2");
}

/// Verifies that one introspection construct can be nested inside another: the position
/// argument of `func_get_arg()` is itself computed from `func_num_args()`.
#[test]
fn test_func_get_arg_with_computed_position() {
    let out = compile_and_run(
        r#"<?php
function last($a) { return func_get_arg(func_num_args() - 1); }
var_dump(last(1, 2, "tail"));
"#,
    );
    assert_eq!(out, "string(4) \"tail\"\n");
}

/// Verifies that a generator sees its own arguments: the surplus arguments survive the
/// coroutine frame the generator body runs on.
#[test]
fn test_func_args_in_generator() {
    let out = compile_and_run(
        r#"<?php
function gen() { yield func_num_args(); foreach (func_get_args() as $v) { yield $v; } }
foreach (gen(1, "b", 3.5) as $v) { var_dump($v); }
"#,
    );
    assert_eq!(out, "int(3)\nint(1)\nstring(1) \"b\"\nfloat(3.5)\n");
}

/// Verifies the name forms PHP accepts: a fully-qualified call from inside a namespace and
/// an upper-case spelling, since PHP function names are case-insensitive.
#[test]
fn test_func_args_namespaced_and_case_insensitive() {
    let out = compile_and_run(
        r#"<?php
namespace App;
function g() { return \func_num_args(); }
function h() { return FUNC_GET_ARGS(); }
echo g(1, 2), "|";
var_dump(h("a", "b"));
"#,
    );
    assert_eq!(
        out,
        "2|array(2) {\n  [0]=>\n  string(1) \"a\"\n  [1]=>\n  string(1) \"b\"\n}\n"
    );
}

/// Verifies iterating the argument list, the idiomatic "sum every argument" use.
#[test]
fn test_func_get_args_is_iterable() {
    let out = compile_and_run(
        r#"<?php
function counted() { $seen = 0; foreach (func_get_args() as $arg) { $seen += (int) $arg; } return $seen . "/" . func_num_args(); }
echo counted(1, 2, 3);
"#,
    );
    assert_eq!(out, "6/3");
}

/// Verifies that the position expression handed to `func_get_arg()` is evaluated exactly
/// once even though the lowering range-checks it before reading: `$i++` must leave `$i` at
/// 1, not at 2 or 3.
#[test]
fn test_func_get_arg_evaluates_position_once() {
    let out = compile_and_run(
        r#"<?php
function once_only() { $i = 0; return func_get_arg($i++) . ":" . $i; }
echo once_only("first", "second");
"#,
    );
    assert_eq!(out, "first:1");
}
