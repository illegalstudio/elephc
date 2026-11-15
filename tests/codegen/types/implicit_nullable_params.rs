//! Purpose:
//! End-to-end coverage for implicitly nullable parameters: a non-nullable declared type with a
//! literal `null` default (`string $s = null`) is `?string` in PHP, so null arrives as null.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected stdout is verbatim php 8.5 output with its compile-time `Deprecated:` lines
//!   removed; elephc reports those as compiler deprecations, pinned by the CLI test below.
//! - By-reference parameters keep their declared type (see `ScopePass::transform_param_type`).

use crate::support::*;

/// Verifies scalar, array, class and union parameters with a `null` default receive null,
/// whether the argument is omitted or passed, and still bind real values.
#[test]
fn test_implicitly_nullable_function_params_hold_null() {
    let out = compile_and_run(
        r#"<?php
function s(string $s = null) { var_dump($s, is_null($s), $s ?? "d", $s === null); }
function i(int $i = null) { var_dump($i); }
function fl(float $f = null) { var_dump($f); }
function b(bool $b = null) { var_dump($b); }
function a(array $a = null) { var_dump($a === null ? "no array" : count($a)); }
class P {}
function o(P $p = null) { echo $p === null ? "no P" : get_class($p), "\n"; }
function u(int|string $u = null) { var_dump($u); }
function r(int &$r = null) { $r = ($r ?? 0) + 1; }
s(); s(null); s("x");
i(); i(null); i(3);
fl(); fl(1.5);
b(); b(false);
a(); a([1, 2]);
o(); o(new P());
u(); u("k"); u(2);
r(); $v = 4; r($v); var_dump($v);
"#,
    );
    assert_eq!(
        out,
        "NULL\nbool(true)\nstring(1) \"d\"\nbool(true)\n\
NULL\nbool(true)\nstring(1) \"d\"\nbool(true)\n\
string(1) \"x\"\nbool(false)\nstring(1) \"x\"\nbool(false)\n\
NULL\nNULL\nint(3)\n\
NULL\nfloat(1.5)\n\
NULL\nbool(false)\n\
string(8) \"no array\"\nint(2)\n\
no P\nP\n\
NULL\nstring(1) \"k\"\nint(2)\n\
int(5)\n"
    );
}

/// Verifies methods, static methods, constructors, interface and trait methods, closures and
/// arrow functions widen the same way.
#[test]
fn test_implicitly_nullable_method_and_closure_params_hold_null() {
    let out = compile_and_run(
        r#"<?php
interface Named { public function name(string $n = null): string; }
trait Counts { public function count(int $c = null): string { return $c === null ? "none" : "c=$c"; } }
class Box implements Named {
    use Counts;
    public ?string $label;
    public function __construct(string $label = null) { $this->label = $label; }
    public function name(string $n = null): string { return $n ?? "anon"; }
    public static function make(int $size = null): string { return is_null($size) ? "unsized" : "size $size"; }
}
$b = new Box();
var_dump($b->label);
$b = new Box("lbl");
var_dump($b->label);
echo $b->name(), " ", $b->name("n"), "\n";
echo $b->count(), " ", $b->count(7), "\n";
echo Box::make(), " ", Box::make(2), "\n";
$cl = function (int $i = null) { return $i === null ? "null" : $i * 2; };
$ar = fn(string $s = null) => $s ?? "dflt";
echo $cl(), " ", $cl(null), " ", $cl(21), "\n";
echo $ar(), " ", $ar("v"), "\n";
"#,
    );
    assert_eq!(
        out,
        "NULL\nstring(3) \"lbl\"\nanon n\nnone c=7\nunsized size 2\nnull null 42\ndflt v\n"
    );
}

/// Verifies the compiler reports php 8.5's compile-time deprecation once per implicitly
/// nullable parameter, naming the function, method or closure as php-src does, and stays
/// silent for explicitly nullable, `mixed`, `T|null`, untyped and non-null defaults.
#[test]
fn test_implicitly_nullable_params_report_php_deprecation() {
    let stderr = compile_cli_file_with_flags_capturing_stderr(
        r#"<?php
namespace App;
function f(string $s = null, ?int $i = null, mixed $m = null, int|null $n = null, $x = null, int $j = 0) {
    $g = function (float $v = NULL) { return $v; };
    return $g() ?? $s ?? $i ?? $m ?? $n ?? $x ?? $j;
}
class C { public function m(int $i = null) { return $i; } }
var_dump(f(), (new C())->m());
"#,
        &[],
    );
    let deprecations: Vec<&str> = stderr
        .lines()
        .filter(|line| line.starts_with("deprecated["))
        .map(|line| line.split_once("]: ").expect("diagnostic location").1)
        .collect();
    let suffix = "as nullable is deprecated, the explicit nullable type must be used instead";
    assert_eq!(
        deprecations,
        vec![
            format!("App\\f(): Implicitly marking parameter $s {}", suffix),
            format!("{{closure:App\\f():4}}(): Implicitly marking parameter $v {}", suffix),
            format!("App\\C::m(): Implicitly marking parameter $i {}", suffix),
        ],
        "{}",
        stderr
    );
    assert!(stderr.contains("main.php:3:1]: App\\f()"), "{}", stderr);
}
