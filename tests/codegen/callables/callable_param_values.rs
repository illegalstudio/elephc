//! Purpose:
//! Regression tests for the VALUE a `callable` parameter or a `Callable`-typed local holds.
//!
//! Called from:
//! - `cargo test --test codegen_tests callable_param_values` through Rust's test harness.
//!
//! Key details:
//! - php keeps what the caller passed: `is_string('strlen')`, `is_array([$o, 'm'])` and `$c[0]` see
//!   the string or the array. A `callable` parameter used to be normalized into a callable
//!   descriptor at the boundary, which answered `Closure` for every spelling; a body that reads the
//!   parameter as a value now keeps it (`Checker::callable_param_storage`), and a body that only
//!   calls it keeps the descriptor.
//! - A store of a string or an array into a slot narrowed or typed `Callable` widens it to `Mixed`
//!   instead of being swallowed by the callable (`Checker::merged_assignment_type`).
//! - Every expected string is `php -n` 8.5's output on the same source.

use crate::support::*;

/// The spelling the caller used survives the `callable` boundary when the body inspects it.
#[test]
fn test_a_callable_parameter_read_as_a_value_keeps_the_callers_value() {
    let out = compile_and_run(
        r#"<?php
class K { public static function m(): int { return 1; } public function i(int $x): int { return $x * 2; } }
function kind(callable $c): string {
    return is_string($c) ? "string" : (is_array($c) ? "array" : (is_object($c) ? get_class($c) : "other"));
}
echo kind("strlen"), " ", kind([new K, "i"]), " ", kind(["K", "m"]), " ", kind(fn() => 1), " ", kind(K::m(...)), "\n";
function callit(callable $c, int $v): int { $n = is_string($c) ? 100 : 0; return $n + $c($v); }
echo callit("abs", -3), " ", callit([new K, "i"], 4), " ", callit(fn($x) => $x + 1, 1), "\n";
function only_call(callable $c): int { return $c(5); }
echo only_call(fn($x) => $x * 3), " ", only_call("abs"), "\n";
"#,
    );
    assert_eq!(out, "string array array Closure Closure\n103 8 2\n15 5\n");
}

/// A `?callable` stored in a property keeps the caller's value, and a closure read back from it is
/// a `Closure` to `method_exists()`: twig's `TwigFunction` stores its callable, and
/// symfony/twig-bridge's `DebugCommand` inspects it with `\is_array($cb)`, `method_exists($cb[0],
/// $cb[1])` and `method_exists($cb, '__invoke')`.
#[test]
fn test_a_stored_callable_keeps_its_shape_for_member_probes() {
    let out = compile_and_run(
        r#"<?php
class Ext { public function upper(string $s): string { return strtoupper($s); } }
class TwigFn {
    private $callable;
    public function __construct(private string $name, ?callable $callable = null) { $this->callable = $callable; }
    public function getCallable() { return $this->callable; }
}
$fns = [new TwigFn("a", [new Ext(), "upper"]), new TwigFn("b", "max"), new TwigFn("c", fn() => 1)];
foreach ($fns as $f) {
    $cb = $f->getCallable();
    if (is_array($cb)) { echo method_exists($cb[0], $cb[1]) ? "array-ok" : "array-no"; }
    elseif (is_object($cb) && method_exists($cb, "__invoke")) { echo "invokable"; }
    elseif (function_exists($cb)) { echo "function"; }
    else { echo "string"; }
    echo "\n";
}
$o = fn() => 1;
echo method_exists($o, "__invoke") ? "Y" : "N", method_exists($o, "nope") ? "Y" : "N", "\n";
"#,
    );
    assert_eq!(out, "array-ok\nfunction\ninvokable\nYN\n");
}

/// An invokable object is the third callable shape, also for a `callable|array` union parameter
/// (symfony/event-dispatcher's `TraceableEventDispatcher::getListenerPriority()`).
#[test]
fn test_an_invokable_object_satisfies_a_callable_union_parameter() {
    let out = compile_and_run(
        r#"<?php
class W { public function __invoke(int $x): int { return $x + 1; } }
function run(callable $c): int { return $c(41); }
function prio(callable|array $l): string { return is_object($l) ? get_class($l) : "arr"; }
echo run(new W()), " ", prio(new W()), " ", prio([1]), "\n";
"#,
    );
    assert_eq!(out, "42 W arr\n");
}

/// An array stored into a variable that `instanceof \Closure` narrowed is an array afterwards, in a
/// method and in a function (symfony/cache's `EarlyExpirationMessage::create()` shape).
#[test]
fn test_an_array_stored_over_a_closure_narrowing_stays_an_array() {
    let out = compile_and_run(
        r#"<?php
class S {
    public function n(mixed $c): string {
        if ($c instanceof \Closure) { $c = ["a", "b"]; echo $c[0]; }
        return is_array($c) ? "array" : "other";
    }
}
function f(mixed $c): string {
    if ($c instanceof \Closure && true) { $c = ["a", "b"]; $c[0] ?: $c = "z"; }
    return is_array($c) ? "array" : "other";
}
echo (new S())->n(fn() => 1), " ", f(fn() => 1), " ", f("x"), "\n";
"#,
    );
    assert_eq!(out, "aarray array other\n");
}

/// A local stored in ONE arm of a conditional expression keeps the other arm's value after it:
/// the merge used to adopt the storing arm's type, so an untaken `$c = 'z'` made a slot still
/// holding an array read as a string. Covers `?:`, `? :` and `??` on a reassigned parameter.
#[test]
fn test_a_store_in_one_arm_of_a_conditional_expression_keeps_the_other_arms_value() {
    let out = compile_and_run(
        r#"<?php
function g(mixed $c): string { $c = ["a"]; $c[0] ?: $c = "z"; return is_array($c) ? "array" : $c; }
function t(mixed $c, int $f): string { $c = ["a"]; $x = $f ? ($c = "z") : 1; return is_array($c) ? "array" : $c; }
function n(mixed $c, ?int $f): string { $c = ["a"]; $x = $f ?? ($c = "z"); return is_array($c) ? "array" : $c; }
echo g(1), " ", t(1, 0), " ", t(1, 1), " ", n(1, 5), " ", n(1, null), "\n";
"#,
    );
    assert_eq!(out, "array array z array z\n");
}
