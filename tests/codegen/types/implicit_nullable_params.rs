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
echo $cl(), " ", $cl(null), " ", $cl(21), " ", gettype($cl(21)), "\n";
echo $ar(), " ", $ar("v"), "\n";
"#,
    );
    assert_eq!(
        out,
        "NULL\nstring(3) \"lbl\"\nanon n\nnone c=7\nunsized size 2\nnull null 42 integer\ndflt v\n"
    );
}

/// Verifies an attribute imported under the alias `Export` is not the `#[Export]` attribute: its
/// function widens like any other, as php 8.5 runs it.
#[test]
fn test_implicitly_nullable_param_with_attribute_aliased_as_export() {
    let out = compile_and_run(
        r#"<?php
namespace App;
use Foo\Marker as Export;
#[Export] function f(string $x = null) { var_dump($x); }
f(); f("v");
"#,
    );
    assert_eq!(out, "NULL\nstring(1) \"v\"\n");
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
    assert!(stderr.contains("main.php:3:24]: App\\f()"), "{}", stderr);
}

/// Verifies the deprecation follows the `--php-version` profile: php 8.3 accepts `T $p = null`
/// silently and php 8.4 deprecates it, while the parameter is implicitly nullable under both.
#[test]
fn test_implicitly_nullable_param_deprecation_starts_with_php_8_4() {
    let source = "<?php\nfunction f(int $x = null) { return $x; }\nvar_dump(f());\n";
    for (version, expected) in [("8.2", 0), ("8.3", 0), ("8.4", 1)] {
        let stderr = compile_cli_file_with_flags_capturing_stderr(source, &["--php-version", version]);
        let deprecations = stderr.lines().filter(|line| line.starts_with("deprecated[")).count();
        assert_eq!(deprecations, expected, "--php-version {}: {}", version, stderr);
    }
}

/// Verifies an included file's deprecation is reported once per declaration, as php 8.5 does,
/// although the resolver parses the file twice (discovery, then expansion) under two source
/// identities.
#[test]
fn test_implicitly_nullable_param_in_included_file_reports_once() {
    let dir = make_cli_test_dir("elephc_cli_test_implicit_nullable_include");
    fs::write(dir.join("lib.php"), "<?php\nfunction f(int $x = null) { return $x; }\n").unwrap();
    let main = dir.join("main.php");
    fs::write(&main, "<?php\ninclude \"lib.php\";\nvar_dump(f());\n").unwrap();
    let output = elephc_cli_command(&dir).arg("--check").arg(&main).output().unwrap();
    let _ = fs::remove_dir_all(&dir);
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success(), "{}", stderr);
    let deprecations: Vec<&str> =
        stderr.lines().filter(|line| line.starts_with("deprecated[")).collect();
    assert_eq!(deprecations.len(), 1, "{}", stderr);
    assert!(deprecations[0].contains("lib.php:2:21]: f(): Implicitly marking parameter $x"), "{}", stderr);
}

/// Verifies each deprecation is located at its parameter's `null` default, as php-src reports the
/// parameter's line, and that the deprecations follow source order even though anonymous classes
/// are hoisted to the end of the program.
#[test]
fn test_implicitly_nullable_param_deprecation_points_at_each_parameter() {
    let stderr = compile_cli_file_with_flags_capturing_stderr(
        r#"<?php
function f(
    int $a = 1,
    string $b = null,
    int $c = null
) { return [$a, $b, $c]; }
$o = new class { public function m(int $x = null) { return $x; } };
function g(int $q = null) { return $q; }
var_dump(f(), $o->m(), g());
"#,
        &[],
    );
    let deprecations: Vec<&str> = stderr
        .lines()
        .filter(|line| line.starts_with("deprecated["))
        .map(|line| {
            let location = line.split_once("]: ").expect("diagnostic location").0;
            location.rsplit('/').next().expect("file name")
        })
        .collect();
    assert_eq!(
        deprecations,
        vec!["main.php:4:17", "main.php:5:14", "main.php:7:45", "main.php:8:21"],
        "{}",
        stderr
    );
}

/// Verifies an anonymous class's methods are named as php 8.5 names them in the deprecation:
/// `class@anonymous`, or the resolved parent or first interface name followed by `@anonymous`,
/// with no method suffix, since php-src's name stops at the NUL byte after `@anonymous`.
#[test]
fn test_implicitly_nullable_param_deprecation_names_anonymous_classes_like_php() {
    let stderr = compile_cli_file_with_flags_capturing_stderr(
        r#"<?php
namespace N;
use Other\Base as B;
interface I {}
class Base {}
$a = new class implements I { public function m(int $x = null) { return $x; } };
$b = new class extends Base { public function m(int $x = null) { return $x; } };
$c = new class { public function m() { return function (int $y = null) { return $y; }; } };
var_dump($a->m(), $b->m(), ($c->m())());
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
            format!("N\\I@anonymous(): Implicitly marking parameter $x {}", suffix),
            format!("N\\Base@anonymous(): Implicitly marking parameter $x {}", suffix),
            format!("{{closure:class@anonymous(): Implicitly marking parameter $y {}", suffix),
        ],
        "{}",
        stderr
    );
}

/// Verifies `ptr` and `ptr<T>` parameters with a `null` default keep their pointer type, whose
/// null pointer already is null, with no PHP deprecation.
#[test]
fn test_null_default_pointer_params_stay_pointers() {
    let out = compile_and_run(
        r#"<?php
class C {}
function p(ptr $q = null) { echo ptr_is_null($q) ? "null\n" : "set\n"; }
function t(ptr<C> $q = null) { echo ptr_is_null($q) ? "null\n" : "set\n"; }
p(); t(); p(ptr_null());
"#,
    );
    assert_eq!(out, "null\nnull\nnull\n");
    let stderr = compile_cli_file_with_flags_capturing_stderr(
        "<?php\nfunction p(ptr $q = null) { return ptr_is_null($q); }\nvar_dump(p());\n",
        &[],
    );
    assert!(!stderr.contains("deprecated["), "{}", stderr);
}

/// Verifies a deprecation from a class file autoloaded while checking is still reported when
/// the checker then rejects the program, ahead of the error, as php prints it first.
#[test]
fn test_implicitly_nullable_param_deprecation_from_autoload_precedes_check_error() {
    let dir = make_cli_test_dir("elephc_cli_test_implicit_nullable_autoload");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("composer.json"), r#"{"autoload": {"psr-4": {"App\\": "src/"}}}"#).unwrap();
    fs::write(
        dir.join("src/K.php"),
        "<?php\nnamespace App;\nclass K { public function m(int $x = null) { return $x; } }\n",
    )
    .unwrap();
    let main = dir.join("main.php");
    fs::write(
        &main,
        "<?php\n(new App\\K())->m();\nfunction f(int $i): int { return $i; }\necho f([1]);\n",
    )
    .unwrap();
    let output = elephc_cli_command(&dir).arg("--check").arg(&main).output().unwrap();
    let _ = fs::remove_dir_all(&dir);
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(!output.status.success(), "{}", stderr);
    let deprecation = stderr.find("App\\K::m(): Implicitly marking parameter $x");
    let error = stderr.find("error[");
    assert!(
        matches!((deprecation, error), (Some(d), Some(e)) if d < e),
        "{}",
        stderr
    );
}
