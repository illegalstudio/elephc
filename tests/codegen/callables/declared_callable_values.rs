//! Purpose:
//! End-to-end coverage for values bound to a declared `callable` parameter or returned through
//! a `callable` return type: they keep the PHP value the caller passed.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expectation is the PHP 8.5 output of the same fixture. PHP appends
//!   `, called in FILE on line N` to a userland binding `TypeError`; elephc's entry guards stop
//!   at `given`, like its other parameter guards, so the fixtures print the message up to there.
//! - A declared `callable` is stored as a boxed value (`PhpType::DeclaredCallable`); only the
//!   sites that invoke it (or hand it to a descriptor-typed slot) build a callable descriptor.

use crate::support::*;

/// Verifies strings, arrays, invokable objects and closures keep their PHP type and value
/// inside a `callable` parameter and through a `callable` return, while still invoking.
///
/// Before declared callables kept their value, every one of them arrived as a `Closure`:
/// `gettype()` said `object`, `is_string()` was false and `ctype_alpha("strrev")` deprecated
/// a `Closure` instead of scanning the name.
#[test]
fn test_declared_callable_parameters_keep_php_values() {
    let out = compile_and_run_capture(
        r#"<?php
class Shout { public function __invoke(string $s): string { return strtoupper($s); } }
class Label { public function wrap(string $s): string { return "[$s]"; } public static function tag(string $s): string { return "<$s>"; } }
function describe(callable $f): string {
    return gettype($f) . ":" . (is_string($f) ? "s" : "-") . (is_array($f) ? "a" : "-")
        . (is_object($f) ? "o" : "-") . (ctype_alpha($f) ? "A" : "-") . ":" . $f("hi");
}
echo describe("strrev"), "\n";
echo describe("Label::tag"), "\n";
echo describe([new Label(), "wrap"]), "\n";
echo describe(new Shout()), "\n";
echo describe(fn(string $s): string => "($s)"), "\n";
echo describe(ucfirst(...)), "\n";
function keep(callable $f): callable { return $f; }
var_dump(keep("strlen"), keep(["Label", "tag"]));
echo keep("ucfirst")("word"), "\n";
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "string:s--A:ih\nstring:s---:<hi>\narray:-a--:[hi]\nobject:--o-:HI\nobject:--o-:(hi)\n\
         object:--o-:Hi\nstring(6) \"strlen\"\narray(2) {\n  [0]=>\n  string(5) \"Label\"\n  [1]=>\n  \
         string(3) \"tag\"\n}\nWord\n"
    );
    let deprecation = |type_name: &str| {
        format!(
            "Deprecated: ctype_alpha(): Argument of type {type_name} will be interpreted as string in the future"
        )
    };
    let expected = [deprecation("array"), deprecation("Shout"), deprecation("Closure"), deprecation("Closure")];
    assert_eq!(normalize_diagnostic_text(&out.stderr), expected.join("\n") + "\n");
}

/// Verifies runtime-selected names, branch-selected and instance callable arrays, and spread
/// arguments bind to a declared `callable` and invoke, as do the pipe operator, builtin
/// callbacks and `bindTo()` on the bound value.
///
/// These used to be refused at compile time because a `callable` parameter needed a
/// statically resolved descriptor target.
#[test]
fn test_declared_callable_accepts_runtime_selected_targets() {
    let out = compile_and_run(
        r#"<?php
class BranchCallable { public static function left(): int { return 1; } public static function right(): int { return 2; } }
class InstanceCallable { public function hit(): int { return 3; } }
function consume(callable $callback): int { return $callback(); }
function choose(int $value): int {
    if ($value > 0) { $callback = [BranchCallable::class, 'left']; } else { $callback = [BranchCallable::class, 'right']; }
    return consume($callback);
}
function chooseInstance(int $value): int {
    $receiver = new InstanceCallable();
    $callback = $value > 0 ? [$receiver, 'hit'] : [new InstanceCallable(), 'hit'];
    return consume($callback);
}
function apply(callable $f, string $s): string { return $f($s); }
$name = $argc > 0 ? "strtoupper" : "strtolower";
$callbacks = [[BranchCallable::class, 'right']];
echo choose($argc), chooseInstance($argc), apply($name, "a"), consume(...$callbacks), "\n";
function piped(callable $f, string $s): string { return $s |> $f; }
function mapped(callable $f): string { return implode(",", array_map($f, ["a", "b"])); }
function rebound(callable $f): string { return $f->bindTo(new InstanceCallable())(); }
echo piped("strrev", "abc"), " ", mapped("strtoupper"), " ", rebound(function () { return get_class($this); }), "\n";
"#,
    );
    assert_eq!(out, "13A2\ncba A,B InstanceCallable\n");
}

