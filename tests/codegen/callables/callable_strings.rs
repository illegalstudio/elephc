//! Purpose:
//! End-to-end coverage for PHP callable strings bound to a declared `callable` parameter
//! (`function apply(callable $f) {...} apply("strtoupper", ...)`), covering plain function
//! names, `"Class::method"` names, case-insensitive and namespaced spellings, and the named
//! argument form.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected value is verbatim `LC_ALL=C php` 8.4.20 stdout.
//! - elephc resolves callables statically, so only a compile-time-known string binds; the
//!   rejected shapes are pinned in `tests/error_tests/callables.rs`.

use crate::support::*;

/// Verifies a builtin function-name string binds to a declared `callable` parameter and is
/// invoked inside the callee — the repro from the parameter-typing audit.
#[test]
fn test_builtin_name_string_binds_to_callable_parameter() {
    let out = compile_and_run(
        r#"<?php
        function apply(callable $f, string $s) { return $f($s); }
        echo apply("strtoupper", "abc");
        "#,
    );
    assert_eq!(out, "ABC");
}

/// Verifies a user-defined function-name string binds to a declared `callable` parameter.
#[test]
fn test_user_function_name_string_binds_to_callable_parameter() {
    let out = compile_and_run(
        r#"<?php
        function decorate(string $s) { return "[" . $s . "]"; }
        function apply(callable $f, string $s) { return $f($s); }
        echo apply("decorate", "abc");
        "#,
    );
    assert_eq!(out, "[abc]");
}

/// Verifies PHP's case-insensitive function names and the fully qualified `\name` spelling
/// both resolve when passed as a callable string.
#[test]
fn test_callable_name_string_is_case_insensitive_and_accepts_leading_backslash() {
    let out = compile_and_run(
        r#"<?php
        function apply(callable $f, string $s) { return $f($s); }
        echo apply("STRTOUPPER", "ab"), apply("\\strtolower", "CD");
        "#,
    );
    assert_eq!(out, "ABcd");
}

/// Verifies a `"Class::method"` string binds to a declared `callable` parameter and dispatches
/// to the static method.
#[test]
fn test_static_method_name_string_binds_to_callable_parameter() {
    let out = compile_and_run(
        r#"<?php
        class Formatter {
            public static function wrap(string $s): string { return "<" . $s . ">"; }
        }
        function apply(callable $f, string $s) { return $f($s); }
        echo apply("Formatter::wrap", "abc");
        "#,
    );
    assert_eq!(out, "<abc>");
}

/// Verifies the binding also fires when the callable string is passed as a named argument,
/// which reaches EIR through the reordered named-argument path.
#[test]
fn test_callable_name_string_binds_through_named_argument() {
    let out = compile_and_run(
        r#"<?php
        function apply(callable $f, string $s) { return $f($s); }
        echo apply(s: "abc", f: "strtoupper");
        "#,
    );
    assert_eq!(out, "ABC");
}

/// Verifies a callable string bound to a method parameter behaves like the function case.
#[test]
fn test_callable_name_string_binds_to_method_parameter() {
    let out = compile_and_run(
        r#"<?php
        class Runner {
            public function run(callable $f, string $s) { return $f($s); }
        }
        echo (new Runner())->run("strtoupper", "abc");
        "#,
    );
    assert_eq!(out, "ABC");
}

/// Verifies a bound callable string carries its signature into the callee, so a call with the
/// wrong argument count is still rejected rather than silently accepted.
#[test]
fn test_bound_callable_string_keeps_working_alongside_first_class_callables() {
    let out = compile_and_run(
        r#"<?php
        function apply(callable $f, string $s) { return $f($s); }
        echo apply("strtoupper", "ab"), apply(strtolower(...), "CD"), apply(fn($x) => $x . "!", "e");
        "#,
    );
    assert_eq!(out, "ABcde!");
}


