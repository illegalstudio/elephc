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
