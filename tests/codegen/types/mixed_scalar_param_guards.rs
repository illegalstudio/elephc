//! Purpose:
//! End-to-end coverage for the runtime `TypeError` raised when a `mixed` (or merged union)
//! argument reaches a declared non-nullable scalar parameter of a user function or method.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected value is verbatim `php` 8.5 stdout (deprecations silenced for the implicitly
//!   nullable default); `__FILE__` is replaced so the "called in" path is stable.
//! - Values are routed through a `mixed` helper so the checker cannot decide the binding.

use super::*;

/// Issue repro: a `mixed` holding null passed to a `string` parameter used to bind as `""`;
/// PHP throws `TypeError` naming the callee, the parameter and the call line.
#[test]
fn test_mixed_null_to_string_parameter_throws_type_error() {
    let out = compile_and_run(
        r#"<?php
function t(string $s): string { return $s; }
function f(mixed $v): string { return t($v); }
try { echo f(null), "|\n"; } catch (TypeError $e) { echo str_replace(__FILE__, "F", $e->getMessage()), "\n"; }
echo f("ok"), "|\n";
"#,
    );
    assert_eq!(out, "t(): Argument #1 ($s) must be of type string, null given, called in F on line 3\nok|\n");
}

/// A ternary that merges `string` with `string|null` is `mixed`; its null arm must still be
/// rejected while the string arms bind unchanged.
#[test]
fn test_ternary_merging_nullable_string_into_string_parameter() {
    let out = compile_and_run(
        r#"<?php
function t(string $s): string { return "[" . $s . "]"; }
function pick(string|int|null $v): string { return t(is_int($v) ? (string) $v : $v); }
foreach ([5, "a", null] as $v) {
    try { echo pick($v), "\n"; } catch (TypeError $e) { echo str_replace(__FILE__, "F", $e->getMessage()), "\n"; }
}
"#,
    );
    assert_eq!(out, "[5]\n[a]\nt(): Argument #1 ($s) must be of type string, null given, called in F on line 3\n");
}

/// Every declared scalar rejects null, arrays and plain objects from a `mixed` argument in
/// coercive mode; a `Stringable` object still binds to `string` but not to `int`, and scalars
/// keep their coercive conversions.
#[test]
fn test_non_scalar_mixed_values_rejected_by_scalar_parameters() {
    let out = compile_and_run(
        r#"<?php
class Name { public function __toString(): string { return "name"; } }
class Plain {}
function str(string $s): string { return "s:" . $s; }
function num(int $i): string { return "i:" . $i; }
function real(float $f): string { return "f:" . $f; }
function flag(bool $b): string { return $b ? "b:T" : "b:F"; }
function box(mixed $v): mixed { return $v; }
function show(TypeError $e): void { echo str_replace(__FILE__, "F", $e->getMessage()), "\n"; }
$null = box(null); $list = box([1, 2]); $name = box(new Name()); $plain = box(new Plain()); $one = box(1);
try { echo str($null), "\n"; } catch (TypeError $e) { show($e); }
try { echo num($null), "\n"; } catch (TypeError $e) { show($e); }
try { echo real($null), "\n"; } catch (TypeError $e) { show($e); }
try { echo flag($null), "\n"; } catch (TypeError $e) { show($e); }
try { echo str($list), "\n"; } catch (TypeError $e) { show($e); }
try { echo num($plain), "\n"; } catch (TypeError $e) { show($e); }
try { echo str($name), "\n"; } catch (TypeError $e) { show($e); }
try { echo num($name), "\n"; } catch (TypeError $e) { show($e); }
echo str($one), " ", num($one), " ", real($one), " ", flag($one), "\n";
"#,
    );
    assert_eq!(out, "str(): Argument #1 ($s) must be of type string, null given, called in F on line 11\nnum(): Argument #1 ($i) must be of type int, null given, called in F on line 12\nreal(): Argument #1 ($f) must be of type float, null given, called in F on line 13\nflag(): Argument #1 ($b) must be of type bool, null given, called in F on line 14\nstr(): Argument #1 ($s) must be of type string, array given, called in F on line 15\nnum(): Argument #1 ($i) must be of type int, Plain given, called in F on line 16\ns:name\nnum(): Argument #1 ($i) must be of type int, Name given, called in F on line 18\ns:1 i:1 f:1 b:T\n");
}

/// `?string`, `mixed`, `string|null` and an implicitly nullable `string $s = null` keep
/// accepting null, and builtins keep their own null handling (no TypeError).
#[test]
fn test_nullable_mixed_union_and_null_default_parameters_accept_null() {
    let out = compile_and_run(
        r#"<?php
function nullable(?string $s): string { return $s === null ? "null" : $s; }
function anything(mixed $s): string { return $s === null ? "null" : "set"; }
function either(string|null $s): string { return $s === null ? "null" : $s; }
function defaulted(string $s = null): string { return "defaulted"; }
function box(mixed $v): mixed { return $v; }
$null = box(null);
echo nullable($null), " ", anything($null), " ", either($null), " ", defaulted($null), "\n";
echo strlen(box("abc")), "\n";
"#,
    );
    assert_eq!(out, "null null null defaulted\n3\n");
}

/// Instance and static methods, promoted constructor parameters, named arguments and an
/// associative spread all run the check and name the declared parameter.
#[test]
fn test_methods_constructors_and_named_arguments_are_checked() {
    let out = compile_and_run(
        r#"<?php
class Counter {
    public function __construct(public string $label = "none") {}
    public function add(int $n): string { return $this->label . "+" . $n; }
    public static function scale(float $f): string { return "x" . $f; }
}
function pair(string $a, int $b = 0): string { return $a . $b; }
function box(mixed $v): mixed { return $v; }
function show(TypeError $e): void { echo str_replace(__FILE__, "F", $e->getMessage()), "\n"; }
$null = box(null);
try { echo (new Counter())->add($null), "\n"; } catch (TypeError $e) { show($e); }
try { echo Counter::scale($null), "\n"; } catch (TypeError $e) { show($e); }
try { echo (new Counter($null))->label, "\n"; } catch (TypeError $e) { show($e); }
try { echo pair(b: 1, a: $null), "\n"; } catch (TypeError $e) { show($e); }
try { echo pair(...['a' => $null]), "\n"; } catch (TypeError $e) { show($e); }
echo pair(b: 2, a: box("ok")), "\n";
"#,
    );
    assert_eq!(out, "Counter::add(): Argument #1 ($n) must be of type int, null given, called in F on line 11\nCounter::scale(): Argument #1 ($f) must be of type float, null given, called in F on line 12\nCounter::__construct(): Argument #1 ($label) must be of type string, null given, called in F on line 13\npair(): Argument #1 ($a) must be of type string, null given, called in F on line 14\npair(): Argument #1 ($a) must be of type string, null given, called in F on line 15\nok2\n");
}

/// Under `strict_types=1` a `mixed` argument must hold the exact scalar (plus int for float);
/// coercive conversions such as int-to-string or `"12"`-to-int throw instead.
#[test]
fn test_strict_types_requires_exact_scalar_from_mixed_argument() {
    let out = compile_and_run(
        r#"<?php
declare(strict_types=1);
function str(string $s): string { return "s:" . $s; }
function num(int $i): string { return "i:" . $i; }
function real(float $f): string { return "f:" . $f; }
function flag(bool $b): string { return $b ? "b:T" : "b:F"; }
function box(mixed $v): mixed { return $v; }
function show(TypeError $e): void { echo str_replace(__FILE__, "F", $e->getMessage()), "\n"; }
$null = box(null); $one = box(1); $text = box("12"); $half = box(1.5); $yes = box(true);
try { echo str($null), "\n"; } catch (TypeError $e) { show($e); }
try { echo str($one), "\n"; } catch (TypeError $e) { show($e); }
try { echo num($text), "\n"; } catch (TypeError $e) { show($e); }
try { echo num($half), "\n"; } catch (TypeError $e) { show($e); }
try { echo real($yes), "\n"; } catch (TypeError $e) { show($e); }
try { echo flag($one), "\n"; } catch (TypeError $e) { show($e); }
echo str($text), " ", num($one), " ", real($one), " ", real($half), " ", flag($yes), "\n";
"#,
    );
    assert_eq!(out, "str(): Argument #1 ($s) must be of type string, null given, called in F on line 10\nstr(): Argument #1 ($s) must be of type string, int given, called in F on line 11\nnum(): Argument #1 ($i) must be of type int, string given, called in F on line 12\nnum(): Argument #1 ($i) must be of type int, float given, called in F on line 13\nreal(): Argument #1 ($f) must be of type float, true given, called in F on line 14\nflag(): Argument #1 ($b) must be of type bool, int given, called in F on line 15\ns:12 i:1 f:1 f:1.5 b:T\n");
}

/// A call whose result is unused and whose callee is pure must survive dead-code elimination
/// and keep its `catch (TypeError)` reachable, because the argument check itself throws.
#[test]
fn test_guarded_call_to_pure_function_is_not_eliminated() {
    let out = compile_and_run(
        r#"<?php
function pure(string $s): int { return 1; }
function box(mixed $v): mixed { return $v; }
$null = box(null);
try {
    pure($null);
    echo "not reached\n";
} catch (TypeError $e) {
    echo "caught: ", str_replace(__FILE__, "F", $e->getMessage()), "\n";
}
echo "end\n";
"#,
    );
    assert_eq!(out, "caught: pure(): Argument #1 ($s) must be of type string, null given, called in F on line 6\nend\n");
}

/// Closures, arrow functions and `call_user_func` targets bound statically run the same check,
/// with PHP's `{closure:file:line}` callee spelling.
#[test]
fn test_closures_and_call_user_func_check_mixed_arguments() {
    let out = compile_and_run(
        r#"<?php
function box(mixed $v): mixed { return $v; }
function named(string $s): string { return "n:" . $s; }
function show(TypeError $e): void { echo str_replace(__FILE__, "F", $e->getMessage()), "\n"; }
$null = box(null);
$closure = function (string $x): string { return "c:" . $x; };
$arrow = fn(int $i): int => $i + 1;
try { echo $closure($null), "\n"; } catch (TypeError $e) { show($e); }
try { echo $arrow($null), "\n"; } catch (TypeError $e) { show($e); }
try { echo call_user_func('named', $null), "\n"; } catch (TypeError $e) { show($e); }
echo $closure(box("ok")), " ", $arrow(box(1)), "\n";
"#,
    );
    assert_eq!(out, "{closure:F:6}(): Argument #1 ($x) must be of type string, null given, called in F on line 8\n{closure:F:7}(): Argument #1 ($i) must be of type int, null given, called in F on line 9\nnamed(): Argument #1 ($s) must be of type string, null given, called in F on line 10\nc:ok 2\n");
}

/// An unused call to a pure callee keeps its check when the guarded value reaches the
/// parameter through a static associative spread or a named argument: the optimizer must not
/// delete the call or prune the `catch (TypeError)` around it.
#[test]
fn test_spread_and_named_guarded_calls_to_pure_function_are_not_eliminated() {
    let out = compile_and_run(
        r#"<?php
function pure(string $s): int { return 1; }
function box(mixed $v): mixed { return $v; }
$v = box(null);
try {
    pure(...['s' => $v]);
    echo "not reached\n";
} catch (TypeError $e) {
    echo "caught: ", str_replace(__FILE__, "F", $e->getMessage()), "\n";
}
try {
    pure(s: $v);
    echo "not reached\n";
} catch (TypeError $e) {
    echo "caught: ", str_replace(__FILE__, "F", $e->getMessage()), "\n";
}
echo "end\n";
"#,
    );
    assert_eq!(out, "caught: pure(): Argument #1 ($s) must be of type string, null given, called in F on line 6\ncaught: pure(): Argument #1 ($s) must be of type string, null given, called in F on line 12\nend\n");
}

/// PHP evaluates every argument before the callee checks a parameter type: a later argument's
/// side effects run first, named arguments are checked in parameter order, and a call split
/// over several lines reports the line where the call starts.
#[test]
fn test_parameter_check_runs_after_all_arguments_and_reports_the_call_line() {
    let out = compile_and_run(
        r#"<?php
function t(string $s, int $i = 0): string { return $s . $i; }
function k(int $a, string $b): string { return $a . $b; }
function m(mixed $v): mixed { return $v; }
function side(string $label): int { echo "side:$label\n"; return 1; }
function show(TypeError $e): void { echo str_replace(__FILE__, "F", $e->getMessage()), "\n"; }
$n = m(null);
try { echo t($n, side("a")), "\n"; } catch (TypeError $e) { show($e); }
try { echo t(
    $n,
    side("b")
), "\n"; } catch (TypeError $e) { show($e); }
try { echo k(side("c"), $n), "\n"; } catch (TypeError $e) { show($e); }
try { echo t(i: side("d"), s: $n), "\n"; } catch (TypeError $e) { show($e); }
try { t($n); echo "not reached\n"; } catch (TypeError $e) { show($e); }
echo t(m("ok"), side("e")), "\n";
echo "end\n";
"#,
    );
    assert_eq!(out, "side:a\nt(): Argument #1 ($s) must be of type string, null given, called in F on line 8\nside:b\nt(): Argument #1 ($s) must be of type string, null given, called in F on line 9\nside:c\nk(): Argument #2 ($b) must be of type string, null given, called in F on line 13\nside:d\nt(): Argument #1 ($s) must be of type string, null given, called in F on line 14\nt(): Argument #1 ($s) must be of type string, null given, called in F on line 15\nside:e\nok1\nend\n");
}