/// Verifies a value that is not callable raises PHP's binding `TypeError` when it reaches a
/// declared `callable`, including through a `mixed` forwarder, and that `?callable` admits null.
#[test]
fn test_declared_callable_rejects_non_callables_on_entry() {
    let out = compile_and_run(
        r#"<?php
function apply(callable $f): string { return "called"; }
function maybe(?callable $f): string { return $f === null ? "null" : "set"; }
function forward(mixed $value): string { return apply($value); }
foreach ([[1, 2], [1, "join"], "missing_function", 42] as $value) {
    try { echo apply($value), "\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
try { echo forward(null), "\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
echo maybe(null), " ", maybe("strlen"), "\n";
"#,
    );
    assert_eq!(
        out,
        "apply(): Argument #1 ($f) must be of type callable, array given\n\
         apply(): Argument #1 ($f) must be of type callable, array given\n\
         apply(): Argument #1 ($f) must be of type callable, string given\n\
         apply(): Argument #1 ($f) must be of type callable, int given\n\
         apply(): Argument #1 ($f) must be of type callable, null given\n\
         null set\n"
    );
}

/// Verifies `bindTo()` / `call()` on a declared `callable` act on a Closure only.
///
/// An invokable object runs its own `bindTo()` / `call()`, a string raises PHP's
/// `Call to a member function bindTo() on string`, and a Closure is still rebound. These
/// used to unbox any value as a Closure descriptor and rebind it.
#[test]
fn test_declared_callable_closure_methods_need_a_closure() {
    let out = compile_and_run(
        r#"<?php
class C { public function __invoke() {} public function bindTo(): string { return "user"; } public function call(): string { return "usercall"; } }
function bt(callable $c): mixed { return $c->bindTo(null); }
function cl(callable $c): mixed { return $c->call(new stdClass()); }
echo bt(new C()), " ", cl(new C()), "\n";
try { bt("strlen"); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
$b = bt(function () { return isset($this) ? "this" : "none"; });
echo $b(), "\n";
"#,
    );
    assert_eq!(
        out,
        "user usercall\nError: Call to a member function bindTo() on string\nnone\n"
    );
}

/// Verifies by-reference and variadic `callable` parameters check their values on entry, and
/// that a union with a coercible scalar (`int|callable`) still takes its members.
#[test]
fn test_declared_callable_reference_and_variadic_parameters_check_values() {
    let out = compile_and_run(
        r#"<?php
function r(callable &$c): void { echo "ok\n"; }
$x = $argc > 0 ? "nope" : [1];
try { r($x); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$y = $argc > 0 ? "strlen" : [1];
r($y);
function v(callable ...$xs): int { return count($xs); }
$nope = $argc > 0 ? "nope" : "strlen";
try { v("strlen", $nope); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$pair = $argc > 0 ? [1, 2] : [];
try { v($pair); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
echo v("strlen", fn() => 1), "\n";
function u(int|callable $v): string { return is_int($v) ? "int" : gettype($v); }
echo u(5), " ", u(fn() => 1), "\n";
"#,
    );
    assert_eq!(
        out,
        "r(): Argument #1 ($c) must be of type callable, string given\nok\n\
         v(): Argument #2 must be of type callable, string given\n\
         v(): Argument #1 must be of type callable, array given\n2\nint object\n"
    );
}
