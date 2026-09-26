//! Purpose:
//! Pins behaviour reported broken in open issues that is fixed on main, one fixture per issue.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each fixture is the issue's own reproduction, trimmed; the expected stdout was captured from PHP 8.5.

use super::*;

/// A by-reference `foreach` over a by-reference `array` parameter failed EIR validation
/// (PhpTypeMismatch). Pins that it compiles and writes through to the caller's nested element.
/// Regression for #643.
#[test]
fn test_issue_643_by_ref_foreach_inside_by_ref_array_param() {
    let out = compile_and_run(
        r#"<?php
function f(array &$r) {
    foreach ($r as &$v) { $v = $v * 2; }
}
$a = [[1, 2], [3, 4]];
f($a[0]);
echo implode(",", $a[0]), "|", implode(",", $a[1]), "\n";
"#,
    );
    assert_eq!(out, "2,4|3,4\n");
}

/// `array_filter()` renumbered surviving keys and rejected the one-argument form. Pins key
/// preservation for int and string receivers and truthiness filtering without a callback.
/// Regression for #672.
#[test]
fn test_issue_672_array_filter_preserves_keys_and_single_arg_form() {
    let out = compile_and_run(
        r#"<?php
print_r(array_filter([1, 2, 3], fn($v) => $v !== 2));
print_r(array_filter(["a", "b", "c"], fn($v) => $v !== "b"));
var_dump(array_filter([0, 0]) === []);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "Array\n(\n    [0] => 1\n    [2] => 3\n)\nArray\n(\n    [0] => a\n",
            "    [2] => c\n)\nbool(true)\n",
        )
    );
}

/// `array_slice()` and `array_chunk()` refused every associative receiver at compile time.
/// Pins string/int key handling in both `preserve_keys` modes and assoc chunking.
/// Regression for #683.
#[test]
fn test_issue_683_array_slice_and_chunk_accept_assoc_receivers() {
    let out = compile_and_run(
        r#"<?php
print_r(array_slice(["x" => 1, "y" => 2, "z" => 3], 1, 2, true));
print_r(array_slice(["x" => 1, "y" => 2, "z" => 3], 1, 2));
print_r(array_slice([5 => 1, 9 => 2, 12 => 3], 1, 2, true));
print_r(array_slice([5 => 1, 9 => 2, 12 => 3], 1, 2));
print_r(array_slice(["x" => "a", "y" => "b"], 0, 1, true));
print_r(array_slice(["a" => 1, 7 => 2, "b" => 3, 9 => 4], 1));
print_r(array_chunk(["x" => 1, "y" => 2], 1, true));
"#,
    );
    assert_eq!(
        out,
        concat!(
            "Array\n(\n    [y] => 2\n    [z] => 3\n)\nArray\n(\n    [y] => 2\n",
            "    [z] => 3\n)\nArray\n(\n    [9] => 2\n    [12] => 3\n)\nArray\n(\n",
            "    [0] => 2\n    [1] => 3\n)\nArray\n(\n    [x] => a\n)\nArray\n(\n",
            "    [0] => 2\n    [b] => 3\n    [1] => 4\n)\nArray\n(\n",
            "    [0] => Array\n        (\n            [x] => 1\n        )\n\n",
            "    [1] => Array\n        (\n            [y] => 2\n        )\n\n)\n",
        )
    );
}

/// Calling a function with a defaulted by-reference parameter and omitting the argument failed
/// EIR lowering. Pins int and string defaults, plus write-back when an argument is given.
/// Regression for #704.
#[test]
fn test_issue_704_defaulted_by_ref_param_call_omits_argument() {
    let out = compile_and_run(
        r#"<?php
function bump(mixed &$x = 5): int {
    $x = $x + 1;
    return $x;
}
function tag(mixed &$x = "seed"): string {
    $x = $x . "!";
    return $x;
}
echo bump(), "\n";
$v = 1;
echo bump($v), " ", $v, "\n";
echo tag(), "\n";
$t = "abc";
echo tag($t), " ", $t, "\n";
"#,
    );
    assert_eq!(out, "6\n2 2\nseed!\nabc! abc!\n");
}