/// A string held in `mixed` binds to `int`/`float` only when PHP's numeric grammar accepts all
/// of it (surrounding whitespace allowed); leading-numeric, hex, `INF` and empty strings throw.
/// A lossy float or float-string to `int` emits the 8.1+ deprecation, and a float or integer
/// string outside the int range throws, exactly as php 8.5 does.
#[test]
fn test_numeric_strings_and_floats_from_mixed_follow_php_binding_rules() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $no, string $msg): bool { echo "[diag] ", $msg, "\n"; return true; });
function ti(int $i): string { return var_export($i, true); }
function tf(float $f): string { return var_export($f, true); }
function m(mixed $v): mixed { return $v; }
function show(TypeError $e): void { echo str_replace(__FILE__, "F", $e->getMessage()), "\n"; }
foreach (["12", " 12", "12 ", "12abc", "abc", "", "1.5", "1e3", "0x1A", " ", "1.0", "9999999999999999999", ".5", "INF", "1e100", "12.5abc", "-7"] as $s) {
    try { echo "ti(", var_export($s, true), ") = ", ti(m($s)), "\n"; } catch (TypeError $e) { show($e); }
    try { echo "tf(", var_export($s, true), ") = ", tf(m($s)), "\n"; } catch (TypeError $e) { show($e); }
}
foreach ([1.5, 1.0, 1e100, NAN, INF, true, false, 3] as $v) {
    try { echo "ti(", var_export($v, true), ") = ", ti(m($v)), "\n"; } catch (TypeError $e) { show($e); }
    try { echo "tf(", var_export($v, true), ") = ", tf(m($v)), "\n"; } catch (TypeError $e) { show($e); }
}
"#,
    );
    assert_eq!(out, "ti('12') = 12\ntf('12') = 12.0\nti(' 12') = 12\ntf(' 12') = 12.0\nti('12 ') = 12\ntf('12 ') = 12.0\nti('12abc') = ti(): Argument #1 ($i) must be of type int, string given, called in F on line 8\ntf('12abc') = tf(): Argument #1 ($f) must be of type float, string given, called in F on line 9\nti('abc') = ti(): Argument #1 ($i) must be of type int, string given, called in F on line 8\ntf('abc') = tf(): Argument #1 ($f) must be of type float, string given, called in F on line 9\nti('') = ti(): Argument #1 ($i) must be of type int, string given, called in F on line 8\ntf('') = tf(): Argument #1 ($f) must be of type float, string given, called in F on line 9\nti('1.5') = [diag] Implicit conversion from float-string \"1.5\" to int loses precision\n1\ntf('1.5') = 1.5\nti('1e3') = 1000\ntf('1e3') = 1000.0\nti('0x1A') = ti(): Argument #1 ($i) must be of type int, string given, called in F on line 8\ntf('0x1A') = tf(): Argument #1 ($f) must be of type float, string given, called in F on line 9\nti(' ') = ti(): Argument #1 ($i) must be of type int, string given, called in F on line 8\ntf(' ') = tf(): Argument #1 ($f) must be of type float, string given, called in F on line 9\nti('1.0') = 1\ntf('1.0') = 1.0\nti('9999999999999999999') = ti(): Argument #1 ($i) must be of type int, string given, called in F on line 8\ntf('9999999999999999999') = 1.0E+19\nti('.5') = [diag] Implicit conversion from float-string \".5\" to int loses precision\n0\ntf('.5') = 0.5\nti('INF') = ti(): Argument #1 ($i) must be of type int, string given, called in F on line 8\ntf('INF') = tf(): Argument #1 ($f) must be of type float, string given, called in F on line 9\nti('1e100') = ti(): Argument #1 ($i) must be of type int, string given, called in F on line 8\ntf('1e100') = 1.0E+100\nti('12.5abc') = ti(): Argument #1 ($i) must be of type int, string given, called in F on line 8\ntf('12.5abc') = tf(): Argument #1 ($f) must be of type float, string given, called in F on line 9\nti('-7') = -7\ntf('-7') = -7.0\nti(1.5) = [diag] Implicit conversion from float 1.5 to int loses precision\n1\ntf(1.5) = 1.5\nti(1.0) = 1\ntf(1.0) = 1.0\nti(1.0E+100) = ti(): Argument #1 ($i) must be of type int, float given, called in F on line 12\ntf(1.0E+100) = 1.0E+100\nti(NAN) = ti(): Argument #1 ($i) must be of type int, float given, called in F on line 12\ntf(NAN) = NAN\nti(INF) = ti(): Argument #1 ($i) must be of type int, float given, called in F on line 12\ntf(INF) = INF\nti(true) = 1\ntf(true) = 1.0\nti(false) = 0\ntf(false) = 0.0\nti(3) = 3\ntf(3) = 3.0\n");
}

/// Positional spreads, static or from a runtime array, check each element bound to a declared
/// scalar from its value (numeric strings convert), and an unused call to a pure callee keeps
/// the check and its `catch`.
#[test]
fn test_positional_spread_elements_are_checked() {
    let out = compile_and_run(
        r#"<?php
function named(string $s, int $n = 0): string { return "n:" . $s . $n; }
function pure(string $s): int { return 1; }
function box(mixed $v): mixed { return $v; }
function show(TypeError $e): void { echo str_replace(__FILE__, "F", $e->getMessage()), "\n"; }
$null = box(null);
$args = [$null];
$tail = [box("x"), box("nope")];
try { echo named(...[$null]), "\n"; } catch (TypeError $e) { show($e); }
try { echo named(...$args), "\n"; } catch (TypeError $e) { show($e); }
try { echo named(...$tail), "\n"; } catch (TypeError $e) { show($e); }
try { pure(...[$null]); echo "not reached\n"; } catch (TypeError $e) { show($e); }
try { pure(...$args); echo "not reached\n"; } catch (TypeError $e) { show($e); }
echo named(...[box("ok"), box(" 7")]), "\n";
"#,
    );
    assert_eq!(out, "named(): Argument #1 ($s) must be of type string, null given, called in F on line 9\nnamed(): Argument #1 ($s) must be of type string, null given, called in F on line 10\nnamed(): Argument #2 ($n) must be of type int, string given, called in F on line 11\npure(): Argument #1 ($s) must be of type string, null given, called in F on line 12\npure(): Argument #1 ($s) must be of type string, null given, called in F on line 13\nn:ok7\n");
}

/// The `int`/`float` guard replaces a temporary box with the converted scalar, so the box must
/// be released: 400k guarded calls run inside a 1 MiB heap.
#[test]
fn test_numeric_guard_releases_the_temporary_box() {
    let out = compile_and_run_with_heap_size(
        r#"<?php
function twice(int $n): int { return $n * 2; }
function half(float $f): float { return $f / 2; }
function box(mixed $v): mixed { return $v; }
$nums = [box(1), box("2"), box(3.0), box(true)];
$total = 0;
$sum = 0.0;
for ($i = 0; $i < 200000; $i++) {
    $total += twice($nums[$i & 3]);
    $sum += half($nums[$i & 3]);
}
echo $total, " ", $sum, "\n";
"#,
        1_048_576,
    );
    assert_eq!(out, "700000 175000\n");
}

/// An integer-form string binds to `int` exactly across the whole int range (no rounding
/// through a double): `"9007199254740993"` stays odd and `"9223372036854775807"` binds as
/// `PHP_INT_MAX`, while one past it throws and float-form strings keep their deprecation.
#[test]
fn test_integer_strings_bind_to_int_exactly() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $no, string $msg): bool { echo "[diag] ", $msg, "\n"; return true; });
function ti(int $i): string { return (string) $i; }
function m(mixed $v): mixed { return $v; }
function show(TypeError $e): void { echo str_replace(__FILE__, "F", $e->getMessage()), "\n"; }
foreach (["9007199254740993", "9223372036854775807", "-9223372036854775808", " 9223372036854775807 ", "9223372036854775808", "-9223372036854775809", "1e3", "1.5", "007", "+5", "-0"] as $s) {
    try { echo var_export($s, true), " => ", ti(m($s)), "\n"; } catch (TypeError $e) { show($e); }
}
"#,
    );
    assert_eq!(out, "'9007199254740993' => 9007199254740993\n'9223372036854775807' => 9223372036854775807\n'-9223372036854775808' => -9223372036854775808\n' 9223372036854775807 ' => 9223372036854775807\n'9223372036854775808' => ti(): Argument #1 ($i) must be of type int, string given, called in F on line 7\n'-9223372036854775809' => -9223372036854775808\n'1e3' => 1000\n'1.5' => [diag] Implicit conversion from float-string \"1.5\" to int loses precision\n1\n'007' => 7\n'+5' => 5\n'-0' => 0\n");
}

/// The checker routes a guarded call's `TypeError` to its `catch` while it types the function:
/// the guarded-argument set the optimizer reads is only installed after type checking, so a
/// pure callee left the handler unreachable and `$w`, bound only there, unknown after the try
/// (`strlen()` then refused it at compile time).
#[test]
fn test_catch_of_guarded_pure_call_is_reachable_for_the_checker() {
    let out = compile_and_run(
        r#"<?php
function take(string $s): int { return 1; }
function run(mixed $v): void {
    try {
        take($v);
    } catch (TypeError $e) {
        $w = "caught";
    }
    echo $w, " ", strlen($w), "\n";
}
run(null);
"#,
    );
    assert_eq!(out, "caught 6\n");
}

/// A guard inside a callee's body makes the CALLER's `catch (TypeError)` reachable: `g()` binds
/// its `mixed` to `take(string)`, so `g($v)` can throw even though nothing at the call site is
/// guarded. The checker's callee summaries are built before any body is typed, so they missed
/// that `TypeError` and `$w` (bound only in the handler) was unknown after the `try`; the same
/// goes for a static method reached through an instance method and a directly invoked closure.
#[test]
fn test_catch_of_call_whose_callee_guards_is_reachable_for_the_checker() {
    let out = compile_and_run(
        r#"<?php
function take(string $s): int { return 1; }
function g(mixed $v): int { return take($v); }
function box(mixed $v): mixed { return $v; }
class K {
    public static function s(mixed $v): int { return take($v); }
    public function m(mixed $v): int { return K::s($v); }
}
function run(mixed $v): void {
    try { g($v); } catch (TypeError $e) { $w = "caught"; }
    echo $w, " ", strlen($w), "\n";
}
run(null);
$v = box(null);
try { g($v); } catch (TypeError $e) { $w = "top"; }
echo $w, " ", strlen($w), "\n";
try { (new K())->m($v); } catch (TypeError $e) { $u = "method"; }
echo $u, " ", strlen($u), "\n";
try { (function () use ($v) { take($v); })(); } catch (TypeError $e) { $c = "closure"; }
echo $c, " ", strlen($c), "\n";
"#,
    );
    assert_eq!(out, "caught 6\ntop 3\nmethod 6\nclosure 7\n");
}

/// A runtime spread the callee binds itself left the positional arguments before it unchecked:
/// `$o->m($null, ...side())` bound `null` as `""`. PHP evaluates the spread (its side effects
/// first) and then throws the prefix's `TypeError`, for methods, static methods, functions,
/// constructors and nullsafe calls alike, and a pure callee's call stays observable.
#[test]
fn test_runtime_spread_calls_check_the_positional_prefix() {
    let out = compile_and_run(
        r#"<?php
function box(mixed $v): mixed { return $v; }
function side(): array { echo "side\n"; return [2]; }
function show(TypeError $e): void { echo str_replace(__FILE__, "F", $e->getMessage()), "\n"; }
class O {
    public function m(string $s, int ...$rest): string { return "m:" . $s; }
    public static function s(string $s, int ...$rest): string { return "s:" . $s; }
    public function p(string $s, int $n, int ...$r): string { return "p:" . $s; }
    public function __construct(string $s = "x", int ...$rest) {}
}
function f(string $s, int ...$rest): string { return "f:" . $s; }
function pure(string $s, int ...$r): int { return 1; }
$o = new O();
$v = box(null);
try { echo $o->m($v, ...side()), "\n"; } catch (TypeError $e) { show($e); }
try { echo O::s($v, ...side()), "\n"; } catch (TypeError $e) { show($e); }
try { echo f($v, ...side()), "\n"; } catch (TypeError $e) { show($e); }
try { new O($v, ...side()); echo "new ok\n"; } catch (TypeError $e) { show($e); }
try { echo $o?->m($v, ...side()), "\n"; } catch (TypeError $e) { show($e); }
try { echo $o->p($v, 1, ...side()), "\n"; } catch (TypeError $e) { show($e); }
try { echo $o->m(box("ok"), ...side()), "\n"; } catch (TypeError $e) { show($e); }
try { pure($v, ...[1, 2]); echo "kept?\n"; } catch (TypeError $e) { show($e); }
"#,
    );
    assert_eq!(out, "side\nO::m(): Argument #1 ($s) must be of type string, null given, called in F on line 15\nside\nO::s(): Argument #1 ($s) must be of type string, null given, called in F on line 16\nside\nf(): Argument #1 ($s) must be of type string, null given, called in F on line 17\nside\nO::__construct(): Argument #1 ($s) must be of type string, null given, called in F on line 18\nside\nO::m(): Argument #1 ($s) must be of type string, null given, called in F on line 19\nside\nO::p(): Argument #1 ($s) must be of type string, null given, called in F on line 20\nside\nm:ok\npure(): Argument #1 ($s) must be of type string, null given, called in F on line 22\n");
}

