//! Purpose:
//! Regression tests for php parity defects found while compiling Symfony under `--web`:
//! 1. Static-local containers: a hash static written by key and returned survives later calls
//!    (it was returned as a movable owner, and converted to mixed in place), and a null static
//!    later assigned an array keeps its relocated payload.
//! 2. Object handles: `new A(new B())` numbers the outer object first, as php does.
//! 3. Reflection objects: dumps, casts and json show only php's `name`/`class` properties, and
//!    class-constant and enum-case reflectors carry `class` and php's exceptions.
//! 4. Lazily filled reflection members: parameters, their count and variadic element types.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected output was captured from php 8.5 running the same source.

use crate::support::*;

/// A keyed write into a hash static, returned from the function, must leave the static intact
/// for the next call; the returned copy must not be the static itself.
#[test]
fn test_static_hash_local_survives_being_returned() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function hits(string $k) { static $h = ["a" => 0]; $h[$k] = ($h[$k] ?? 0) + 1; $h["a"]++; return $h; }
hits("x"); hits("x");
echo json_encode(hits("y")), "\n";
function h1(string $k) { static $h = ["a" => 0]; $h[$k] = 1; return $h; }
h1("x"); echo json_encode(h1("y")), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "{\"a\":3,\"x\":2,\"y\":1}\n{\"a\":0,\"x\":1,\"y\":1}\n");
}

/// A static initialized to null and assigned an array on first use keeps growing across calls,
/// and an element increment through a static persists.
#[test]
fn test_null_then_array_static_keeps_its_elements() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function bag() { static $t = null; if ($t === null) { $t = []; } $t[] = count($t); return count($t); }
for ($i = 0; $i < 20; $i++) { bag(); }
var_dump(bag());
function st() { static $s = [0]; $s[0]++; return $s[0]; }
st(); var_dump(st());
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "int(21)\nint(2)\n");
}

/// php allocates the object under construction before evaluating its constructor arguments, so
/// the outer object takes the lower handle.
#[test]
fn test_new_with_object_argument_numbers_the_outer_object_first() {
    let out = compile_and_run(
        r#"<?php
class B {} class A { public function __construct(public $b = null) {} }
function mk() { return new B(); }
$a = new A(new B());
$c = new A(mk());
var_dump(spl_object_id($a), spl_object_id($a->b), spl_object_id($c), spl_object_id($c->b));
var_dump(new A(new B()));
"#,
    );
    assert_eq!(
        out,
        "int(1)\nint(2)\nint(3)\nint(4)\nobject(A)#5 (1) {\n  [\"b\"]=>\n  object(B)#6 (0) {\n  }\n}\n"
    );
}

/// Built-in reflection objects expose only php's declared properties to every dump and cast.
#[test]
fn test_reflection_objects_dump_only_their_php_properties() {
    let out = compile_and_run(
        r#"<?php
class P { const K = 1; public int $a = 1; public function m(int $x): int { return $x; } }
enum Suit: string { case H = "h"; }
$r = new ReflectionClass("P");
var_dump($r);
print_r(new ReflectionMethod("P", "m")); echo "\n";
echo json_encode([new ReflectionProperty("P", "a"), new ReflectionClassConstant("P", "K"), (new ReflectionEnum(Suit::class))->getCase("H")]), "\n";
var_dump(get_object_vars($r), (array) new ReflectionMethod("P", "m"));
var_export(new ReflectionFunction("strlen")); echo "\n";
$s = new ReflectionClass("P");
$s->getMethods();
var_dump($r == $s);
"#,
    );
    assert_eq!(
        out,
        r#"object(ReflectionClass)#1 (1) {
  ["name"]=>
  string(1) "P"
}
ReflectionMethod Object
(
    [name] => m
    [class] => P
)

[{"name":"a","class":"P"},{"name":"K","class":"P"},{"name":"H","class":"Suit"}]
array(1) {
  ["name"]=>
  string(1) "P"
}
array(2) {
  ["name"]=>
  string(1) "m"
  ["class"]=>
  string(1) "P"
}
\ReflectionFunction::__set_state(array(
   'name' => 'strlen',
))
bool(true)
"#
    );
}

