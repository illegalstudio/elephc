//! Purpose:
//! End-to-end coverage for the runtime `TypeError` raised when a `mixed` value is returned from
//! a declared `string`, `float` or `bool` return type.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected value is verbatim `php` 8.5 stdout; `__FILE__` is replaced so paths are
//!   stable. Values are routed through a `mixed` helper so the checker cannot decide them.

use super::*;

/// A `mixed` value returned from a declared `string`, `float` or `bool` return type is checked
/// like PHP: null, arrays and objects throw `f(): Return value must be of type T, U returned`
/// (a `Stringable` object still converts to `string`), numeric strings convert to `float`,
/// `int` keeps its existing check, and `?string`, `mixed` and unions keep returning null.
#[test]
fn test_mixed_values_returned_from_scalar_return_types_follow_php() {
    let out = compile_and_run(
        r#"<?php
class Name { public function __toString(): string { return "name"; } }
class Plain {}
function box(mixed $v): mixed { return $v; }
function gs(mixed $v): string { return $v; }
function gf(mixed $v): float { return $v; }
function gb(mixed $v): bool { return $v; }
function gi(mixed $v): int { return $v; }
function gns(mixed $v): ?string { return $v; }
function gm(mixed $v): mixed { return $v; }
function gu(mixed $v): string|int|null { return $v; }
function show(TypeError $e): void { echo str_replace(__FILE__, "F", $e->getMessage()), "\n"; }
class K {
    public function s(mixed $v): string { return $v; }
    public static function f(mixed $v): float { return $v; }
}
foreach ([null, [1], new Plain(), new Name(), "12", "1.5", "abc", 7, 2.5, true] as $v) {
    foreach (['gs', 'gf', 'gb'] as $f) {
        try { var_dump($f(box($v))); } catch (TypeError $e) { show($e); }
    }
}
try { var_dump(gs(box(null))); } catch (TypeError $e) { show($e); }
try { var_dump(gf(box("x"))); } catch (TypeError $e) { show($e); }
try { var_dump(gb(box([]))); } catch (TypeError $e) { show($e); }
try { var_dump(gi(box(null))); } catch (TypeError $e) { show($e); }
try { var_dump((new K())->s(box(null))); } catch (TypeError $e) { show($e); }
try { var_dump(K::f(box("1e3"))); } catch (TypeError $e) { show($e); }
var_dump(gns(box(null)), gm(box(null)), gu(box(null)));
try { gs(box(null)); echo "not reached\n"; } catch (TypeError $e) { show($e); }
"#,
    );
    assert_eq!(out, "gs(): Return value must be of type string, null returned\ngf(): Return value must be of type float, null returned\ngb(): Return value must be of type bool, null returned\ngs(): Return value must be of type string, array returned\ngf(): Return value must be of type float, array returned\ngb(): Return value must be of type bool, array returned\ngs(): Return value must be of type string, Plain returned\ngf(): Return value must be of type float, Plain returned\ngb(): Return value must be of type bool, Plain returned\nstring(4) \"name\"\ngf(): Return value must be of type float, Name returned\ngb(): Return value must be of type bool, Name returned\nstring(2) \"12\"\nfloat(12)\nbool(true)\nstring(3) \"1.5\"\nfloat(1.5)\nbool(true)\nstring(3) \"abc\"\ngf(): Return value must be of type float, string returned\nbool(true)\nstring(1) \"7\"\nfloat(7)\nbool(true)\nstring(3) \"2.5\"\nfloat(2.5)\nbool(true)\nstring(1) \"1\"\nfloat(1)\nbool(true)\ngs(): Return value must be of type string, null returned\ngf(): Return value must be of type float, string returned\ngb(): Return value must be of type bool, array returned\ngi(): Return value must be of type int, null returned\nK::s(): Return value must be of type string, null returned\nfloat(1000)\nNULL\nNULL\nNULL\ngs(): Return value must be of type string, null returned\n");
}

/// Under `strict_types=1` the returned value must be the exact scalar, plus an int for `float`.
#[test]
fn test_strict_types_returns_require_the_exact_scalar() {
    let out = compile_and_run(
        r#"<?php
declare(strict_types=1);
function box(mixed $v): mixed { return $v; }
function gs(mixed $v): string { return $v; }
function gf(mixed $v): float { return $v; }
function gb(mixed $v): bool { return $v; }
function show(TypeError $e): void { echo str_replace(__FILE__, "F", $e->getMessage()), "\n"; }
foreach ([1, "x", 2.5, true, null] as $v) {
    foreach (['gs', 'gf', 'gb'] as $f) {
        try { var_dump($f(box($v))); } catch (TypeError $e) { show($e); }
    }
}
"#,
    );
    assert_eq!(out, "gs(): Return value must be of type string, int returned\nfloat(1)\ngb(): Return value must be of type bool, int returned\nstring(1) \"x\"\ngf(): Return value must be of type float, string returned\ngb(): Return value must be of type bool, string returned\ngs(): Return value must be of type string, float returned\nfloat(2.5)\ngb(): Return value must be of type bool, float returned\ngs(): Return value must be of type string, true returned\ngf(): Return value must be of type float, true returned\nbool(true)\ngs(): Return value must be of type string, null returned\ngf(): Return value must be of type float, null returned\ngb(): Return value must be of type bool, null returned\n");
}

/// A generator's `Generator` return type is untouched, every `return` of a function is checked,
/// an undeclared return never throws, and the `TypeError` unwinds through `finally`.
#[test]
fn test_return_check_with_generators_early_returns_inferred_types_and_finally() {
    let out = compile_and_run(
        r#"<?php
function box(mixed $v): mixed { return $v; }
function gen(mixed $v): Generator { yield 1; return $v; }
function show(TypeError $e): void { echo str_replace(__FILE__, "F", $e->getMessage()), "\n"; }
function early(mixed $v): string { if ($v === 1) { return "one"; } return $v; }
function inferred($v) { return $v; }
$y = box(null);
$g = gen(box(null)); foreach ($g as $z) {} var_dump($g->getReturn());
try { var_dump(early(box(1)), early(box(null))); } catch (TypeError $e) { show($e); }
var_dump(inferred(box(null)));
try { try { early(box([])); } finally { echo "finally\n"; } } catch (TypeError $e) { show($e); }
"#,
    );
    assert_eq!(out, "NULL\nearly(): Return value must be of type string, null returned\nNULL\nfinally\nearly(): Return value must be of type string, array returned\n");
}

/// A runtime-typed value returned as `bool` converts by PHP truthiness: `"abc"` and `0.5` are
/// true, `"0"` and `""` are false (previously the value was read as an integer).
#[test]
fn test_mixed_returned_as_bool_uses_truthiness() {
    let out = compile_and_run(
        r#"<?php
function gb(mixed $v): bool { return $v; }
function box(mixed $v): mixed { return $v; }
var_dump(gb(box("abc")), gb(box("0")), gb(box(0.5)), gb(box("")));
"#,
    );
    assert_eq!(out, "bool(true)\nbool(false)\nbool(true)\nbool(false)\n");
}