/// A runtime unpack binds parameters in PHP's order: a parameter skipped before a bound one is
/// "not passed" before anything is checked, then each parameter is either missing (PHP's "Too
/// few arguments" message, with the passed count) or checked against its declared scalar, so a
/// `TypeError` for an earlier parameter wins over a later missing one. Before, every missing
/// parameter threw a generic `ArgumentCountError` ahead of all the parameter checks.
#[test]
fn test_runtime_unpack_checks_arity_and_types_in_parameter_order() {
    let out = compile_and_run(
        r#"<?php
function box(mixed $v): mixed { return $v; }
function show(Throwable $e): void { echo get_class($e), ": ", str_replace(__FILE__, "F", $e->getMessage()), "\n"; }
function f(string $s, int $n, string $t) { return "ok"; }
function g(int $n, string $s) { return "g:" . $n . $s; }
class C { public function m(string $s, int $n) { return "m"; } public static function s(int $n, string $s) { return "s"; } }
$v = box(null);
$c = new C();
try { f($v, ...box([])); } catch (Throwable $e) { show($e); }
try { f("x", ...box([])); } catch (Throwable $e) { show($e); }
try { f($v, ...box([1])); } catch (Throwable $e) { show($e); }
try { f("x", ...box([$v])); } catch (Throwable $e) { show($e); }
try { f(...box(['s' => $v])); } catch (Throwable $e) { show($e); }
try { f(...box(['n' => 1, 't' => $v])); } catch (Throwable $e) { show($e); }
try { f(...box(['s' => "a", 't' => $v])); } catch (Throwable $e) { show($e); }
try { g(...box([1 => $v])); } catch (Throwable $e) { show($e); }
try { echo g(...box(["7", "x"])), "\n"; } catch (Throwable $e) { show($e); }
try { $c->m($v, ...box([])); } catch (Throwable $e) { show($e); }
"#,
    );
    assert_eq!(out, "TypeError: f(): Argument #1 ($s) must be of type string, null given, called in F on line 9\nArgumentCountError: Too few arguments to function f(), 1 passed in F on line 10 and exactly 3 expected\nTypeError: f(): Argument #1 ($s) must be of type string, null given, called in F on line 11\nTypeError: f(): Argument #2 ($n) must be of type int, null given, called in F on line 12\nTypeError: f(): Argument #1 ($s) must be of type string, null given, called in F on line 13\nArgumentCountError: f(): Argument #1 ($s) not passed\nArgumentCountError: f(): Argument #2 ($n) not passed\nTypeError: g(): Argument #1 ($n) must be of type int, null given, called in F on line 16\ng:7x\nTypeError: C::m(): Argument #1 ($s) must be of type string, null given, called in F on line 18\n");
}

/// A guard that throws after the other arguments were evaluated must not leak them: the
/// freshly built array bound next to a rejected `int`/`string` argument (and the rejected box
/// itself when it is a call result) used to stay live forever, because the deferred check ran
/// before those operands were published for exception cleanup.
#[test]
fn test_guard_throw_releases_the_evaluated_arguments() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
function box(mixed $v): mixed { return $v; }
function ti(int $i, array $a): int { return 1; }
function tf(array $a, float $f, string $s): int { return 1; }
function ts(string $s, array $a): int { return 1; }
class O { public function m(string $s, array $a): int { return 1; } }
$o = new O();
$null = box(null);
for ($i = 0; $i < 3; $i++) {
    try { ti($null, [$i, str_repeat("a", 40 + $i)]); } catch (TypeError $e) { echo "i"; }
    try { ti(box(null), [str_repeat("b", 40)]); } catch (TypeError $e) { echo "b"; }
    try { tf([str_repeat("c", 40)], box([1]), str_repeat("d", 30)); } catch (TypeError $e) { echo "f"; }
    try { ts(box([2]), [str_repeat("e", 40)]); } catch (TypeError $e) { echo "s"; }
    try { $o->m($null, [str_repeat("g", 40)]); } catch (TypeError $e) { echo "m"; }
    echo ti(box("5"), [str_repeat("h", 40)]), "\n";
}
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "ibfsm1\nibfsm1\nibfsm1\n");
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// A rejected prefix before a runtime spread must not leak the spread array: the last argument
/// is never rooted while arguments are evaluated, so a fresh array from `side()` stayed live on
/// every caught `TypeError` (review of the runtime-spread prefix check).
#[test]
fn test_rejected_spread_call_releases_the_spread_array() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
function box(mixed $v): mixed { return $v; }
function side(): array { return [str_repeat("x", 40), "yy"]; }
class O { public function m(string $s, string ...$rest): int { return 1; } public static function s(string $s, string ...$r): int { return 1; } }
function f(string $s, string ...$r): int { return 1; }
$o = new O();
$null = box(null);
$n = 0;
for ($i = 0; $i < 3; $i++) {
    try { $o->m($null, ...side()); } catch (TypeError $e) { $n++; }
    try { O::s($null, ...side()); } catch (TypeError $e) { $n++; }
    try { f($null, ...side()); } catch (TypeError $e) { $n++; }
}
echo $n, "\n";
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "9\n");
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// A `Stringable` accepted by a deferred `string` check is converted after every argument was
/// evaluated; when its `__toString()` throws, the box (a call result or a local) and the other
/// owned operands must still be released by the unwinder, for functions and methods and
/// whether the throwing conversion comes first or after an earlier string was converted.
#[test]
fn test_throwing_to_string_in_a_deferred_string_check_releases_the_box() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
class Boom {
    public function __construct(private string $tag) {}
    public function __toString(): string { throw new RuntimeException("boom " . $this->tag); }
}
class O { public function m(string $s, array $a): int { return 1; } }
function box(mixed $v): mixed { return $v; }
function f(string $s): int { return strlen($s); }
function g(string $a, string $b, array $c): int { return 1; }
$o = new O();
$n = 0;
for ($i = 0; $i < 3; $i++) {
    $v = box(new Boom(str_repeat("t", 40 + ($i & 3))));
    try { f($v); } catch (RuntimeException $e) { $n++; }
    try { f(box(new Boom(str_repeat("u", 40)))); } catch (RuntimeException $e) { $n++; }
    try { g(box(new Boom("a")), box(str_repeat("v", 40)), [str_repeat("w", 40)]); } catch (RuntimeException $e) { $n++; }
    try { g(box(str_repeat("x", 40)), box(new Boom("b")), [str_repeat("y", 40)]); } catch (RuntimeException $e) { $n++; }
    try { $o->m(box(new Boom("c")), [str_repeat("z", 40)]); } catch (RuntimeException $e) { $n++; $last = $e->getMessage(); }
}
echo $n, " ", $last, "\n";
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "15 boom c\n");
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// PHP converts an accepted `string` argument while binding it, so in a runtime unpack a
/// `Stringable`'s `__toString()` runs (and its exception wins) before a later parameter is
/// checked or found missing. The in-order unpack check used to accept the object and only
/// convert it after every parameter was checked, skipping `__toString()` whenever a later
/// parameter threw (review of the unpack order).
#[test]
fn test_runtime_unpack_converts_a_stringable_before_the_next_parameter() {
    let out = compile_and_run(
        r#"<?php
function box(mixed $v): mixed { return $v; }
function show(Throwable $e): void { echo get_class($e), ": ", str_replace(__FILE__, "F", $e->getMessage()), "\n"; }
class Name { public function __toString(): string { echo "toString\n"; return "name"; } }
class Bad { public function __toString(): string { throw new RuntimeException("from toString"); } }
function f(string $s, int $n) { return "f:" . $s . $n; }
function g(int $n, string $s, int $m) { return "g"; }
try { echo f(...box([new Name(), null])), "\n"; } catch (Throwable $e) { show($e); }
try { echo f(...box([new Bad(), null])), "\n"; } catch (Throwable $e) { show($e); }
try { echo f(...box([new Name(), 3])), "\n"; } catch (Throwable $e) { show($e); }
try { echo g(...box([1, new Name()])), "\n"; } catch (Throwable $e) { show($e); }
try { echo f(box(new Bad()), box(null)), "\n"; } catch (Throwable $e) { show($e); }
"#,
    );
    assert_eq!(out, "toString\nTypeError: f(): Argument #2 ($n) must be of type int, null given, called in F on line 8\nRuntimeException: from toString\ntoString\nf:name3\ntoString\nArgumentCountError: Too few arguments to function g(), 2 passed in F on line 11 and exactly 3 expected\nRuntimeException: from toString\n");
}

/// The string a runtime unpack converts a `Stringable` to must still be owned when the object
/// itself is released: the object (and the spread source holding its last reference) used to be
/// released after the operands were acquired, so a throwing destructor leaked the converted
/// string on every caught call (review of the in-order unpack conversion).
#[test]
fn test_runtime_unpack_throwing_destructor_releases_the_converted_string() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
class S {
    public function __toString(): string { return str_repeat("s", 40); }
    public function __destruct() { throw new RuntimeException("dtor"); }
}
class N { public function __toString(): string { return str_repeat("n", 30); } }
function box(mixed $v): mixed { return $v; }
function mk(): mixed { return [new S(), 1]; }
function f(string $s, int $n): string { return strlen($s) . ":" . $n; }
function g(int $a, string $s, string $t): string { return $a . strlen($s) . strlen($t); }
for ($i = 0; $i < 3; $i++) {
    try { echo f(...mk()), "\n"; } catch (RuntimeException $e) { echo "caught: ", $e->getMessage(), "\n"; }
    try { echo g(7, ...box([new S(), "tt"])), "\n"; } catch (RuntimeException $e) { echo "caught: ", $e->getMessage(), "\n"; }
    try { echo f(...box(['n' => 5, 's' => new S()])), "\n"; } catch (RuntimeException $e) { echo "caught: ", $e->getMessage(), "\n"; }
    echo f(...box([new N(), 3])), "\n";
}
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "caught: dtor\ncaught: dtor\ncaught: dtor\n30:3\n".repeat(3));
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// Calls through a callable value bind their arguments in the shared runtime invoker, which
/// never checked declared scalar parameters: `call_user_func_array()`, array callables in
/// `call_user_func()`, and callbacks invoked by builtins (`array_map()`, `usort()`,
/// `array_filter()`, `array_reduce()`) bound `null` as `""`/`0` and an object as a string. They
/// now throw PHP's `TypeError`, naming functions, methods and closures (`{closure:file:line}`) as
/// PHP does, and a builtin's typed callback keeps its `catch` reachable.
#[test]
fn test_calls_through_callable_values_check_scalar_parameters() {
    let out = compile_and_run(
        r#"<?php
function box(mixed $v): mixed { return $v; }
// PHP adds ", called in <file> on line <n>" for a call made from user code; elephc omits it
// on the runtime invoker path, so only the part before it is compared.
function show(Throwable $e): void { echo explode(", called in", str_replace(__FILE__, "F", $e->getMessage()))[0], "\n"; }
function named(string $s): string { return "n:" . $s; }
function cmp(int $x, int $y): int { return $x <=> $y; }
function keep(string $s): bool { return true; }
class O { public function m(string $s): string { return "m:" . $s; } public static function st(float $f): float { return $f; } }
$unused = function () { return 1; };
$null = box(null);
$c = named(...);
$f = function (string $s) { return $s; };
try { echo call_user_func_array($c, [$null]), "\n"; } catch (TypeError $e) { show($e); }
try { echo call_user_func([new O, 'm'], $null), "\n"; } catch (TypeError $e) { show($e); }
try { echo call_user_func(['O', 'st'], box([])), "\n"; } catch (TypeError $e) { show($e); }
try { echo call_user_func_array($f, ['s' => box(new O)]), "\n"; } catch (TypeError $e) { show($e); }
try { array_map($f, [box(null)]); } catch (TypeError $e) { show($e); }
try { array_map($c, ["a", box(new O)]); } catch (TypeError $e) { show($e); }
$a = [box(null), 1];
try { usort($a, fn(int $x, int $y) => 0); echo "usort closure no\n"; } catch (TypeError $e) { show($e); }
$a = [box(null), 1];
try { usort($a, 'cmp'); echo "usort named no\n"; } catch (TypeError $e) { show($e); }
try { array_filter([box(null)], 'keep'); echo "filter no\n"; } catch (TypeError $e) { show($e); }
try { array_filter([box([])], fn(string $s) => true); echo "filter closure no\n"; } catch (TypeError $e) { show($e); }
try { echo array_reduce([box("x")], fn(int $c, int $i) => $c + $i, 0), "\n"; } catch (TypeError $e) { show($e); }
echo implode(",", array_map(fn(int $i) => $i * 2, [box("3"), box(4.0), box(true)])), " ", call_user_func_array($c, [box(5)]), "\n";
"#,
    );
    assert_eq!(out, "named(): Argument #1 ($s) must be of type string, null given\nO::m(): Argument #1 ($s) must be of type string, null given\nO::st(): Argument #1 ($f) must be of type float, array given\n{closure:F:13}(): Argument #1 ($s) must be of type string, O given\n{closure:F:13}(): Argument #1 ($s) must be of type string, null given\nnamed(): Argument #1 ($s) must be of type string, O given\n{closure:F:21}(): Argument #1 ($x) must be of type int, null given\ncmp(): Argument #1 ($x) must be of type int, null given\nkeep(): Argument #1 ($s) must be of type string, null given\n{closure:F:25}(): Argument #1 ($s) must be of type string, array given\n{closure:F:26}(): Argument #2 ($i) must be of type int, string given\n6,8,2 n:5\n");
}

