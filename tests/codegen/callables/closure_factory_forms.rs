//! Purpose:
//! Covers the callable forms `Closure::fromCallable()` accepts and the identity of the closures
//! it returns.
//!
//! Called from:
//! - The codegen integration test suite.
//!
//! Key details:
//! - Every fixture runs with the EIR optimizer on and off, with heap debug enabled.
//! - Expected output is what PHP 8.5 prints for the unchanged source.

use crate::support::*;

/// Asserts PHP-equivalent stdout and a clean heap with the EIR optimizer on and off.
fn assert_factory_output(source: &str, expected: &str) {
    for ir_opt in [true, false] {
        let out = if ir_opt {
            compile_and_run_with_heap_debug(source)
        } else {
            without_ir_opt(|| compile_and_run_with_heap_debug(source))
        };
        assert!(out.success, "ir_opt={ir_opt} stdout={:?}\nstderr={}", out.stdout, out.stderr);
        assert_eq!(out.stdout, expected, "ir_opt={ir_opt}\n{}", out.stderr);
        assert!(
            out.stderr.contains("HEAP DEBUG: leak summary: clean"),
            "ir_opt={ir_opt}\n{}",
            out.stderr
        );
    }
}

/// A literal `"Class::method"` string names a static method, resolved with the visibility of
/// the class scope that calls the factory.
#[test]
fn test_closure_factory_literal_class_method_string() {
    assert_factory_output(r#"<?php
namespace App;
class K {
    public static function s(int $n): int { return $n + 3; }
    private static function hidden(): string { return "hidden"; }
    public static function expose(): \Closure { return \Closure::fromCallable("App\\K::hidden"); }
    public static function selfForm(): \Closure { return \Closure::fromCallable(self::class . "::hidden"); }
}
function f(): string { return "f"; }
$a = \Closure::fromCallable("App\\K::s"); echo $a(1), "\n";
$b = \Closure::fromCallable("\\App\\K::s"); echo $b(2), "\n";
$c = \Closure::fromCallable('App\K::s'); echo $c(3), "\n";
echo K::expose()(), "\n";
echo K::selfForm()(), "\n";
try { \Closure::fromCallable("App\\K::hidden"); echo "bad\n"; } catch (\TypeError $e) { echo "TypeError\n"; }
try { \Closure::fromCallable("App\\K::absent"); echo "bad\n"; } catch (\TypeError $e) { echo "TypeError\n"; }
$d = \Closure::fromCallable("App\\f"); echo $d(), "\n";
$u = \Closure::fromCallable("strtoupper"); echo $u("x"), "\n";
echo implode(",", array_map(\Closure::fromCallable("App\\K::s"), [1, 2])), "\n";
"#, "4\n5\n6\nhidden\nhidden\nTypeError\nTypeError\nf\nX\n4,5\n");
}

/// Two closure values are identical only when they are the same closure: the factory returns
/// the closure it was given, and every other conversion creates a new one.
#[test]
fn test_closure_identity_between_closure_values() {
    assert_factory_output(r#"<?php
function named(): int { return 1; }
class K { public function m(): int { return 2; } public static function s(): int { return 3; } }
function mk(): Closure { return function(): int { return 5; }; }
function same(Closure $x, Closure $y): bool { return $x === $y; }
function nsame(?Closure $x, ?Closure $y): bool { return $x === $y; }
$f = fn(): int => 1;
$g = Closure::fromCallable($f);
echo "factory-of-closure "; var_dump($g === $f, $g !== $f);
$h = $f;
echo "copy "; var_dump($h === $f);
$a = fn(): int => 1;
echo "distinct-arrow "; var_dump($a === $f);
$n1 = Closure::fromCallable("named"); $n2 = Closure::fromCallable("named");
echo "two-factories-named "; var_dump($n1 === $n2);
$n3 = Closure::fromCallable($n1);
echo "factory-of-factory "; var_dump($n3 === $n1);
$k = new K();
$m1 = Closure::fromCallable([$k, "m"]); $m2 = Closure::fromCallable([$k, "m"]);
echo "two-factories-method "; var_dump($m1 === $m2);
$fc1 = named(...); $fc2 = named(...);
echo "two-first-class "; var_dump($fc1 === $fc2);
echo "two-created "; var_dump(mk() === mk());
$list = [$f];
echo "array-element "; var_dump($list[0] === $f);
echo "params "; var_dump(same($f, $g), same($f, $a));
echo "nullable-params "; var_dump(nsame($f, $g), nsame($f, null), nsame(null, null), nsame($f, $a));
$loop = [];
for ($i = 0; $i < 2; $i++) { $loop[] = function(): int { return 7; }; }
echo "loop-created "; var_dump($loop[0] === $loop[1]);
$s1 = K::s(...); $s2 = Closure::fromCallable("K::s");
echo "static-forms "; var_dump($s1 === $s2);
"#, concat!(
        "factory-of-closure bool(true)\n",
        "bool(false)\n",
        "copy bool(true)\n",
        "distinct-arrow bool(false)\n",
        "two-factories-named bool(false)\n",
        "factory-of-factory bool(true)\n",
        "two-factories-method bool(false)\n",
        "two-first-class bool(false)\n",
        "two-created bool(false)\n",
        "array-element bool(true)\n",
        "params bool(true)\n",
        "bool(false)\n",
        "nullable-params bool(true)\n",
        "bool(false)\n",
        "bool(true)\n",
        "bool(false)\n",
        "loop-created bool(false)\n",
        "static-forms bool(false)\n",
    ));
}

