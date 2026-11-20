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

/// A closure's return `TypeError` names it by its PHP label, `{closure:file:line}` (or the
/// enclosing function's name inside a function), not its generated `__eir_closure_*` symbol.
#[test]
fn test_closure_return_type_error_names_the_closure_label() {
    let out = compile_and_run(
        r#"<?php
function box(mixed $v): mixed { return $v; }
function show(Throwable $e): void { echo str_replace(__FILE__, "F", $e->getMessage()), "\n"; }
$s = function (mixed $v): string { return box($v); };
$f = fn(mixed $v): float => box($v);
$b = static function (mixed $v): bool { return box($v); };
$i = function (mixed $v): int { return box($v); };
function outer(): void {
    $inner = function (): string { return box(null); };
    try { $inner(); } catch (TypeError $e) { show($e); }
}
try { $s(null); } catch (TypeError $e) { show($e); }
try { $f("x"); } catch (TypeError $e) { show($e); }
try { $b([]); } catch (TypeError $e) { show($e); }
try { $i(null); } catch (TypeError $e) { show($e); }
outer();
"#,
    );
    assert_eq!(out, "{closure:F:4}(): Return value must be of type string, null returned\n{closure:F:5}(): Return value must be of type float, string returned\n{closure:F:6}(): Return value must be of type bool, array returned\n{closure:F:7}(): Return value must be of type int, null returned\n{closure:outer():9}(): Return value must be of type string, null returned\n");
}

/// A function an included file declares is lowered under a generated include-variant symbol;
/// its return `TypeError`, and the label of a closure declared inside it, name the PHP function.
#[test]
fn test_return_type_error_names_an_included_function_by_its_php_name() {
    let out = compile_and_run_files(
        &[
            (
                "defs.php",
                r#"<?php
declare(strict_types=1);
function ps(mixed $v): string { return $v; }
function pi_(mixed $v): int { return $v; }
function mk(): Closure { return fn(mixed $v): bool => $v; }
"#,
            ),
            (
                "main.php",
                r#"<?php
require __DIR__ . '/defs.php';
function box(mixed $v): mixed { return $v; }
try { ps(box(5)); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { pi_(box(null)); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$c = mk();
try { $c(box("x")); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "ps(): Return value must be of type string, int returned\npi_(): Return value must be of type int, null returned\n{closure:mk():5}(): Return value must be of type bool, string returned\n");
}

/// An anonymous class's method is named `class@anonymous::m` (`Parent@anonymous::m`,
/// `Interface@anonymous::m`), not by the class's generated `class@anonymous#N` symbol.
#[test]
fn test_return_type_error_names_an_anonymous_class_like_php() {
    let out = compile_and_run(
        r#"<?php
function box(mixed $v): mixed { return $v; }
class Base {}
interface I {}
$c = new class { public function s(mixed $v): string { return $v; } public function i(mixed $v): int { return $v; } };
$d = new class extends Base { public function f(mixed $v): float { return $v; } };
$e = new class implements I { public function b(mixed $v): bool { return $v; } };
try { $c->s(box(null)); } catch (TypeError $x) { echo $x->getMessage(), "\n"; }
try { $c->i(box(null)); } catch (TypeError $x) { echo $x->getMessage(), "\n"; }
try { $d->f(box(null)); } catch (TypeError $x) { echo $x->getMessage(), "\n"; }
try { $e->b(box(null)); } catch (TypeError $x) { echo $x->getMessage(), "\n"; }
"#,
    );
    assert_eq!(out, "class@anonymous::s(): Return value must be of type string, null returned\nclass@anonymous::i(): Return value must be of type int, null returned\nBase@anonymous::f(): Return value must be of type float, null returned\nI@anonymous::b(): Return value must be of type bool, null returned\n");
}

/// A refused return releases the owned temporary being returned (`return mk();`), which no
/// cleanup root covers; it used to leak on every `TypeError`.
#[test]
fn test_refused_return_releases_the_returned_temporary() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
function mk(int $i): mixed { return [str_repeat("x", 100), $i]; }
function gs(int $i): string { return mk($i); }
function gf(int $i): float { return mk($i); }
function gb(int $i): bool { return mk($i); }
for ($i = 0; $i < 3; $i++) {
    foreach (['gs', 'gf', 'gb'] as $f) {
        try { $f($i); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    }
}
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "gs(): Return value must be of type string, array returned\ngf(): Return value must be of type float, array returned\ngb(): Return value must be of type bool, array returned\n".repeat(3));
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// A refused return value whose destructor throws: the `TypeError` is thrown first and the
/// value released while it unwinds, so the destructor's exception carries the `TypeError` as
/// its previous one (and the message no longer leaks), and a `catch` for the destructor's
/// exception class around the call is kept.
#[test]
fn test_refused_return_with_throwing_destructor_chains_the_type_error() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
class D { public function __destruct() { throw new LogicException("d"); } }
function mk(): mixed { return new D(); }
function gs(): string { return mk(); }
function gf(): float { return mk(); }
function gb(): bool { return mk(); }
foreach (['gs', 'gf', 'gb'] as $f) {
    try { $f(); echo "no throw\n"; } catch (LogicException $e) {
        $p = $e->getPrevious();
        echo get_class($e), ":", $e->getMessage(), " previous ", get_class($p), ": ", $p->getMessage(), "\n";
    }
}
try { gs(); echo "no throw\n"; } catch (LogicException $e) { echo "logic ", $e->getMessage(), "\n"; }
echo "end\n";
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "LogicException:d previous TypeError: gs(): Return value must be of type string, D returned\nLogicException:d previous TypeError: gf(): Return value must be of type float, D returned\nLogicException:d previous TypeError: gb(): Return value must be of type bool, D returned\nlogic d\nend\n");
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// A refused `string`/`float`/`bool` return releases the returned temporary, and the exception
/// its destructor throws reaches the returning function's own `catch`: the checker keeps the
/// catch's assignments for the code after it, and an uncaught one carries the `TypeError` as its
/// previous exception.
#[test]
fn test_refused_return_cleanup_exception_reaches_the_local_catch() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
class D { public function __destruct() { throw new LogicException("dtor"); } }
function mk(): mixed { return new D; }
function gs(): string {
    $w = null;
    try { return mk(); } catch (LogicException $e) { $w = "caught " . $e->getMessage(); }
    echo strlen($w), " ", $w, "\n";
    return "after";
}
function gb(): bool {
    $w = 1;
    try { return mk(); } catch (LogicException $e) { $w = "caught"; } finally { echo "fin\n"; }
    echo strlen($w), "\n";
    return false;
}
function gf(): float {
    try { return mk(); } catch (TypeError $e) { echo "TypeError ", get_class($e->getPrevious() ?? $e), "\n"; }
    return 2.5;
}
var_dump(gs(), gb());
try { var_dump(gf()); } catch (LogicException $e) { echo "outer ", $e->getMessage(), " prev=", get_class($e->getPrevious()), "\n"; }
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "11 caught dtor\nfin\n6\nstring(5) \"after\"\nbool(false)\nouter dtor prev=TypeError\n");
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}