/// Through the invoker, every declared scalar applies PHP's coercive rules to a boxed value
/// like a direct call: numeric strings, the float precision deprecation, the int range check,
/// `Stringable` objects for `string`, and the type names in the message (`true`, class names).
#[test]
fn test_invoker_scalar_parameters_follow_php_coercion() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function ($no, $msg) { echo "[diag] ", $msg, "\n"; return true; });
function box(mixed $v): mixed { return $v; }
class Name { public function __toString(): string { return "name"; } }
class Plain {}
function str(string $s): string { return "s:" . $s; }
function num(int $i): string { return "i:" . $i; }
function real(float $f): string { return "f:" . $f; }
function flag(bool $b): string { return $b ? "b:T" : "b:F"; }
$values = [null, [1], new Plain(), new Name(), true, false, 3, 2.5, "12", " 7 ", "12abc", "1e3", "x", 1e30, "9223372036854775807"];
foreach (['str', 'num', 'real', 'flag'] as $fn) {
    foreach ($values as $v) {
        try { echo implode(",", array_map($fn, [box($v)])), "\n"; }
        catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    }
}
"#,
    );
    assert_eq!(out, "str(): Argument #1 ($s) must be of type string, null given\nstr(): Argument #1 ($s) must be of type string, array given\nstr(): Argument #1 ($s) must be of type string, Plain given\ns:name\ns:1\ns:\ns:3\ns:2.5\ns:12\ns: 7 \ns:12abc\ns:1e3\ns:x\ns:1.0E+30\ns:9223372036854775807\nnum(): Argument #1 ($i) must be of type int, null given\nnum(): Argument #1 ($i) must be of type int, array given\nnum(): Argument #1 ($i) must be of type int, Plain given\nnum(): Argument #1 ($i) must be of type int, Name given\ni:1\ni:0\ni:3\n[diag] Implicit conversion from float 2.5 to int loses precision\ni:2\ni:12\ni:7\nnum(): Argument #1 ($i) must be of type int, string given\ni:1000\nnum(): Argument #1 ($i) must be of type int, string given\nnum(): Argument #1 ($i) must be of type int, float given\ni:9223372036854775807\nreal(): Argument #1 ($f) must be of type float, null given\nreal(): Argument #1 ($f) must be of type float, array given\nreal(): Argument #1 ($f) must be of type float, Plain given\nreal(): Argument #1 ($f) must be of type float, Name given\nf:1\nf:0\nf:3\nf:2.5\nf:12\nf:7\nreal(): Argument #1 ($f) must be of type float, string given\nf:1000\nreal(): Argument #1 ($f) must be of type float, string given\nf:1.0E+30\nf:9.2233720368548E+18\nflag(): Argument #1 ($b) must be of type bool, null given\nflag(): Argument #1 ($b) must be of type bool, array given\nflag(): Argument #1 ($b) must be of type bool, Plain given\nflag(): Argument #1 ($b) must be of type bool, Name given\nb:T\nb:F\nb:T\nb:T\nb:T\nb:T\nb:T\nb:T\nb:T\nb:T\nb:T\n");
}

/// PHP raises the `int` precision deprecation while binding the parameter, so it reports the
/// PARAMETER's line (where its type starts, after any attribute), not the call's: for functions,
/// methods, static methods with defaulted parameters and closures, with calls and declarations
/// spread over several lines. The guard used to report the call's line.
#[test]
fn test_int_precision_deprecation_reports_the_parameter_line() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function ($no, $msg, $file, $line) { echo "[$line] $msg\n"; return true; });
function box(mixed $v): mixed { return $v; }
function ti(
    string $pad,
    #[SensitiveParameter]
    int
    $i
): int { return $i; }
function one(int $i): int { return $i; }
class C { public function m(
  int $i) { return $i; } public static function s(string $a, int $b = 2,
    int $c = 3) { return $c; } }
echo ti(
    "x",
    box("1.5")
), "\n";
echo one(box(2.5)), "\n";
echo (new C)->m(
   box("3.5")), "\n";
echo C::s("a", box(5.5), box(6.5)), "\n";
$f = function (
  int $i) { return $i; };
echo $f(box(4.5)), "\n";
"#,
    );
    assert_eq!(out, "[7] Implicit conversion from float-string \"1.5\" to int loses precision\n1\n[10] Implicit conversion from float 2.5 to int loses precision\n2\n[12] Implicit conversion from float-string \"3.5\" to int loses precision\n3\n[12] Implicit conversion from float 5.5 to int loses precision\n[13] Implicit conversion from float 6.5 to int loses precision\n6\n[23] Implicit conversion from float 4.5 to int loses precision\n4\n");
}