/// A guard that can never match (`is_string` on an int) narrowed the local to the guard's
/// type, so an int assignment inside the dead branch was a hard error. This pins that it compiles
/// and runs like PHP, for a capture, a plain local, and a parameter.
/// Regression for #773.
#[test]
fn test_issue_773_statically_false_guard_keeps_original_type() {
    let out = compile_and_run(
        r#"<?php
$m = 1;
$f = function (int $n) use ($m) { if (is_string($m)) { $m = 2; } $m = 4; return $m + $n; };
var_dump($f(2));

$k = 1;
if (is_string($k)) { $k = 2; }
var_dump($k);

function g(int $m): int {
    if (is_string($m)) { $m = 2; }
    return $m;
}
var_dump(g($argc + 6));
"#,
    );
    assert_eq!(out, "int(6)\nint(1)\nint(7)\n");
}

/// String appends made inside a `try` before a throw were lost on the unwind path, so the
/// function returned `|c|f`. This pins that the try-body writes are kept, as in PHP.
/// Regression for #775.
#[test]
fn test_issue_775_try_body_string_appends_survive_throw() {
    let out = compile_and_run(
        r#"<?php
function f(): string {
    $r = "";
    try {
        $r .= "A";
        $r .= "B";
        throw new RuntimeException("b");
    } catch (RuntimeException $e) {
        $r .= "|c";
    } finally {
        $r .= "|f";
    }
    return $r;
}
echo f(), "\n";
"#,
    );
    assert_eq!(out, "AB|c|f\n");
}

/// A heap string stored through a `use (&$m)` capture was visible inside the closure but the
/// caller read NULL afterwards. This pins the writeback at top level and inside a function.
/// Regression for #776.
#[test]
fn test_issue_776_by_ref_capture_string_writeback_reaches_caller() {
    let out = compile_and_run(
        r#"<?php
$m = "a";
$f = function (int $n) use (&$m) { $m = "b"; if ($n > 1) { $m = "c"; } return $m; };
var_dump($f(2));
var_dump($m);

function run(int $k): string {
    $m = "a" . $k;
    $f = function (int $n) use (&$m) {
        $m = "b" . $n;
        if ($n > 1) { $m = "c" . $n; }
        return $m;
    };
    $r = $f($k);
    return $r . "/" . $m;
}
echo run(1), "\n";
echo run(5), "\n";
"#,
    );
    assert_eq!(out, "string(1) \"c\"\nstring(1) \"c\"\nb1/b1\nc5/c5\n");
}

/// A local assigned inside `try` still held its pre-try value in `catch`. This pins that the
/// catch sees the try-body write, at top level and inside a function.
/// Regression for #800.
#[test]
fn test_issue_800_local_written_in_try_visible_in_catch() {
    let out = compile_and_run(
        r#"<?php
$x = 'init';
try {
    $x = 'written';
    throw new Exception('x');
} catch (Exception $e) {
    echo $x, "\n";
}

function f(int $n): string {
    $x = 'init';
    $i = 0;
    try {
        $x = 'written' . $n;
        $i = $n * 10;
        throw new Exception('e');
    } catch (Exception $e) {
        return $x . ' ' . $i;
    }
}
echo f($argc), "\n";
"#,
    );
    assert_eq!(out, "written\nwritten1 10\n");
}

/// `sort`, `rsort`, `usort` and sorting a copy segfaulted on an array written through a
/// by-reference parameter. This pins the sorted results and counts.
/// Regression for #819.
#[test]
fn test_issue_819_sort_array_filled_through_by_ref_param() {
    let out = compile_and_run(
        r#"<?php
function fillByRef(array &$out): void { $out = [31, 30]; }

$a = [];
fillByRef($a);
printf("before=%s count=%d\n", implode(',', $a), count($a));
sort($a);
printf("after=%s count=%d\n", implode(',', $a), count($a));

$b = [];
fillByRef($b);
rsort($b);
echo "rsort: ", implode(',', $b), " count=", count($b), "\n";

$c = [];
fillByRef($c);
usort($c, fn($x, $y) => $x <=> $y);
echo "usort: ", implode(',', $c), " count=", count($c), "\n";

$d = [];
fillByRef($d);
$e = $d;
sort($e);
echo "copy: ", implode(',', $e), " orig=", implode(',', $d), " count=", count($e), "\n";
"#,
    );
    assert_eq!(
        out,
        "before=31,30 count=2\nafter=30,31 count=2\nrsort: 31,30 count=2\n\
         usort: 30,31 count=2\ncopy: 30,31 orig=31,30 count=2\n"
    );
}

