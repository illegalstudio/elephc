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