/// With several deferred guards in one call, a later guard's `TypeError` must release what the
/// earlier ones left: the fresh array bound to a second `int` (the last argument is never rooted
/// while arguments are evaluated), and the string an earlier `string` guard's conversion made
/// (a `Stringable`'s `__toString()` result or a converted box). Each caught call leaked them
/// (review on the return-guard follow-up).
#[test]
fn test_guard_throw_after_an_earlier_guard_releases_its_operands() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
function box(mixed $v): mixed { return $v; }
function make(): mixed { return [str_repeat("a", 40), 2]; }
function t(int $a, int $b): int { return $a + $b; }
function u(string $s, int $b): int { return strlen($s) + $b; }
class Name { public function __toString(): string { return str_repeat("n", 30); } }
$n = 0;
for ($i = 0; $i < 3; $i++) {
    try { t(box(null), make()); } catch (TypeError $e) { $n++; }
    try { u(box(new Name()), make()); } catch (TypeError $e) { $n++; }
    try { u(box(str_repeat("s", 20)), box(null)); } catch (TypeError $e) { $n++; }
    $n += t(box(1), box(2)) + u(box(new Name()), box("4"));
}
echo $n, "\n";
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "120\n");
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// PHP evaluates every argument, the spread source included, before it checks any parameter:
/// a refused prefix before a runtime spread must not skip the spread's side effects, nor hide
/// an exception it throws. The prefix was checked before the spread was read (review on the
/// spread-prefix follow-up).
#[test]
fn test_positional_spread_prefix_is_checked_after_the_spread_is_evaluated() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
function f(string $s, int $n, int $a, int $b): string { return $s . $n . $a . $b; }
function box(mixed $v): mixed { return $v; }
function side(): int { echo "side\n"; return 0; }
function boom(): int { throw new RuntimeException("boom"); }
function show(Throwable $e): void { echo get_class($e), ": ", str_replace(__FILE__, "F", $e->getMessage()), "\n"; }
$null = box(null);
$lists = [[1, 2]];
for ($i = 0; $i < 2; $i++) {
    try { echo f($null, box(1), ...$lists[side()]), "\n"; } catch (Throwable $e) { show($e); }
    try { echo f($null, box(1), ...$lists[boom()]), "\n"; } catch (Throwable $e) { show($e); }
    try { echo f(box("x"), box("q"), ...$lists[side()]), "\n"; } catch (Throwable $e) { show($e); }
    echo f(box("x"), box(" 7"), ...$lists[side()]), "\n";
}
"#,
    );
    assert!(output.success, "{}", output.stderr);
    let round = "side\nTypeError: f(): Argument #1 ($s) must be of type string, null given, called in F on line 10\nRuntimeException: boom\nside\nTypeError: f(): Argument #2 ($n) must be of type int, string given, called in F on line 12\nside\nx712\n";
    assert_eq!(output.stdout, round.repeat(2));
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// The string a deferred prefix check converts (here a `Stringable`'s fresh `__toString()`
/// result) must be rooted before the spread elements and omitted defaults are lowered: a
/// throwing default leaked it on every caught call (review on the spread-prefix follow-up).
#[test]
fn test_throwing_default_after_a_spread_prefix_releases_the_converted_string() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
class Name { public function __toString(): string { return str_repeat("n", 30); } }
function box(mixed $v): mixed { return $v; }
function f(string $s, int $a, int $b, int $c = 1 % 0): string { return $s . $a . $b . $c; }
$rows = [[1, 2], [3, 4]];
for ($i = 0; $i < 3; $i++) {
    try { echo f(box(new Name()), ...$rows[$i & 1]), "\n"; } catch (Error $e) { echo "caught: ", $e->getMessage(), "\n"; }
}
echo f(box("ok"), 1, 2, 3), "\n";
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "caught: Modulo by zero\ncaught: Modulo by zero\ncaught: Modulo by zero\nok123\n");
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// The `int` precision deprecation can reach a user error handler that throws. The exception
/// must reach the `catch` (the call's catch flow was narrowed to `TypeError`, so a
/// `catch (ErrorException)` was dropped) and must not leak the owned arguments: the converted
/// box and the later arguments only the guard's own refusal released.
#[test]
fn test_error_handler_exception_from_a_guard_is_caught_without_leaks() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
function take(int $n): int { return $n; }
function two(int $a, string $s): string { return $a . $s; }
function box(mixed $v): mixed { return $v; }
function words(): mixed { return str_repeat("w", 30); }
set_error_handler(function (int $no, string $msg): bool { throw new ErrorException($msg); });
for ($i = 0; $i < 3; $i++) {
    try { echo take(box(1.5)), "\n"; } catch (ErrorException $e) { echo "caught: ", $e->getMessage(), "\n"; }
    try { echo two(box(2.5), words()), "\n"; } catch (ErrorException $e) { echo "caught: ", $e->getMessage(), "\n"; }
    try { echo two(box("3.5"), box(words())), "\n"; } catch (ErrorException $e) { echo "caught: ", $e->getMessage(), "\n"; }
}
restore_error_handler();
echo take(box(4)), "\n";
"#,
    );
    assert!(output.success, "{}", output.stderr);
    let round = "caught: Implicit conversion from float 1.5 to int loses precision\ncaught: Implicit conversion from float 2.5 to int loses precision\ncaught: Implicit conversion from float-string \"3.5\" to int loses precision\n";
    assert_eq!(output.stdout, format!("{}4\n", round.repeat(3)));
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// A guarded `string` parameter can call a `Stringable`'s `__toString()`, which may throw any
/// exception: the call's catch flow must keep a `catch` for it, not only `catch (TypeError)`.
#[test]
fn test_stringable_exception_from_a_guarded_string_parameter_is_caught() {
    let out = compile_and_run(
        r#"<?php
function str(string $s): string { return $s; }
function box(mixed $v): mixed { return $v; }
class Bad { public function __toString(): string { throw new LogicException("bad"); } }
try { echo str(box(new Bad())), "\n"; } catch (LogicException $e) { echo "caught: ", $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(out, "caught: bad\n");
}

/// The `int` precision deprecation names the file that declares the parameter: a function or
/// method an included file declares reported the entry script (and the call's line, as the
/// included function is lowered under a generated name).
#[test]
fn test_int_precision_deprecation_names_the_included_declaring_file() {
    let out = compile_and_run_files(
        &[
            (
                "defs.php",
                r#"<?php
namespace Lib;

class C {
    public function m(
        string $s,
        int $n
    ): int { return $n; }
}

function f(
    int $a
): int { return $a; }
"#,
            ),
            (
                "main.php",
                r#"<?php
require __DIR__ . '/defs.php';
function box(mixed $v): mixed { return $v; }
set_error_handler(function (int $no, string $msg, string $file, int $line): bool {
    echo basename($file), ":", $line, " ", $msg, "\n";
    return true;
});
echo (new Lib\C())->m("x", box(1.5)), "\n";
echo Lib\f(box("2.5")), "\n";
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "defs.php:7 Implicit conversion from float 1.5 to int loses precision\n1\ndefs.php:12 Implicit conversion from float-string \"2.5\" to int loses precision\n2\n");
}

/// PHP binds the supplied parameters before it evaluates an omitted default: a refused argument
/// throws `TypeError` before a throwing default (`int $n = 1 % 0`) runs. The default ran first
/// and its `DivisionByZeroError` won. When the check accepts, the string it converted and an
/// earlier default (`array $l = []`) must not leak when the later default throws (review on
/// the deferred positional checks).
#[test]
fn test_supplied_parameters_are_checked_before_omitted_defaults() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
class Name { public function __toString(): string { return str_repeat("n", 30); } }
function box(mixed $v): mixed { return $v; }
function f(string $s, int $n = 1 % 0): string { return $s . $n; }
function g(int $a, string $s, array $l = [], int $n = 1 % 0): string { return $a . $s; }
function show(Throwable $e): void { echo get_class($e), ": ", str_replace(__FILE__, "F", $e->getMessage()), "\n"; }
for ($i = 0; $i < 2; $i++) {
    try { echo f(box(null)), "\n"; } catch (Throwable $e) { show($e); }
    try { echo f(box(new Name())), "\n"; } catch (Throwable $e) { show($e); }
    try { echo g(box(1), box([1])), "\n"; } catch (Throwable $e) { show($e); }
    try { echo g(box(1), box(new Name())), "\n"; } catch (Throwable $e) { show($e); }
}
"#,
    );
    assert!(output.success, "{}", output.stderr);
    let round = "TypeError: f(): Argument #1 ($s) must be of type string, null given, called in F on line 8\nDivisionByZeroError: Modulo by zero\nTypeError: g(): Argument #2 ($s) must be of type string, array given, called in F on line 10\nDivisionByZeroError: Modulo by zero\n";
    assert_eq!(output.stdout, round.repeat(2));
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// An accepted `string` check converts at once, and a `Stringable`'s `__toString()` can throw
/// before the next parameter is checked. The other unpublished arguments (a fresh array from
/// `make()`, the operand's own box, an earlier converted string) were released only by a
/// guard's refusal, so each caught call leaked them (review on the deferred string conversion).
#[test]
fn test_throwing_string_conversion_releases_the_other_arguments() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
class Bad { public function __toString(): string { throw new RuntimeException("bad"); } }
class Name { public function __toString(): string { return str_repeat("n", 30); } }
function box(mixed $v): mixed { return $v; }
function make(): mixed { return [str_repeat("a", 40), 2]; }
function u(string $s, int $b): int { return strlen($s) + $b; }
function v(string $a, string $b, int $c): int { return 1; }
$bad = box(new Bad());
$n = 0;
for ($i = 0; $i < 3; $i++) {
    try { u($bad, make()); } catch (RuntimeException $e) { $n++; }
    try { u(box(new Bad()), make()); } catch (RuntimeException $e) { $n++; }
    try { v(box(new Name()), $bad, make()); } catch (RuntimeException $e) { $n++; }
    try { v(box(str_repeat("s", 20)), box(new Bad()), box(3)); } catch (RuntimeException $e) { $n++; }
    $n += u(box(new Name()), box("4")) + v(box(new Name()), box("x"), box(5));
}
echo $n, "\n";
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "117\n");
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// PHP names the class that declares the method, not the receiver's: `A::m()` for an inherited
/// `B::m`, `A::__construct()` for an inherited constructor, and the using class for a trait
/// method. The `int` precision deprecation reports the parameter's own line, which an inherited
/// or trait method's parameters only had under the declaring name (review: the guard named the
/// receiver class and the deprecation fell back to the call line).
#[test]
fn test_inherited_and_trait_methods_are_named_by_their_declaring_class() {
    let out = compile_and_run(
        r#"<?php
function box(mixed $v): mixed { return $v; }
trait T {
    public function viaTrait(
        int $n
    ): int { return $n; }
}
class A {
    public function __construct(string $s = "") {}
    public function m(string $s): string { return $s; }
    public static function st(string $s): string { return $s; }
    public function n(
        int $n
    ): int { return $n; }
}
class B extends A { use T; }
function show(Throwable $e): void { echo str_replace(__FILE__, "F", $e->getMessage()), "\n"; }
set_error_handler(function (int $no, string $msg, string $file, int $line): bool { echo "[$line] $msg\n"; return true; });
$null = box(null);
$b = new B();
try { $b->m($null); } catch (TypeError $e) { show($e); }
try { B::st($null); } catch (TypeError $e) { show($e); }
try { new B($null); } catch (TypeError $e) { show($e); }
try { $b->viaTrait($null); } catch (TypeError $e) { show($e); }
echo $b->n(box(1.5)), "\n";
echo $b->viaTrait(box(2.5)), "\n";
"#,
    );
    assert_eq!(
        out,
        "A::m(): Argument #1 ($s) must be of type string, null given, called in F on line 21\n\
A::st(): Argument #1 ($s) must be of type string, null given, called in F on line 22\n\
A::__construct(): Argument #1 ($s) must be of type string, null given, called in F on line 23\n\
B::viaTrait(): Argument #1 ($n) must be of type int, null given, called in F on line 24\n\
[13] Implicit conversion from float 1.5 to int loses precision\n1\n\
[5] Implicit conversion from float 2.5 to int loses precision\n2\n"
    );
}

/// Closures with the same signature share one descriptor invoker: the `TypeError` reads the
/// closure's label (`{closure:file:line}`) from its descriptor. An invoker per closure doubled
/// the binary of a program with 200 `fn(int $x)` callbacks (review on the invoker cache key).
#[test]
fn test_closures_with_one_signature_share_their_invoker() {
    let source = r#"<?php
$fs = [fn(int $x) => $x + 1, fn(int $x) => $x + 2, fn(int $x) => $x + 3];
$t = 0;
foreach ($fs as $f) { $t += $f(1); }
try { array_map($fs[1], [null]); } catch (TypeError $e) { echo str_replace(__FILE__, "F", $e->getMessage()), "\n"; }
echo $t, "\n";
"#;
    let out = compile_and_run(source);
    assert_eq!(out, "{closure:F:2}(): Argument #1 ($x) must be of type int, null given\n9\n");
    // As many invoker bodies as with a single closure of that signature.
    let invoker_bodies = |source: &str, name: &str| {
        let dir = make_cli_test_dir(name);
        let (user_asm, _, _) = compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
        user_asm.lines()
            .filter(|line| line.contains("callable_invoker_") && !line.contains("__") && line.ends_with(':'))
            .count()
    };
    let single = source.replace("fn(int $x) => $x + 2, fn(int $x) => $x + 3", "");
    assert_eq!(
        invoker_bodies(source, "elephc_shared_closure_invokers"),
        invoker_bodies(&single, "elephc_single_closure_invoker"),
    );
}

/// A constant default can raise `E_DEPRECATED` (`MT_RAND_PHP`), and a user error handler can
/// turn that into an exception after the supplied parameters were checked. The string the
/// check converted must already be rooted then: treating every constant read as unable to throw
/// leaked it on each caught call (review on the constant-default shortcut).
#[test]
fn test_deprecated_constant_default_releases_the_converted_string() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
class Name { public function __toString(): string { return str_repeat("n", 30); } }
function box(mixed $v): mixed { return $v; }
function f(string $s, int $a = MT_RAND_PHP): string { return $s . $a; }
set_error_handler(function (int $no, string $msg): bool { throw new ErrorException($msg); });
for ($i = 0; $i < 3; $i++) {
    try { echo f(box(new Name())), "\n"; } catch (ErrorException $e) { echo "caught\n"; }
}
restore_error_handler();
echo f(box("ok"), 1), "\n";
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "caught\ncaught\ncaught\nok1\n");
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// An anonymous class's method (or constructor) is named by the class alone, as PHP prints it:
/// `class@anonymous()`, `Parent@anonymous()`, `Interface@anonymous()`, never the generated
/// `class@anonymous#N::m` symbol; an inherited named method keeps its declaring class.
#[test]
fn test_scalar_param_error_names_an_anonymous_class_like_php() {
    let out = compile_and_run(
        r#"<?php
function box(mixed $v): mixed { return $v; }
function show(TypeError $x): void { echo str_replace(__FILE__, "F", $x->getMessage()), "\n"; }
class Base {}
interface I {}
class T { public function q(int $n): void {} }
$c = new class { public function p(string $s): void {} };
$d = new class extends Base { public function p(int $i): void {} };
$e = new class implements I { public function p(bool $b): void {} };
$f = new class extends T {};
try { $c->p(box(null)); } catch (TypeError $x) { show($x); }
try { $d->p(box(null)); } catch (TypeError $x) { show($x); }
try { $e->p(box([])); } catch (TypeError $x) { show($x); }
try { $f->q(box(null)); } catch (TypeError $x) { show($x); }
try { new class(box(null)) { public function __construct(float $f) {} }; } catch (TypeError $x) { show($x); }
"#,
    );
    assert_eq!(out, "class@anonymous(): Argument #1 ($s) must be of type string, null given, called in F on line 11\nBase@anonymous(): Argument #1 ($i) must be of type int, null given, called in F on line 12\nI@anonymous(): Argument #1 ($b) must be of type bool, array given, called in F on line 13\nT::q(): Argument #1 ($n) must be of type int, null given, called in F on line 14\nclass@anonymous(): Argument #1 ($f) must be of type float, null given, called in F on line 15\n");
}

/// An object of a class `eval()` declares is backed by a native `stdClass`, so the callable
/// invoker's static `Stringable` check missed its `__toString()` and refused it; it now asks the
/// eval bridge, as the direct-call guard does, and binds the object to `string`. (The value a
/// builtin callback then receives is converted without the bridge, as on main: issue #2429.)
#[test]
fn test_callable_invoker_binds_an_eval_declared_stringable_to_string() {
    let out = compile_and_run(
        r#"<?php
eval('class EV { public function __toString(): string { return "ev"; } }');
$o = new EV();
function s(string $s): string { return "[$s]"; }
echo count(array_map('s', [$o])), "\n";
echo array_sum(array_map(fn(string $s): int => 1, [$o, $o])), "\n";
echo call_user_func('s', $o), "\n";
echo s($o), "\n";
"#,
    );
    assert_eq!(out, "1\n2\n[ev]\n[ev]\n");
}

/// A refused argument whose release runs a throwing destructor does not leak the `TypeError`
/// message (see `test_refused_argument_destructor_throws_after_the_type_error` for the
/// exception order).
#[test]
fn test_guard_refusal_with_throwing_destructor_does_not_leak_the_message() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
class D { public function __destruct() { throw new LogicException("d"); } }
function mk(): mixed { return new D(); }
function p(string $s): void {}
for ($i = 0; $i < 3; $i++) {
    try { p(mk()); echo "bound\n"; } catch (Throwable $e) { echo "caught\n"; }
}
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "caught\ncaught\ncaught\n");
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// The Composer classmap scan parses a file only to index its classes, under the entry file's
/// source identity; a one-line function it declares at the same line and column used to replace
/// the entry function's parameter lines, so the `int` precision deprecation named line 2
/// instead of the parameter's line 3.
#[test]
fn test_classmap_scan_keeps_the_entry_parameter_lines() {
    let out = compile_and_run_files(
        &[
            ("composer.json", r#"{"autoload":{"classmap":["lib/"]}}"#),
            (
                "lib/Unused.php",
                "<?php\nfunction unused_g(int $x): int { return $x; }\nclass Unused {}\n",
            ),
            (
                "main.php",
                r#"<?php
function f(
    int $n
): int { return $n; }
function box(mixed $v): mixed { return $v; }
set_error_handler(function (int $no, string $msg, string $file, int $line): bool { echo basename($file), ":", $line, " ", $msg, "\n"; return true; });
echo f(box(1.5)), "\n";
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "main.php:3 Implicit conversion from float 1.5 to int loses precision\n1\n");
}

/// The `int` precision deprecation a guarded binding raises runs the user error handler before
/// the callee, so a global it writes must not be read back from a value the optimizer saved
/// before the call: an empty `take(int $n)` used to leave `$x` at 1. The method, static-method,
/// static-property and captured-by-reference cases already invalidated and stay correct.
#[test]
fn test_guard_error_handler_writes_are_seen_after_the_call() {
    let out = compile_and_run(
        r#"<?php
function box(mixed $v): mixed { return $v; }
function take(int $n): void {}
class K { public function m(int $n): void {} public static function s(int $n): void {} public static int $p = 0; }
set_error_handler('handler');
$v = box(1.5);
$x = 1;
take($v);
echo $x, "\n";
$y = 1;
(new K())->m($v);
echo $y, "\n";
K::$p = 1;
K::s($v);
echo K::$p, "\n";
function inner(mixed $v): void {
    $z = 1;
    set_error_handler(function (int $no, string $msg) use (&$z): bool { $z = 2; return true; });
    take($v);
    echo $z, "\n";
}
inner(box(2.5));
function handler(int $no, string $msg): bool { global $x, $y; $x = 2; $y = 2; K::$p = 2; return true; }
"#,
    );
    assert_eq!(out, "2\n2\n2\n2\n");
}

/// A supplied named argument is checked before an omitted default that can throw is
/// evaluated, as PHP binds supplied parameters first: `f(s: box(null))` with
/// `int $n = 1 % 0` threw `DivisionByZeroError` instead of the `TypeError` for `$s`.
#[test]
fn test_named_call_checks_supplied_arguments_before_throwing_defaults() {
    let out = compile_and_run(
        r#"<?php
function box(mixed $v): mixed { return $v; }
function show(Throwable $e): void { echo get_class($e), ": ", str_replace(__FILE__, "F", $e->getMessage()), "\n"; }
function f(string $s, int $n = 1 % 0) {}
class C { public function m(string $s, int $n = 1 % 0) {} public static function sm(int $i, int $n = 1 % 0) {} }
try { f(s: box(null)); } catch (Throwable $e) { show($e); }
try { f(box(null)); } catch (Throwable $e) { show($e); }
try { (new C())->m(s: box(null)); } catch (Throwable $e) { show($e); }
try { C::sm(i: box("x")); } catch (Throwable $e) { show($e); }
try { f(n: 2, s: box([])); } catch (Throwable $e) { show($e); }
try { f(s: box("ok")); } catch (Throwable $e) { show($e); }
"#,
    );
    assert_eq!(out, "TypeError: f(): Argument #1 ($s) must be of type string, null given, called in F on line 6\nTypeError: f(): Argument #1 ($s) must be of type string, null given, called in F on line 7\nTypeError: C::m(): Argument #1 ($s) must be of type string, null given, called in F on line 8\nTypeError: C::sm(): Argument #1 ($i) must be of type int, string given, called in F on line 9\nTypeError: f(): Argument #1 ($s) must be of type string, array given, called in F on line 10\nDivisionByZeroError: Modulo by zero\n");
}

/// A helper prelude (here the tokenizer one, whose `token_get_all` starts on its line 12) is
/// parsed under the entry file's source identity; its parameter lines replaced the entry
/// function's at the same position, so the `int` precision deprecation named line 12 instead
/// of the parameter's line 13. Only user files record parameter lines now.
#[test]
fn test_prelude_parse_keeps_the_entry_parameter_lines() {
    let out = compile_and_run(
        r#"<?php
function box(mixed $v): mixed { return $v; }
set_error_handler(function (int $no, string $msg, string $file, int $line): bool { echo $line, ' ', $msg, "\n"; return true; });
echo count(token_get_all("<?php echo 1;")), "\n";







function f(
    int $n
): int { return $n; }
echo f(box(1.5)), "\n";
"#,
    );
    assert_eq!(out, "5\n13 Implicit conversion from float 1.5 to int loses precision\n1\n");
}

/// The named-call precheck binds in parameter order as PHP does: an accepted `Stringable` runs
/// `__toString()` (and can throw from it) before a later supplied parameter is refused, even
/// when the later one is named first.
#[test]
fn test_named_call_precheck_converts_a_stringable_before_the_next_parameter() {
    let out = compile_and_run(
        r#"<?php
function box(mixed $v): mixed { return $v; }
class Name { public function __toString(): string { echo "toString ran\n"; return "n"; } }
class Bad { public function __toString(): string { throw new LogicException("from toString"); } }
function f(string $s, int $n, int $d = MT_RAND_PHP) { echo "body\n"; }
function g(string $s, int $n, int $d = 1 % 0) { echo "body\n"; }
function show(Throwable $e): void { echo get_class($e), ": ", explode(', called', $e->getMessage())[0], "\n"; }
try { f(s: box(new Name()), n: box(null)); } catch (Throwable $e) { show($e); }
try { g(s: box(new Name()), n: box(null)); } catch (Throwable $e) { show($e); }
try { g(s: box(new Bad()), n: box(null)); } catch (Throwable $e) { show($e); }
try { g(n: box(null), s: box(new Name())); } catch (Throwable $e) { show($e); }
"#,
    );
    assert_eq!(out, "toString ran\nTypeError: f(): Argument #2 ($n) must be of type int, null given\ntoString ran\nTypeError: g(): Argument #2 ($n) must be of type int, null given\nLogicException: from toString\ntoString ran\nTypeError: g(): Argument #2 ($n) must be of type int, null given\n");
}

/// A builtin's callback and a call through a callable value bind each value through the
/// callable invoker's scalar checks, whose precision deprecation runs the error handler: a
/// global it writes must not be read back from a value the optimizer saved before the call
/// (`array_map(fn(int $n) => $n, [1.5])`, `usort` and `$f($v)` kept `$x` at 1).
#[test]
fn test_callable_binding_handler_writes_are_seen_after_the_call() {
    let out = compile_and_run(
        r#"<?php
function box(mixed $v): mixed { return $v; }
function ti(int $n): int { return $n; }
set_error_handler('handler');
$v = box(1.5);
$x = 1; array_map(fn(int $n) => $n, [$v]); echo "map ", $x, "\n";
$x = 1; array_map('ti', [$v]); echo "map-named ", $x, "\n";
$x = 1; array_filter([$v], fn(int $n) => true); echo "filter ", $x, "\n";
$x = 1; $a = [$v, $v]; usort($a, fn(int $a, int $b) => 0); echo "usort ", $x, "\n";
$x = 1; call_user_func('ti', $v); echo "cuf ", $x, "\n";
$x = 1; call_user_func_array('ti', [$v]); echo "cufa ", $x, "\n";
$x = 1; array_reduce([$v], fn($c, int $n) => 0, 0); echo "reduce ", $x, "\n";
$f = 'ti'; $x = 1; $f($v); echo "var ", $x, "\n";
$g = fn(int $n) => $n; $x = 1; $g($v); echo "closure ", $x, "\n";
function handler(int $no, string $msg): bool { global $x; $x = 2; return true; }
"#,
    );
    assert_eq!(out, "map 2\nmap-named 2\nfilter 2\nusort 2\ncuf 2\ncufa 2\nreduce 2\nvar 2\nclosure 2\n");
}

/// A call through a callable value the compiler resolves statically (`$f = 'ts'; $f($v)`, a
/// `'K::sm'` string) binds through the direct-call path, but the checker records no argument
/// site for it, so its scalar checks were skipped: `null` reached `string` as `""` and a lossy
/// float reached `int` without the deprecation. They now decide from the values.
#[test]
fn test_statically_resolved_callable_values_check_scalar_parameters() {
    let out = compile_and_run(
        r#"<?php
function box(mixed $v): mixed { return $v; }
function ts(string $s): string { return "[$s]"; }
function ti(int $n): int { return $n; }
class K { public function m(string $s): string { return $s; } public static function sm(string $s): string { return $s; } }
function show(Throwable $e): void { echo str_replace(__FILE__, "F", $e->getMessage()), "\n"; }
$f = 'ts';
try { var_dump($f(box(null))); } catch (TypeError $e) { show($e); }
$g = [new K(), 'm'];
try { var_dump($g(box(null))); } catch (TypeError $e) { show($e); }
$h = 'K::sm';
try { var_dump($h(box(null))); } catch (TypeError $e) { show($e); }
try { var_dump(call_user_func('ts', box(null))); } catch (TypeError $e) { show($e); }
$c = fn(string $s) => $s;
try { var_dump($c(box(null))); } catch (TypeError $e) { show($e); }
set_error_handler(function (int $no, string $msg, string $file, int $line) { echo "deprecation on line $line\n"; return true; });
$i = 'ti';
echo $i(box(1.5)), "\n";
"#,
    );
    assert_eq!(out, "ts(): Argument #1 ($s) must be of type string, null given, called in F on line 8\nK::m(): Argument #1 ($s) must be of type string, null given, called in F on line 10\nK::sm(): Argument #1 ($s) must be of type string, null given, called in F on line 12\nts(): Argument #1 ($s) must be of type string, null given, called in F on line 13\n{closure:F:14}(): Argument #1 ($s) must be of type string, null given, called in F on line 15\ndeprecation on line 4\n1\n");
}

/// A refused argument releases an extra named argument a variadic collects: `extra: make()`
/// leaked its fresh array whenever the check for `$s` refused, both through the named-call
/// precheck (a throwing default) and through the ordinary check, which did not root the
/// collected variadic array. A positional variadic tail is released too.
#[test]
fn test_named_call_precheck_releases_extra_named_arguments_on_refusal() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
function box(mixed $v): mixed { return $v; }
function make(int $i): array { return [str_repeat("x", 50), $i]; }
function f(string $s, int $d = PHP_INT_MAX, ...$rest) { return $s . ":" . $d; }
function g(string $s, int $d = 1 % 0, ...$rest) { return $s . ":" . $d; }
function p(string $s, ...$rest) { return $s; }
for ($i = 0; $i < 3; $i++) {
    try { f(s: box(null), extra: make($i)); } catch (TypeError $e) { echo "f\n"; }
    try { g(s: box(null), extra: make($i)); } catch (TypeError $e) { echo "g\n"; }
    try { p(box(null), make($i), make($i)); } catch (TypeError $e) { echo "p\n"; }
}
echo f(s: box("ok"), extra: make(9)), "\n";
echo p(box("ok"), make(9)), "\n";
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "f\ng\np\nf\ng\np\nf\ng\np\nok:9223372036854775807\nok\n");
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// A positional spread binds its unpacked elements before an omitted default that can throw is
/// evaluated, as PHP does: `f(...$row)` with `$row = [box(null)]` and `int $n = 1 % 0` threw
/// `DivisionByZeroError` instead of the `TypeError` for `$s`. The prefix (a fresh array here)
/// and the elements stay rooted while the default runs.
#[test]
fn test_positional_spread_checks_unpacked_elements_before_throwing_defaults() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
function box(mixed $v): mixed { return $v; }
function make(int $i): array { return [str_repeat("x", 50), $i]; }
function show(Throwable $e): void { echo get_class($e), ": ", explode(', called', $e->getMessage())[0], "\n"; }
function f(string $s, int $n = 1 % 0) { return $s; }
function g(array $a, string $s, int $n = 1 % 0) { return $s; }
function h(string $s, int $m = 5, int $n = 1 % 0) { return $s; }
for ($i = 0; $i < 2; $i++) {
    $rows = [[box(null)], [box([1])]];
    try { f(...$rows[$i]); } catch (Throwable $e) { show($e); }
    try { g(make($i), ...$rows[$i]); } catch (Throwable $e) { show($e); }
    try { h(...[box("ok"), box("x")]); } catch (Throwable $e) { show($e); }
}
try { f(...[box("ok")]); } catch (Throwable $e) { show($e); }
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "TypeError: f(): Argument #1 ($s) must be of type string, null given\nTypeError: g(): Argument #2 ($s) must be of type string, null given\nTypeError: h(): Argument #2 ($m) must be of type int, string given\nTypeError: f(): Argument #1 ($s) must be of type string, array given\nTypeError: g(): Argument #2 ($s) must be of type string, array given\nTypeError: h(): Argument #2 ($m) must be of type int, string given\nDivisionByZeroError: Modulo by zero\n");
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// A pipe binds its value through the callee's scalar-parameter check. `$v |> f(...)` lowered to
/// a direct call whose synthetic argument had no checker record, so `null` bound to `string`
/// without a `TypeError`; a piped closure was treated as pure and removed; and a global the
/// precision deprecation's handler wrote was read back stale after the pipe.
#[test]
fn test_pipe_checks_the_piped_value_against_scalar_parameters() {
    let out = compile_and_run(
        r#"<?php
function box(mixed $v): mixed { return $v; }
function ti(int $n): int { return $n; }
function ts(string $s): string { return "[$s]"; }
class K {
    public function m(string $s): string { return $s; }
    public static function st(string $s): string { return $s; }
}
function obj(): K { echo "obj\n"; return new K(); }
function show(Throwable $e): void { echo get_class($e), ": ", str_replace(__FILE__, "F", $e->getMessage()), "\n"; }
$v = box(null);
try { $v |> (static fn(string $s) => 1); echo "no throw\n"; } catch (TypeError $e) { echo "caught closure\n"; }
try { $v |> ts(...); echo "no throw\n"; } catch (TypeError $e) { show($e); }
try { $v |> K::st(...); echo "no throw\n"; } catch (TypeError $e) { show($e); }
try { $v |> (new K())->m(...); echo "no throw\n"; } catch (TypeError $e) { show($e); }
try { $v |> obj()->m(...); echo "no throw\n"; } catch (TypeError $e) { show($e); }
$g = ts(...);
try { $v |> $g; echo "no throw\n"; } catch (TypeError $e) { echo "caught var\n"; }
echo box("ok") |> ts(...), "\n";
echo box(5) |> ts(...), "\n";
set_error_handler('handler');
$w = box(1.5);
$x = 1; $w |> (static fn(int $n) => 1); echo "closure ", $x, "\n";
$x = 1; $w |> ti(...); echo "fcc ", $x, "\n";
$k = ti(...); $x = 1; $w |> $k; echo "varfcc ", $x, "\n";
$x = 1; $y = 5 |> ti(...); echo "literal ", $x, "\n";
function handler(int $no, string $msg): bool { global $x; $x = 2; return true; }
"#,
    );
    assert_eq!(out, "caught closure\nTypeError: ts(): Argument #1 ($s) must be of type string, null given, called in F on line 13\nTypeError: K::st(): Argument #1 ($s) must be of type string, null given, called in F on line 14\nTypeError: K::m(): Argument #1 ($s) must be of type string, null given, called in F on line 15\nobj\nTypeError: K::m(): Argument #1 ($s) must be of type string, null given, called in F on line 16\ncaught var\n[ok]\n[5]\nclosure 2\nfcc 2\nvarfcc 2\nliteral 1\n");
}

/// A refused argument is released after its `TypeError` is thrown, while it unwinds, so a
/// destructor it runs throws with the `TypeError` as its previous exception. The guarded value
/// itself (`p(mk())`) and another guarded operand (`q(box(null), mk())`) used to be released
/// before the `TypeError` existed, which lost it; an ordinary operand (`r([mk()], ...)`) was
/// already released by the unwinder. Expected output: php 8.5 with
/// `zend.exception_ignore_args=1`, the non-retaining trace configuration elephc follows (with
/// trace arguments retained, PHP keeps the object alive until the `TypeError` is released).
#[test]
fn test_refused_argument_destructor_throws_after_the_type_error() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
class D {
    public function __construct(public string $n) {}
    public function __destruct() { echo "destruct ", $this->n, "\n"; throw new LogicException($this->n); }
}
function mk(string $n): mixed { return new D($n); }
function box(mixed $v): mixed { return $v; }
function p(string $s): void { echo "body\n"; }
function q(int $i, string $s): void { echo "body\n"; }
function r(array $a, string $s): void { echo "body\n"; }
function desc(?Throwable $e): string { return $e ? get_class($e) . "(" . explode(', called', $e->getMessage())[0] . ")" . " <- " . desc($e->getPrevious()) : "none"; }
try { p(mk("a")); echo "after\n"; } catch (Throwable $e) { echo "caught ", desc($e), "\n"; }
try { q(box(null), mk("b")); echo "after\n"; } catch (Throwable $e) { echo "caught ", desc($e), "\n"; }
try { r([mk("c")], box(null)); echo "after\n"; } catch (Throwable $e) { echo "caught ", desc($e), "\n"; }
echo "next\n";
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "destruct a\ncaught LogicException(a) <- TypeError(p(): Argument #1 ($s) must be of type string, D given) <- none\ndestruct b\ncaught LogicException(b) <- TypeError(q(): Argument #1 ($i) must be of type int, null given) <- none\ndestruct c\ncaught LogicException(c) <- TypeError(r(): Argument #2 ($s) must be of type string, null given) <- none\nnext\n");
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// A deferred check's operands are published before the checks, so converting a `Stringable`
/// reads a borrowed box: the conversion used to release the owned box itself, and a destructor
/// that threw from that release leaked the converted string (and left a stale owner pointing at
/// the freed box). Expected output: php 8.5 with `zend.exception_ignore_args=1`.
#[test]
fn test_stringable_argument_conversion_with_throwing_destructor_does_not_leak() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
class S {
    public function __toString(): string { echo "toString\n"; return str_repeat("s", 40); }
    public function __destruct() { throw new LogicException("d"); }
}
class T {
    public function __toString(): string { echo "toString T\n"; return str_repeat("t", 40); }
    public function __destruct() { }
}
function mk(): mixed { return new S(); }
function mt(): mixed { return new T(); }
function p(string $s): void { echo "body ", strlen($s), "\n"; }
function q(string $s, int $n): void { echo "body ", strlen($s), "\n"; }
function box(mixed $v): mixed { return $v; }
function desc(?Throwable $e): string { return $e ? get_class($e) . "(" . explode(', called', $e->getMessage())[0] . ") <- " . desc($e->getPrevious()) : "none"; }
for ($i = 0; $i < 2; $i++) {
    try { p(mk()); echo "after\n"; } catch (Throwable $e) { echo "caught ", desc($e), "\n"; }
    try { q(mt(), box(null)); echo "after\n"; } catch (Throwable $e) { echo "caught ", desc($e), "\n"; }
    try { q(mt(), box(3)); echo "after\n"; } catch (Throwable $e) { echo "caught ", desc($e), "\n"; }
}
echo "next\n";
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "toString\nbody 40\ncaught LogicException(d) <- none\ntoString T\ncaught TypeError(q(): Argument #2 ($n) must be of type int, null given) <- none\ntoString T\nbody 40\nafter\ntoString\nbody 40\ncaught LogicException(d) <- none\ntoString T\ncaught TypeError(q(): Argument #2 ($n) must be of type int, null given) <- none\ntoString T\nbody 40\nafter\nnext\n");
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// Several checked operands of one call (spread elements, a numeric conversion followed by a
/// refusal) are each released once: an accepted `int`/`float` check reads a published box
/// instead of releasing an owned one a later refusal would release again.
#[test]
fn test_checked_operands_are_released_once_when_a_later_check_refuses() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
function box(mixed $v): mixed { return $v; }
function f(int $a, string $b): void { echo "body\n"; }
function g(float $a, int $b, string $c): void { echo "body\n"; }
function desc(?Throwable $e): string { return $e ? get_class($e) . "(" . explode(', called', $e->getMessage())[0] . ") <- " . desc($e->getPrevious()) : "none"; }
for ($i = 0; $i < 3; $i++) {
    $rows = [[box("7"), box(null)], [box(str_repeat("8", 1)), box([str_repeat("x", 30)])]];
    foreach ($rows as $row) {
        try { f(...$row); echo "after\n"; } catch (Throwable $e) { echo "caught ", desc($e), "\n"; }
    }
    $row3 = [box("1.5"), box("2"), box(null)];
    try { g(...$row3); echo "after\n"; } catch (Throwable $e) { echo "caught ", desc($e), "\n"; }
    try { f(box("7"), box(null)); echo "after\n"; } catch (Throwable $e) { echo "caught ", desc($e), "\n"; }
}
echo "next\n";
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "caught TypeError(f(): Argument #2 ($b) must be of type string, null given) <- none\ncaught TypeError(f(): Argument #2 ($b) must be of type string, array given) <- none\ncaught TypeError(g(): Argument #3 ($c) must be of type string, null given) <- none\ncaught TypeError(f(): Argument #2 ($b) must be of type string, null given) <- none\ncaught TypeError(f(): Argument #2 ($b) must be of type string, null given) <- none\ncaught TypeError(f(): Argument #2 ($b) must be of type string, array given) <- none\ncaught TypeError(g(): Argument #3 ($c) must be of type string, null given) <- none\ncaught TypeError(f(): Argument #2 ($b) must be of type string, null given) <- none\ncaught TypeError(f(): Argument #2 ($b) must be of type string, null given) <- none\ncaught TypeError(f(): Argument #2 ($b) must be of type string, array given) <- none\ncaught TypeError(g(): Argument #3 ($c) must be of type string, null given) <- none\ncaught TypeError(f(): Argument #2 ($b) must be of type string, null given) <- none\nnext\n");
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// A by-reference property argument and a runtime-checked scalar of the same call are checked
/// in parameter order: an earlier property refusal wins over a later `mixed` null, and an
/// earlier `mixed` null wins over a later property refusal. The accepted path still binds the
/// live property.
#[test]
fn test_property_reference_and_scalar_checks_raise_in_parameter_order() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
class C { public string $s = "abc"; public int $n = 4; }
function f(int &$a, int $b): void { echo "f ", $a, " ", $b, "\n"; $a = 9; }
function g(int $b, int &$a): void { echo "g ", $a, " ", $b, "\n"; }
function box(mixed $v): mixed { return $v; }
function show(Throwable $e): void { echo get_class($e), ": ", explode(', called', $e->getMessage())[0], "\n"; }
$o = new C;
try { f($o->s, box(null)); } catch (TypeError $e) { show($e); }
try { g(box(null), $o->s); } catch (TypeError $e) { show($e); }
try { f($o->n, box(null)); } catch (TypeError $e) { show($e); }
try { f($o->n, box("5")); } catch (TypeError $e) { show($e); }
echo $o->n, "\n";
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "TypeError: f(): Argument #1 ($a) must be of type int, string given\nTypeError: g(): Argument #1 ($b) must be of type int, null given\nTypeError: f(): Argument #2 ($b) must be of type int, null given\nf 4 5\n9\n");
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// With a throwing default left out, the supplied named arguments are checked before the
/// default runs, still in parameter order with an earlier by-reference property argument:
/// its refusal wins without converting a later `Stringable` or refusing a later `null`.
#[test]
fn test_named_checks_before_a_throwing_default_raise_earlier_property_references_first() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
class Bad { public function __toString(): string { echo "toString\n"; throw new Exception("from toString"); } }
class C { public string $s = "abc"; public int $n = 4; }
function f(int &$a, string $s, int $d = 1 % 0): void { echo "f ", $a, " ", $s, "\n"; }
function box(mixed $v): mixed { return $v; }
function show(Throwable $e): void { echo get_class($e), ": ", explode(', called', $e->getMessage())[0], "\n"; }
$o = new C;
try { f(a: $o->s, s: box(new Bad())); } catch (Throwable $e) { show($e); }
try { f(s: box(null), a: $o->s); } catch (Throwable $e) { show($e); }
try { f(a: $o->n, s: box(null)); } catch (Throwable $e) { show($e); }
try { f(a: $o->n, s: box("x")); } catch (Throwable $e) { show($e); }
echo "next\n";
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "TypeError: f(): Argument #1 ($a) must be of type int, string given\nTypeError: f(): Argument #1 ($a) must be of type int, string given\nTypeError: f(): Argument #2 ($s) must be of type string, null given\nDivisionByZeroError: Modulo by zero\nnext\n");
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// A method called through a callable value is named in its declared spelling, whatever case
/// the callable string uses: the invoker reads the descriptor's `Class::method` name.
#[test]
fn test_callable_method_guard_names_the_declared_method_spelling() {
    let out = compile_and_run(
        r#"<?php
function box(mixed $v): mixed { return $v; }
class P { public function withPath(string $p): string { return "[" . $p . "]"; } public static function sM(string $p): string { return $p; } }
function show(TypeError $e): void { echo explode(', called', $e->getMessage())[0], "\n"; }
$n = box(null);
try { call_user_func([new P, "withPath"], $n); } catch (TypeError $e) { show($e); }
try { call_user_func([new P, "WITHPATH"], $n); } catch (TypeError $e) { show($e); }
try { $c = [new P, "withpath"]; $c($n); } catch (TypeError $e) { show($e); }
try { call_user_func([P::class, "SM"], $n); } catch (TypeError $e) { show($e); }
"#,
    );
    assert_eq!(
        out,
        "P::withPath(): Argument #1 ($p) must be of type string, null given\nP::withPath(): Argument #1 ($p) must be of type string, null given\nP::withPath(): Argument #1 ($p) must be of type string, null given\nP::sM(): Argument #1 ($p) must be of type string, null given\n"
    );
}