/// Class-constant and enum-case reflectors name their declaring class and throw php's messages.
#[test]
fn test_class_constant_and_enum_case_reflectors() {
    let out = compile_and_run(
        r#"<?php
class P { const K = 1; } class Q extends P {}
$c = new ReflectionClassConstant("Q", "K");
var_dump($c->class, $c->getDeclaringClass()->getName());
var_dump((new ReflectionClass("Q"))->getReflectionConstant("K")->class);
enum E { case A; } enum F: int { case B = 2; }
$u = new ReflectionEnumUnitCase(E::class, "A");
var_dump($u->class, $u->getValue(), $u->getEnum()->getName());
$b = new ReflectionEnumBackedCase(F::class, "B");
var_dump($b->getBackingValue(), $b->class);
try { new ReflectionEnumBackedCase(E::class, "A"); } catch (ReflectionException $e) { echo $e->getMessage(), "\n"; }
try { new ReflectionEnumUnitCase(P::class, "K"); } catch (ReflectionException $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "string(1) \"P\"\nstring(1) \"P\"\nstring(1) \"P\"\nstring(1) \"E\"\nenum(E::A)\nstring(1) \"E\"\nint(2)\nstring(1) \"F\"\nEnum case E::A is not a backed case\nConstant P::K is not a case\n"
    );
}

/// Lazily filled parameters report php's count, names and the variadic ELEMENT type.
#[test]
fn test_lazy_reflection_parameters() {
    let out = compile_and_run(
        r#"<?php
class S { public function __construct(string ...$tags) {} public function two(int $a, ?S $b = null) {} }
function f(int ...$n) {}
$p = (new ReflectionMethod(S::class, "__construct"))->getParameters()[0];
echo $p->getType(), " ", $p->getType()->getName(), " ", var_export($p->isVariadic(), true), "\n";
echo (new ReflectionFunction("f"))->getParameters()[0]->getType(), "\n";
$m = new ReflectionMethod(S::class, "two");
echo $m->getNumberOfParameters(), " ", $m->getNumberOfRequiredParameters(), "\n";
foreach ($m->getParameters() as $param) { echo $param->getPosition(), ":", $param->getName(), ":", $param->getType(), "\n"; }
echo $m->getNumberOfParameters(), "\n";
"#,
    );
    assert_eq!(out, "string string true\nint\n2 1\n0:a:int\n1:b:?S\n2\n");
}

/// `isset()`, `empty()` and `??` reach through a null receiver at any depth: a variable that is
/// statically null, one indexed first, and one returned null all answer false / the default,
/// where elephc refused to compile the nested forms and crashed on `$null->p ?? $d`.
#[test]
fn test_null_probes_reach_through_null_receivers() {
    let out = compile_and_run(
        r#"<?php
class N { public ?N $next = null; public int $v = 3; }
function f() { return null; }
$o = null;
echo $o->a ?? "d1", "\n";
echo $o->a->b ?? "d2", "\n";
echo $o['k']->b ?? "d3", "\n";
$p = f();
echo $p->x->y ?? "d4", "\n";
$n = new N();
echo $n->next->v ?? "d5", "\n";
echo $n->next->next->v ?? "d6", "\n";
var_dump(isset($o->a->b), isset($o["k"]->a), isset($p->x->y), isset($n->next->v), empty($p->x->y));
$o ??= 5;
var_dump($o);
"#,
    );
    assert_eq!(
        out,
        "d1\nd2\nd3\nd4\nd5\nd6\nbool(false)\nbool(false)\nbool(false)\nbool(false)\nbool(true)\nint(5)\n"
    );
}
