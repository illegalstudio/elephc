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

/// Verifies by-reference and variadic `callable` parameters (including a by-reference variadic
/// `callable &...$xs`) check their values on entry, and that a union with a coercible scalar
/// (`int|callable`) still takes its members.
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
function rv(callable &...$xs): void { echo "ran ", count($xs), "\n"; }
$a = $argc > 0 ? "nope" : "strlen";
try { rv($a); echo "after\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$b = "strlen"; $c = "strtoupper"; rv($b, $c);
"#,
    );
    assert_eq!(
        out,
        "r(): Argument #1 ($c) must be of type callable, string given\nok\n\
         v(): Argument #2 must be of type callable, string given\n\
         v(): Argument #1 must be of type callable, array given\n2\nint object\n\
         rv(): Argument #1 must be of type callable, string given\nran 2\n"
    );
}

/// Verifies a `callable` return type checks the returned value, that a nullable variadic
/// `?callable ...` admits null while refusing other non-callables, and that a scalar union
/// (`int|callable`) reached through a `mixed` forwarder still refuses a non-callable array.
///
/// These used to pass unchecked: a `callable` return accepted any string, the variadic entry
/// guard refused null for `?callable`, and the scalar union skipped the entry check.
#[test]
fn test_declared_callable_returns_nullable_variadics_and_scalar_unions_check_values() {
    let out = compile_and_run(
        r#"<?php
function bad(int $argc): callable { $n = $argc > 0 ? "missing_function" : "strlen"; return $n; }
try { bad($argc); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
function good(): callable { return "strlen"; }
echo good()("abc"), "\n";
function take(?callable ...$fs): int { return count($fs); }
echo take(null, "strlen"), "\n";
function pick(int $n): mixed { return $n > 0 ? 5 : null; }
$bad = pick($argc);
try { take(null, $bad); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
function either(callable|false ...$fs): int { return count($fs); }
echo either(false, "strlen"), "\n";
function scalar(int|callable $v): string { return gettype($v); }
function forward(mixed $v): string { return scalar($v); }
echo forward(7), " ", forward("strlen"), "\n";
try { echo forward([]), "\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "bad(): Return value must be of type callable, string returned\n3\n2\n\
         take(): Argument #2 must be of type ?callable, int given\n2\ninteger string\n\
         scalar(): Argument #1 ($v) must be of type callable|int, array given\n"
    );
}

/// Verifies a native method reached from eval through a Closure descriptor resolves a callback
/// name that only eval declared when it binds to a declared `callable` parameter.
///
/// The method invoker built for an eval-created descriptor used to skip the eval-callable
/// conversion, so the eval-only function name was refused as not callable.
#[test]
fn test_declared_callable_from_eval_method_descriptor_sees_eval_functions() {
    let out = compile_and_run(
        r#"<?php
class Target { public static function run(callable $f): string { return $f(); } }
function invoke(Closure $c, string $name): string { return $c($name); }
eval('function evalOnlyCallback(): string { return "E"; } echo invoke(Closure::fromCallable(["Target", "run"]), "evalOnlyCallback"), "\n";');
"#,
    );
    assert_eq!(out, "E\n");
}

/// Verifies a call whose arguments are all fresh objects, and an immediately invoked closure,
/// keep the `TypeError` handler of a declared `callable` check, whose catch used to be pruned.
///
/// Exception-flow DCE let a fresh `new` argument waive every entry check, though it proves
/// nothing about `is_callable`, and modelled only `int` return checks, so a `callable` return
/// of a function given `new Box()` or of an immediate closure escaped its `try`.
#[test]
fn test_declared_callable_type_errors_survive_fresh_objects_and_immediate_closures() {
    let out = compile_and_run(
        r#"<?php
class Box {}
function bad(Box $o): callable { $name = "missing_function"; return $name; }
try { bad(new Box()); } catch (TypeError $e) { echo "A ", $e->getMessage(), "\n"; }
try {
    (function (): callable { $n = "missing_function"; return $n; })();
} catch (TypeError $e) { echo "B caught\n"; }
try { (fn(): callable => "missing_function")(); } catch (TypeError $e) { echo "B2 caught\n"; }
function take(callable $c): void {}
try { take(new Box()); } catch (TypeError $e) { echo "C ", $e->getMessage(), "\n"; }
class Holder { public static function take(callable ...$cs): void {} }
try { Holder::take(new Box()); } catch (TypeError $e) { echo "D ", $e->getMessage(), "\n"; }
echo "done\n";
"#,
    );
    assert_eq!(
        out,
        "A bad(): Return value must be of type callable, string returned\nB caught\nB2 caught\n\
         C take(): Argument #1 ($c) must be of type callable, Box given\n\
         D Holder::take(): Argument #1 must be of type callable, Box given\ndone\n"
    );
}

/// Verifies a native method reached from eval through a Closure descriptor resolves an
/// eval-only callback name bound to `int|callable`, `?callable` and `callable ...`, keeps an int
/// for `int|callable`, and raises PHP's `TypeError` for a value no member accepts.
///
/// Only an exact `callable` parameter used to convert eval-only callbacks; the union, nullable
/// and variadic forms rejected them because the native `is_callable` cannot see eval functions.
#[test]
fn test_declared_callable_unions_and_variadics_from_eval_see_eval_functions() {
    let out = compile_and_run(
        r#"<?php
class Target {
    public static function run(int|callable $f): string { return is_int($f) ? "int" : $f(); }
    public static function maybe(?callable $f): string { return $f === null ? "null" : $f(); }
    public static function many(callable ...$fs): string { $out = ""; foreach ($fs as $f) { $out .= $f(); } return $out; }
}
function invoke(Closure $c, mixed $arg): string { return $c($arg); }
eval('function evalOnlyCallback(): string { return "E"; }
echo invoke(Closure::fromCallable(["Target", "run"]), "evalOnlyCallback"), " ",
    invoke(Closure::fromCallable(["Target", "run"]), 7), " ",
    invoke(Closure::fromCallable(["Target", "maybe"]), "evalOnlyCallback"), " ",
    invoke(Closure::fromCallable(["Target", "maybe"]), null), " ",
    invoke(Closure::fromCallable(["Target", "many"]), "evalOnlyCallback"), "\n";
try { invoke(Closure::fromCallable(["Target", "run"]), "nope"); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }');
"#,
    );
    assert_eq!(
        out,
        "E int E null E\nTarget::run(): Argument #1 ($f) must be of type callable|int, string given\n"
    );
}

/// Verifies a generator raises its declared `callable` `TypeError` at the call (before the
/// Generator exists), checks a by-reference `callable` only once at the call, and that named
/// arguments to a `callable ...$xs` variadic report PHP's positional argument numbers.
///
/// Generators used to check callable parameters only when the body first ran, so the call's
/// `catch` never saw the error and a later change to a by-reference argument was rechecked;
/// named arguments promoted the variadic to an iterable container that skipped the check, and
/// a named entry used to be reported by its name instead of its argument number.
#[test]
fn test_declared_callable_generators_and_named_variadics_check_at_the_call() {
    let out = compile_and_run(
        r#"<?php
function g(callable &...$xs): Generator { echo "body\n"; yield 1; }
function g3(callable $c): Generator { echo "body3\n"; yield 1; }
function g2(callable &$c): Generator { echo "body2\n"; yield 1; }
$a = $argc > 0 ? "nope" : "strlen";
try { $gen = g($a); echo "created\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { $gen = g3($a); echo "created3\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$b = "strlen";
$gen2 = g2($b);
$b = "nope";
foreach ($gen2 as $_) {}
echo "done\n";
function v(callable ...$xs): string { return "ran"; }
try { v(foo: "nope"); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { v(foo: "strlen", bar: "nope"); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { v("strlen", "strrev", z: "nope"); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "g(): Argument #1 must be of type callable, string given\n\
         g3(): Argument #1 ($c) must be of type callable, string given\n\
         body2\n\
         done\n\
         v(): Argument #1 must be of type callable, string given\n\
         v(): Argument #1 must be of type callable, string given\n\
         v(): Argument #3 must be of type callable, string given\n"
    );
}