/// Calls the compiler resolves through a bound method callable check runtime-typed arguments
/// like the method call they stand for: `call_user_func_array([$o, "m"], [...])` (also on a
/// subclass and with a receiver that makes its own call) and `$f = $o->m(...); $f(...)` in
/// either spelling. The internal URI methods keep coercing `null` through the same forms.
#[test]
fn test_bound_method_callables_check_runtime_scalar_arguments() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
error_reporting(E_ALL & ~E_DEPRECATED);
function box(mixed $v): mixed { return $v; }
class P { public function withPath(string $p): string { return "[" . $p . "]"; } }
class Q extends P {}
class Mk { public static function p(string $s): P { echo "mk ", $s, "\n"; return new P; } }
function show(TypeError $e): void { echo explode(', called', $e->getMessage())[0], "\n"; }
$n = box(null);
try { echo call_user_func_array([new P, "withPath"], [$n]), "\n"; } catch (TypeError $e) { show($e); }
try { echo call_user_func_array([new Q, "withPath"], [$n]), "\n"; } catch (TypeError $e) { show($e); }
try { $f = (new P)->withPath(...); echo $f($n), "\n"; } catch (TypeError $e) { show($e); }
try { $f = (new P)->WithPath(...); echo $f($n), "\n"; } catch (TypeError $e) { show($e); }
try { echo call_user_func_array([Mk::p(box("x")), "withPath"], [$n]), "\n"; } catch (TypeError $e) { show($e); }
try { $f = (new Q)->withPath(...); echo $f(box([1])), "\n"; } catch (TypeError $e) { show($e); }
echo call_user_func_array([new P, "withPath"], [box("ok")]), "\n";
$u = new Uri\Rfc3986\Uri("https://example.com/x");
echo call_user_func_array([$u, "withPath"], [$n])->toString(), "\n";
$g = $u->withPath(...);
echo $g($n)->toString(), "\n";
$h = $u->WithPath(...);
echo $h($n)->toString(), "\n";
$w = new Uri\WhatWg\Url("https://example.com/");
echo call_user_func_array([$w, "withQuery"], [$n])->toAsciiString(), "\n";
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(
        output.stdout,
        "P::withPath(): Argument #1 ($p) must be of type string, null given\nP::withPath(): Argument #1 ($p) must be of type string, null given\nP::withPath(): Argument #1 ($p) must be of type string, null given\nP::withPath(): Argument #1 ($p) must be of type string, null given\nmk x\nP::withPath(): Argument #1 ($p) must be of type string, null given\nP::withPath(): Argument #1 ($p) must be of type string, array given\n[ok]\nhttps://example.com\nhttps://example.com\nhttps://example.com\nhttps://example.com/\n"
    );
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// A method that a subclass or another implementor overrides is not checked against the
/// receiver's static class: PHP binds the override selected at run time, which here accepts
/// `null` and an `int` (`Child::m(mixed)`, `Other::name(?string)`), through a direct call and
/// through the callable forms alike. (The overrides print no argument: how a value reaches a
/// wider override through a parent-typed receiver is #2467.)
#[test]
fn test_scalar_checks_leave_overridden_methods_to_the_selected_override() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
function box(mixed $v): mixed { return $v; }
interface Named { public function name(string $s): string; }
class Base implements Named {
    public function m(string $s): string { return "base[" . $s . "]"; }
    public function name(string $s): string { return "base-name[" . $s . "]"; }
}
class Child extends Base {
    public function m(mixed $s): string { return "child"; }
}
class Other implements Named { public function name(?string $s): string { return "other"; } }
function show(TypeError $e): void { echo explode(', called', $e->getMessage())[0], "\n"; }
function run(Base $o, mixed $v): void {
    try { echo $o->m($v), "\n"; } catch (TypeError $e) { show($e); }
    try { $f = [$o, 'm']; echo $f($v), "\n"; } catch (TypeError $e) { show($e); }
    try { echo call_user_func_array([$o, 'm'], [$v]), "\n"; } catch (TypeError $e) { show($e); }
    try { echo call_user_func([$o, 'm'], $v), "\n"; } catch (TypeError $e) { show($e); }
}
function named(Named $n, mixed $v): void {
    try { echo $n->name($v), "\n"; } catch (TypeError $e) { show($e); }
}
run(new Child, box(null));
run(new Child, box(5));
named(new Other, box(null));
echo "done\n";
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "child\nchild\nchild\nchild\nchild\nchild\nchild\nchild\nother\ndone\n");
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// Under `strict_types=1` a bound method callable refuses an `int` for a `string` parameter in
/// every form, including `call_user_func_array($o->m(...), [...])`, which the compiler lowers
/// to a direct call.
#[test]
fn test_bound_method_callables_follow_strict_types() {
    let out = compile_and_run(
        r#"<?php
declare(strict_types=1);
function box(mixed $v): mixed { return $v; }
class P { public function withPath(string $p): string { return "[" . $p . "]"; } }
function show(TypeError $e): void { echo explode(', called', $e->getMessage())[0], "\n"; }
$o = new P;
try { echo call_user_func_array($o->withPath(...), [box(5)]), "\n"; } catch (TypeError $e) { show($e); }
try { echo call_user_func($o->withPath(...), box(5)), "\n"; } catch (TypeError $e) { show($e); }
try { echo call_user_func_array([$o, "withPath"], [box(5)]), "\n"; } catch (TypeError $e) { show($e); }
try { $f = $o->withPath(...); echo $f(box(5)), "\n"; } catch (TypeError $e) { show($e); }
echo call_user_func_array($o->withPath(...), [box("ok")]), "\n";
"#,
    );
    assert_eq!(out, "P::withPath(): Argument #1 ($p) must be of type string, int given\nP::withPath(): Argument #1 ($p) must be of type string, int given\nP::withPath(): Argument #1 ($p) must be of type string, int given\nP::withPath(): Argument #1 ($p) must be of type string, int given\n[ok]\n");
}