/// PHP 8.5 `clone($obj, [...])` failed to parse. This pins that it clones, applies literal and
/// runtime override arrays, and leaves the original untouched.
/// Regression for #824.
#[test]
fn test_issue_824_clone_with_applies_property_overrides() {
    let out = compile_and_run(
        r#"<?php

final class Subject
{
    public int $value = 1;
    public string $name = "orig";
}

$original = new Subject();
$copy = clone($original, ['value' => 2]);
echo $original->value, " ", $copy->value, " ", $copy->name, "\n";

$props = ['value' => $argc + 10, 'name' => "n" . $argc];
$c2 = clone($original, $props);
echo $c2->value, " ", $c2->name, " ", $original->name, "\n";
var_dump($copy !== $original);
"#,
    );
    assert_eq!(out, "1 2 orig\n11 n1 orig\nbool(true)\n");
}

/// Reassigning a string parameter to `Method::tryFrom(...)` was a type error even without
/// `--strict-locals`. This pins that the default mode compiles and runs it like PHP.
/// Regression for #831.
#[test]
fn test_issue_831_parameter_retyped_to_enum_without_strict_locals() {
    let out = compile_and_run(
        r#"<?php

enum Method: string { case GET = "GET"; case POST = "POST"; }

function parse(string $method): ?Method
{
    $method = Method::tryFrom($method);
    return $method;
}

var_dump(parse("GET"));
var_dump(parse("PO" . "ST"));
var_dump(parse("nope" . $argc));
"#,
    );
    assert_eq!(out, "enum(Method::GET)\nenum(Method::POST)\nNULL\n");
}

/// A fluent chain returning `$this` through an interface corrupted a string stored in a nested
/// object's array (`Tempest-` instead of `Tempest-on-Elephc`). This pins the full values.
/// Regression for #835.
#[test]
fn test_issue_835_fluent_interface_calls_keep_nested_object_strings() {
    let out = compile_and_run(
        r#"<?php

interface Response
{
    public function addHeader(string $key, string $value): Response;
    public function getHeaders(): array;
}

final class HeaderValue
{
    public function __construct(
        public string $name,
        public array $values = [],
    ) {}

    public function add(mixed $value): void
    {
        $this->values[] = $value;
    }
}

final class ConcreteResponse implements Response
{
    private array $headers = [];

    public function addHeader(string $key, string $value): Response
    {
        $this->headers[$key] ??= new HeaderValue($key);
        $this->headers[$key]->add($value);
        return $this;
    }

    public function getHeaders(): array
    {
        return $this->headers;
    }
}

$response = (new ConcreteResponse())
    ->addHeader("Content-Type", "text/plain")
    ->addHeader("X-Test", "Tempest-on-Elephc");

foreach ($response->getHeaders() as $header) {
    foreach ($header->values as $value) {
        echo $header->name, ": ", $value, "\n";
    }
}
"#,
    );
    assert_eq!(out, "Content-Type: text/plain\nX-Test: Tempest-on-Elephc\n");
}

/// A class or interface constant declaration listing several names (`const A = 1, B = 2;`)
/// was a parse error; this pins that every grouped name is declared with its own value.
/// Regression for #848.
#[test]
fn test_issue_848_grouped_class_constants() {
    let out = compile_and_run(
        r#"<?php
class Types {
    public const JPEG = 2, PNG = 3, GIF = 1;
}
interface I { const A = 'a', B = 'b'; }
class K implements I {
    private const X = 10, Y = 20;
    final protected const int D = 4, E = 5;
    public static function s(): int { return self::X + self::Y + static::D + self::E; }
}
echo Types::JPEG, Types::PNG, Types::GIF, "\n";
echo I::A, I::B, K::B, "\n";
echo K::s(), "\n";
"#,
    );
    assert_eq!(out, "231\nabb\n39\n");
}