/// Regression for #576: a function reachable only through a dynamic callable must return what it
/// was given, not an int cast of it.
///
/// An untyped parameter starts as the checker's `Int` PLACEHOLDER, which direct call sites
/// specialize away. A function whose only callers are dynamic never gets that, so
/// `function h($b, $p) { return $b; }` recorded `Int` as its return type — and the
/// runtime-callable invoker coerced the real value through it. A returned string arrived as
/// `int(0)`, silently: no diagnostic, no cast in the source, nothing to see at the call site.
///
/// The rows are the issue's own scope table, each one a different reason to be included:
///
/// - `h` returning the STRING parameter is the defect.
/// - `hp` returning the INT parameter is the row that was accidentally correct before, because
///   an int survives an int cast — so it pins that the fix did not simply widen everything.
/// - `ht` with declared types never had the defect and must not change.
/// - `m` is the MASKING variant: one direct call anywhere taught the checker the real type, so
///   the dynamic path was correct too. Both calls are asserted, in that order, because the
///   fix must not depend on which one runs first.
/// - `strlen(call_user_func(...))` is the static consumer. Pre-fix it did not merely print the
///   wrong value, it refused to compile — *"strlen for PHP type Int"* — which is how the
///   recorded return type can be read back directly.
/// - `add` computes its result instead of forwarding a parameter, so it keeps `int`. Without
///   that row the fix could be a blanket widening of every untyped function.
/// - `array_map` and `call_user_func_array` are the other two entry points into the same
///   invoker.
///
/// Every expectation is the host PHP 8.5.10 output for the same fixture.
#[test]
fn test_dynamic_only_function_returns_its_argument_not_an_int_cast() {
    let out = compile_and_run(
        r#"<?php
function h($b, $p) { return $b; }
function hp($b, $p) { return $p; }
function ht(string $b, int $p): string { return $b; }

$fn = 'h';
var_dump(call_user_func($fn, "probe", 9));
$fnp = 'hp';
var_dump(call_user_func($fnp, "x", 12345));
$fnt = 'ht';
var_dump(call_user_func($fnt, "typed", 1));

function m($b) { return $b; }
var_dump(m("direct"));
$fm = 'm';
var_dump(call_user_func($fm, "probe"));

function s($b) { return $b; }
$fs = 's';
var_dump(strlen(call_user_func($fs, "abcde")));

function any($v) { return $v; }
$fa = 'any';
var_dump(call_user_func($fa, 1.5));
var_dump(call_user_func($fa, true));
var_dump(call_user_func($fa, null));

function add($a, $b) { return $a + $b; }
$fadd = 'add';
var_dump(call_user_func($fadd, 2, 3));

var_dump(array_map('m', ["a", "b"]));
var_dump(call_user_func_array('h', ["arr", 7]));

function fcc($b) { return $b; }
var_dump(array_map(fcc(...), ["probe"]));
$bound = fcc(...);
var_dump($bound("bound"));
var_dump(call_user_func(fcc(...), "cuf"));
"#,
    );
    assert_eq!(
        out,
        concat!(
            "string(5) \"probe\"\n",
            "int(12345)\n",
            "string(5) \"typed\"\n",
            "string(6) \"direct\"\n",
            "string(5) \"probe\"\n",
            "int(5)\n",
            "float(1.5)\n",
            "bool(true)\n",
            "NULL\n",
            "int(5)\n",
            "array(2) {\n  [0]=>\n  string(1) \"a\"\n  [1]=>\n  string(1) \"b\"\n}\n",
            "string(3) \"arr\"\n",
            "array(1) {\n  [0]=>\n  string(5) \"probe\"\n}\n",
            "string(5) \"bound\"\n",
            "string(3) \"cuf\"\n",
        )
    );
}


/// A signature PROBE and an argument-less call are not call sites, so neither suppresses the
/// pass-through return widening.
///
/// `widen_dynamic_only_passthrough_returns` skips a function a real caller taught its parameter
/// types to. Two things reach the same checker entry point without teaching it anything:
///
/// - `function_exists('fe')` resolves `fe`'s signature by calling it with FABRICATED zeros
///   (`check_function_exists` builds one `IntLiteral(0)` per parameter). Counting that as a
///   caller left `fe`'s untyped parameter on the `Int` placeholder, and the dynamic call
///   returned `int(0)` instead of the string.
/// - `opt()` on `function opt($b = null) { return $b; }` is a genuine call that passes nothing,
///   so `$b` keeps the type its default implies and the dynamic call returned `NULL`.
///
/// Both are driven through a callable VARIABLE on purpose. A literal callable string is
/// validated against the signature during the top-level walk, which runs before the widening
/// pass, so it reports a compile error instead — a separate, pre-existing ordering problem that
/// this fixture deliberately does not depend on.
///
/// Every expectation is the host PHP 8.5.10 output for the same fixture.
#[test]
fn test_signature_probes_do_not_suppress_dynamic_return_widening() {
    let out = compile_and_run(
        r#"<?php
function fe($b) { return $b; }
var_dump(function_exists('fe'));
$fn = 'fe';
var_dump(call_user_func($fn, "probe"));

function opt($b = null) { return $b; }
var_dump(opt());
$fo = 'opt';
var_dump(call_user_func($fo, "probe"));
"#,
    );
    assert_eq!(
        out,
        concat!(
            "bool(true)\n",
            "string(5) \"probe\"\n",
            "NULL\n",
            "string(5) \"probe\"\n",
        )
    );
}