/// A refused argument of a call through a bound method callable releases the call's other
/// owned arguments: `call_user_func_array($o->m(...), [...])` lowers to a direct expr call whose
/// arguments are rooted in an argument-evaluation scope, like the other bound-method forms. An
/// accepted call releases the owned arguments it borrowed after the call.
#[test]
fn test_refused_bound_method_callable_arguments_do_not_leak() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
function box(mixed $v): mixed { return $v; }
function make(int $i): array { return [$i, "s" . $i]; }
class P { public function m(string $s, array $a): string { return $s . count($a); } }
$o = new P;
$n = 0;
for ($i = 0; $i < 300; $i++) {
    try { call_user_func_array($o->m(...), [box(null), make($i)]); } catch (TypeError $e) { $n++; }
    try { call_user_func_array($o->m(...), [box([$i]), make($i)]); } catch (TypeError $e) { $n++; }
    try { call_user_func_array([$o, "m"], [box(null), make($i)]); } catch (TypeError $e) { $n++; }
    try { $f = $o->m(...); $f(box(null), make($i)); } catch (TypeError $e) { $n++; }
    $r = call_user_func_array($o->m(...), [box("ok"), make($i)]);
}
echo $n, " ", $r, " ", $o->m(box("ok"), make(1)), "\n";
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "1200 ok2 ok2\n");
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// An accepted `string` check allocates exactly what the explicit `(string)` cast it replaces
/// does: one conversion and the call's one persisted operand per call. Publishing the converted
/// string before the call made the call persist a second copy (3 allocations per call instead
/// of 2), which cost about 45% on a `len($words[$i & 3])` loop.
#[test]
fn test_accepted_string_guard_allocates_like_an_explicit_cast() {
    let program = |argument: &str| format!(r#"<?php
function take(string $s): int {{ return strlen($s); }}
function run(mixed $words): int {{
    $total = 0;
    for ($i = 0; $i < 1000; $i++) {{ $total += take({argument}); }}
    return $total;
}}
echo run(json_decode('["alpha", "be"]', true)), "\n";
"#);
    let guarded = compile_and_run_with_gc_stats(&program("$words[$i & 1]"));
    let cast = compile_and_run_with_gc_stats(&program("(string) $words[$i & 1]"));
    assert!(guarded.success && cast.success, "{}{}", guarded.stderr, cast.stderr);
    assert_eq!(guarded.stdout, "3500\n");
    assert_eq!(cast.stdout, "3500\n");
    let (guarded_allocs, guarded_frees) = parse_gc_stats(&guarded.stderr);
    let (cast_allocs, cast_frees) = parse_gc_stats(&cast.stderr);
    assert_eq!(guarded_allocs - guarded_frees, cast_allocs - cast_frees, "{}", guarded.stderr);
    assert_eq!(guarded_allocs, cast_allocs, "guarded: {}\ncast: {}", guarded.stderr, cast.stderr);
}

/// PHP fills an omitted parameter declared before the last supplied named argument before it
/// checks any parameter, and the trailing omitted ones after the checks: `f(b: box(null))` for
/// `f(int $a = 1 % 0, string $b)` throws `DivisionByZeroError`, as does `g(s: box(null), b: 'y')`
/// with the throwing default between them, while `h(s: box(null))` with only a trailing one
/// throws the `TypeError`; a positional prefix and a literal string-keyed spread bind the same
/// way. The supplied values stay released when the leading default throws.
#[test]
fn test_named_leading_throwing_default_runs_before_the_supplied_checks() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
function box(mixed $v): mixed { return $v; }
function show(Throwable $e): void { echo get_class($e), ": ", explode(', called', $e->getMessage())[0], "\n"; }
function f(int $a = 1 % 0, string $b = 'x') { echo "f body\n"; }
function g(string $s, int $a = 1 % 0, string $b = 'x', int $c = 2 % 0) { echo "g body\n"; }
function h(string $s, int $a = 1 % 0) { echo "h body\n"; }
try { f(b: box(null)); } catch (Throwable $e) { show($e); }
try { g(s: box(null), b: 'y'); } catch (Throwable $e) { show($e); }
try { g(s: str_repeat('o', 3), b: box(null)); } catch (Throwable $e) { show($e); }
try { g(s: box(null)); } catch (Throwable $e) { show($e); }
try { h(s: box(null)); } catch (Throwable $e) { show($e); }
try { g(box(null), b: 'y'); } catch (Throwable $e) { show($e); }
try { f(...['b' => box(null)]); } catch (Throwable $e) { show($e); }
try { g(box(null), ...['b' => 'y']); } catch (Throwable $e) { show($e); }
echo "next\n";
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "DivisionByZeroError: Modulo by zero\nDivisionByZeroError: Modulo by zero\nDivisionByZeroError: Modulo by zero\nTypeError: g(): Argument #1 ($s) must be of type string, null given\nTypeError: h(): Argument #1 ($s) must be of type string, null given\nDivisionByZeroError: Modulo by zero\nDivisionByZeroError: Modulo by zero\nDivisionByZeroError: Modulo by zero\nnext\n");
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// A call through a callable alias (`$f = 'take'`) has no checker record for the positional
/// prefix before a spread, so the prefix is checked from its values: `$f(box(null), ...$rows[$i])`
/// and `$f(box(null), ...$list)` threw nothing and bound `null` as `""`.
#[test]
fn test_callable_alias_spread_prefix_is_checked() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
function box(mixed $v): mixed { return $v; }
function show(Throwable $e): void { echo get_class($e), ": ", explode(', called', $e->getMessage())[0], "\n"; }
function take(string $s, int $n) { echo "take [", $s, "] ", $n, "\n"; }
$rows = [[5], [6]];
$i = 0;
$list = [7];
$f = 'take';
try { $f(box(null), ...$rows[$i]); } catch (Throwable $e) { show($e); }
try { $f(box(null), ...$list); } catch (Throwable $e) { show($e); }
$f(box("ok"), ...$rows[1]);
echo "next\n";
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "TypeError: take(): Argument #1 ($s) must be of type string, null given\nTypeError: take(): Argument #1 ($s) must be of type string, null given\ntake [ok] 6\nnext\n");
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// A leading omitted by-reference parameter's default (`array &$a = []`) is not lowered ahead of
/// the supplied named arguments' checks: a refused check leaves nothing to release, an accepted
/// call still binds it by reference, and a callee that throws still releases it (`--heap-debug`
/// leak summary clean).
#[test]
fn test_named_leading_by_reference_default_is_released_on_refusal() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
function box(mixed $v): mixed { return $v; }
function f(array &$a = [], string $b = 'x', int $d = 1 % 0) { echo "f body\n"; }
function g(array &$a = [1, 2], string $b = 'x', int $d = 1 % 0) { echo "g body\n"; }
function k(array &$a = [3, 4], string $b = 'x', int $d = PHP_INT_SIZE) { $a[] = $b; echo "k ", implode(",", $a), " ", $d, "\n"; }
function t(array &$a = [5, 6], string $b = 'x', int $d = PHP_INT_SIZE) { $a[] = $b; throw new Exception("t " . count($a)); }
$n = 0;
for ($i = 0; $i < 3; $i++) {
    try { f(b: box(null)); } catch (TypeError $e) { $n++; }
    try { g(b: box(null)); } catch (TypeError $e) { $n++; }
    try { k(b: box(null)); } catch (TypeError $e) { $n += 100; }
    k(b: box(str_repeat("z", 3)));
    try { t(b: box("ok")); } catch (Exception $e) { echo $e->getMessage(), "\n"; }
}
echo $n, "\n";
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "k 3,4,zzz 8\nt 3\nk 3,4,zzz 8\nt 3\nk 3,4,zzz 8\nt 3\n306\n");
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// A leading omitted by-reference parameter whose default can throw (`int &$a = 1 % 0`) is
/// filled before the supplied named arguments are checked, as PHP does: `f(b: box(null))` throws
/// `DivisionByZeroError`, not the `TypeError` for `$b`. A by-reference default parked that way
/// (`array &$a = [PHP_INT_SIZE]`) is still released when a later check refuses or the callee
/// throws, and an accepted call binds it by reference (`--heap-debug` leak summary clean).
#[test]
fn test_named_leading_throwing_by_reference_default_runs_before_the_checks() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
function box(mixed $v): mixed { return $v; }
function show(Throwable $e): void { echo get_class($e), ": ", explode(', called', $e->getMessage())[0], "\n"; }
function f(int &$a = 1 % 0, string $b = 'x') { echo "f body\n"; }
function g(array &$a = [PHP_INT_SIZE], string $b = 'x', int $d = PHP_INT_SIZE) { $a[] = $b; echo "g ", implode(",", $a), "\n"; }
function t(array &$a = [PHP_INT_SIZE, 1], string $b = 'x', int $d = PHP_INT_SIZE) { $a[] = $b; throw new Exception("t " . count($a)); }
function k(array &$a = [], string $b = 'x', int $d = 1 % 0) { echo "k body\n"; }
for ($i = 0; $i < 3; $i++) {
    try { f(b: box(null)); } catch (Throwable $e) { show($e); }
    try { g(b: box(null)); } catch (Throwable $e) { show($e); }
    g(b: box(str_repeat("z", 3)));
    try { t(b: box("ok")); } catch (Throwable $e) { show($e); }
    try { k(b: box(null)); } catch (Throwable $e) { show($e); }
}
echo "next\n";
"#,
    );
    assert!(output.success, "{}", output.stderr);
    let round = "DivisionByZeroError: Modulo by zero\nTypeError: g(): Argument #2 ($b) must be of type string, null given\ng 8,zzz\nException: t 3\nTypeError: k(): Argument #2 ($b) must be of type string, null given\n";
    assert_eq!(output.stdout, format!("{round}{round}{round}next\n"));
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}