/// Storing a Closure in a static property passed --check but failed in the EIR backend;
/// this pins storing, reading back, invoking and clearing closures in static slots.
/// Regression for #854.
#[test]
fn test_issue_854_closure_in_static_property() {
    let out = compile_and_run(
        r#"<?php
class Registry {
    private static ?Closure $callback = null;
    public static function set(Closure $callback): void { self::$callback = $callback; }
    public static function call(string $x): string {
        return self::$callback === null ? "none" : (self::$callback)($x);
    }
    public static function clear(): void { self::$callback = null; }
}
echo Registry::call("a"), "\n";
Registry::set(static fn () => 'ok');
echo Registry::call("b"), "\n";
$p = $argc > 5 ? "[" : "<";
Registry::set(function (string $s) use ($p) { return $p . $s . ">"; });
echo Registry::call("c"), "\n";
Registry::clear();
echo Registry::call("d"), "\n";
class Macro {
    protected static array $macros = [];
    public static function macro(string $n, Closure $c): void { static::$macros[$n] = $c; }
    public static function run(string $n, int $v): int { return (static::$macros[$n])($v); }
}
Macro::macro("dbl", fn (int $v) => $v * 2);
echo Macro::run("dbl", 21), "\n";
class C2 { public static Closure $f; }
C2::$f = fn () => "typed";
echo (C2::$f)(), "\n";
"#,
    );
    assert_eq!(out, "none\nok\n<c>\nnone\n42\ntyped\n");
}

/// error_reporting(), set/restore_error_handler() and set/restore_exception_handler() were
/// undefined; this pins their return values, handler stacking and uncaught dispatch.
/// Regression for #861.
#[test]
fn test_issue_861_error_and_exception_handler_apis() {
    let out = compile_and_run(
        r#"<?php
$old = error_reporting(E_ALL);
echo "old_is_int:", is_int($old) ? "y" : "n", "\n";
echo "now:", error_reporting() === E_ALL ? "all" : "other", "\n";
$prev = error_reporting(E_ALL & ~E_NOTICE);
echo "prev_all:", $prev === E_ALL ? "y" : "n", "\n";
echo "masked:", error_reporting() === (E_ALL & ~E_NOTICE) ? "y" : "n", "\n";
error_reporting(E_ALL);

$r1 = set_error_handler(static fn () => true);
echo "first_prev_null:", $r1 === null ? "y" : "n", "\n";
$h2 = function (int $no, string $str) { echo "handler2:", $str, "\n"; return true; };
$r2 = set_error_handler($h2);
echo "second_prev_closure:", $r2 instanceof Closure ? "y" : "n", "\n";
trigger_error("custom warning", E_USER_WARNING);
echo "restore:", var_export(restore_error_handler(), true), "\n";
echo "restore2:", var_export(restore_error_handler(), true), "\n";

$e1 = set_exception_handler(static fn (Throwable $e) => null);
echo "exc_prev_null:", $e1 === null ? "y" : "n", "\n";
echo "exc_restore:", var_export(restore_exception_handler(), true), "\n";
set_exception_handler(function (Throwable $e) {
    echo "uncaught:", get_class($e), ":", $e->getMessage(), "\n";
});
echo "before throw\n";
throw new RuntimeException("boom");
"#,
    );
    assert_eq!(
        out,
        "old_is_int:y\n\
         now:all\n\
         prev_all:y\n\
         masked:y\n\
         first_prev_null:y\n\
         second_prev_closure:y\n\
         handler2:custom warning\n\
         restore:true\n\
         restore2:true\n\
         exc_prev_null:y\n\
         exc_restore:true\n\
         before throw\n\
         uncaught:RuntimeException:boom\n"
    );
}