/// Closures from the factory fill `Closure` and `?Closure` properties and array elements for
/// every callable shape, and a target that is not callable throws `TypeError`.
#[test]
fn test_closure_factory_typed_property_forms() {
    assert_factory_output(r#"<?php
final class Box {
    public ?Closure $cb = null;
    public Closure $strict;
    public function __construct(callable $c) { $this->strict = Closure::fromCallable($c); $this->cb = $this->strict; }
    public function call(int $n): int { return ($this->cb)($n) + ($this->strict)($n); }
}
function add1(int $n): int { return $n + 1; }
class M { public function __construct(private int $k) {} public function mul(int $n): int { return $n * $this->k; } public static function neg(int $n): int { return -$n; } }
$f = fn(int $n): int => $n * 2;
$g = Closure::fromCallable($f);
var_dump($g === $f);
var_dump($g instanceof Closure);
$b = new Box($f);
echo $b->call(5), "\n";
$b2 = new Box("add1"); echo $b2->call(5), "\n";
$b3 = new Box([new M(3), "mul"]); echo $b3->call(5), "\n";
$b4 = new Box(["M", "neg"]); echo $b4->call(5), "\n";
$b5 = new Box("M::neg"); echo $b5->call(5), "\n";
$b6 = new Box("strlen"); echo ($b6->cb)("abcd"), "\n";
var_dump($b6->cb instanceof Closure, is_callable($b6->cb), get_class($b6->strict));
$b6->cb = null;
var_dump($b6->cb === null, $b6->cb);
$list = [Closure::fromCallable("add1"), Closure::fromCallable([new M(4), "mul"]), Closure::fromCallable($f)];
foreach ($list as $c) { echo $c(10), " "; }
echo "\n", implode(",", array_map(Closure::fromCallable("add1"), [1, 2, 3])), "\n";
$missing = $argc > 5 ? "add1" : "nope_missing";
try { new Box($missing); } catch (TypeError $e) { echo get_class($e), "\n"; }
try { Closure::fromCallable([new M(1), "absent"]); } catch (TypeError $e) { echo get_class($e), "\n"; }
"#, concat!(
        "bool(true)\n",
        "bool(true)\n",
        "20\n",
        "12\n",
        "30\n",
        "-10\n",
        "-10\n",
        "4\n",
        "bool(true)\n",
        "bool(true)\n",
        "string(7) \"Closure\"\n",
        "bool(true)\n",
        "NULL\n",
        "11 40 20 \n",
        "2,3,4\n",
        "TypeError\n",
        "TypeError\n",
    ));
}