/// clone through a base-typed variable built the copy with the static class; this pins that
/// the copy keeps the runtime subclass, its properties and its method overrides.
/// Regression for #947.
#[test]
fn test_issue_947_clone_keeps_runtime_class() {
    let out = compile_and_run(
        r#"<?php
class A {
    public function __clone() { echo "clone:", get_class($this), "\n"; }
    public function who(): string { return "A::who"; }
}
class B extends A {
    public string $extra = "bx";
    public array $list = [1, 2];
    public function who(): string { return "B::who " . $this->extra . count($this->list); }
}
class Cc extends B {}
function f(A $a): A { $c = clone $a; echo get_class($c), "\n"; return $c; }
function g(object $o): object { return clone $o; }
$b = new B();
$b->extra = "changed";
$b->list[] = 3;
$c = f($b);
echo $c->who(), "\n";
echo $c instanceof B ? "is B" : "not B", "\n";
$b->extra = "orig";
echo $b->who(), " | ", $c->who(), "\n";
$cc = new Cc();
$e = f($cc);
echo $e->who(), "\n";
$h = g($cc);
echo get_class($h), "\n";
"#,
    );
    assert_eq!(
        out,
        "clone:B\n\
         B\n\
         B::who changed3\n\
         is B\n\
         B::who orig3 | B::who changed3\n\
         clone:Cc\n\
         Cc\n\
         B::who bx2\n\
         clone:Cc\n\
         Cc\n"
    );
}

/// A spread following an integer-keyed spread through a dynamic target renumbered from zero
/// and overwrote arguments. Pins PHP's positional binding for each perimeter row.
/// Regression for #1050.
#[test]
fn test_issue_1050_keyed_spread_then_spread_through_dynamic_target() {
    let out = compile_and_run(
        r#"<?php
class C { public function __construct($a = "u", $b = "v", $c = "w") { echo "$a/$b/$c\n"; } }
function f($a = "u", $b = "v", $c = "w") { echo "$a/$b/$c\n"; }
function kn(): string { return "C"; }
function fnm(): string { return "f"; }
$k = kn();
$g = fnm();
$x1 = [5 => 2]; $y1 = [3, 4]; new $k(...$x1, ...$y1);
$x2 = [5 => 2]; $y2 = ["c" => 8]; new $k(...$x2, ...$y2);
$x3 = [3, 4]; $y3 = ["c" => 8]; new $k(...$x3, ...$y3);
$x4 = [5 => 2, 9 => 3]; new $k(...$x4);
$g(1, ...$x1);
$g(...$x1, ...$y1);
"#,
    );
    assert_eq!(out, "2/3/4\n2/v/8\n3/4/8\n2/3/w\n1/2/w\n2/3/4\n");
}

/// implode() on a by-value foreach value bound out of a nested array (array_chunk, literal,
/// parameter) segfaulted or printed wrong scalars. Pins PHP's output for each shape.
/// Regression for #1081.
#[test]
fn test_issue_1081_implode_on_foreach_bound_nested_array() {
    let out = compile_and_run(
        r#"<?php
$idx = [1, 2, 3, 4, 5];
echo "1:";
foreach (array_chunk($idx, 2) as $p => $items) { echo " " . implode(",", $items); }
echo "\n";
$c = array_chunk($idx, 2, true);
foreach ($c as $items) { echo implode(",", $items), ";"; }
echo "\n";
function nested(array $n): void {
    foreach ($n as $p) { echo implode(",", $p), ";"; }
    echo "\n";
}
nested([[1, 2], [3, 4]]);
foreach ([[1.5, 2.5], [3.5, 4.5]] as $p) { echo implode(",", $p), ";"; } echo "\n";
foreach ([[true, false]] as $p) { echo implode(",", $p), ";"; } echo "\n";
foreach ([["a" => 1, "b" => 2]] as $p) { echo join(",", $p), ";"; } echo "\n";
foreach ([[1, "b"], [3, "d"]] as $p) { $q = $p; echo implode(",", $q), ";"; } echo "\n";
"#,
    );
    assert_eq!(out, "1: 1,2 3,4 5\n1,2;3,4;5;\n1,2;3,4;\n1.5,2.5;3.5,4.5;\n1,;\n1,2;\n1,b;3,d;\n");
}
