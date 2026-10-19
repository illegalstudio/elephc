//! Purpose:
//! Integration tests for calls dispatched through an abstract, interface or overridden method
//! whose overrides pass or return values in different representations.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The call site is typed and compiled against the receiver's declared class, but runs the
//!   override the object carries; each override family must share one call ABI.

use super::*;

/// An untyped abstract method called through the abstract type returns what the child returned.
#[test]
fn test_abstract_untyped_return_through_parent_type() {
    let out = compile_and_run(
        r#"<?php
abstract class Shape { abstract public function tag($v); }
class Sq extends Shape { public function tag($v) { echo "Sq "; return $v; } }
class Ci extends Shape { public function tag($v) { echo "Ci "; return $v; } }
function go(Shape $s) { echo $s->tag(1), "\n"; var_dump($s->tag("s")); }
go(new Sq()); go(new Ci());
$q = new Sq(); echo $q->tag(2), "\n";
"#,
    );
    assert_eq!(
        out,
        "Sq 1\nSq string(1) \"s\"\nCi 1\nCi string(1) \"s\"\nSq 2\n"
    );
}

/// Siblings returning int, string, array and object share one boxed return, also when an
/// inherited method returns the dispatched call.
#[test]
fn test_abstract_siblings_with_different_return_types() {
    let out = compile_and_run(
        r#"<?php
class P { public function __construct(public int $n) {} }
abstract class A { abstract public function f($v); public function run($v) { return $this->f($v); } }
class I extends A { public function f($v) { return $v + 1; } }
class S extends A { public function f($v) { return "s$v"; } }
class R extends A { public function f($v) { return [$v, $v]; } }
class O extends A { public function f($v) { return new P($v); } }
foreach ([new I(), new S(), new R(), new O()] as $a) { echo json_encode($a->f(1)), " ", json_encode($a->run(2)), "\n"; }
"#,
    );
    assert_eq!(out, "2 3\n\"s1\" \"s2\"\n[1,1] [2,2]\n{\"n\":1} {\"n\":2}\n");
}

/// A declared `int` slot parameter reaches an untyped override that receives it boxed.
#[test]
fn test_abstract_typed_param_reaches_untyped_override() {
    let out = compile_and_run(
        r#"<?php
abstract class A { abstract public function f(int $v); }
class B extends A { public function f($v) { return $v * 2; } }
function go(A $a) { var_dump($a->f(3)); }
go(new B());
"#,
    );
    assert_eq!(out, "int(6)\n");
}

/// A declared `mixed` abstract return keeps its contract when the overrides narrow it.
#[test]
fn test_abstract_declared_mixed_with_narrower_overrides() {
    let out = compile_and_run(
        r#"<?php
abstract class A { abstract public function f(): mixed; }
class B extends A { public function f(): string { return "b"; } }
class C extends A { public function f(): int { return 7; } }
class D extends A { public function f(): array { return [1]; } }
function go(A $a) { var_dump($a->f()); }
go(new B()); go(new C()); go(new D());
"#,
    );
    assert_eq!(
        out,
        "string(1) \"b\"\nint(7)\narray(1) {\n  [0]=>\n  int(1)\n}\n"
    );
}

/// A concrete parent overridden with another return type dispatches the child's value.
#[test]
fn test_concrete_parent_override_with_different_return_type() {
    let out = compile_and_run(
        r#"<?php
class A { public function f() { return 1; } public function twice() { return [$this->f(), $this->f()]; } }
class B extends A { public function f() { return "s"; } }
class C extends A { public function f() { return null; } }
function go(A $a) { var_dump($a->f()); echo json_encode($a->twice()), "\n"; }
go(new A()); go(new B()); go(new C());
"#,
    );
    assert_eq!(out, "int(1)\n[1,1]\nstring(1) \"s\"\n[\"s\",\"s\"]\nNULL\n[null,null]\n");
}

/// Late static binding through an abstract static method returns each child's value.
#[test]
fn test_abstract_static_method_family_returns() {
    let out = compile_and_run(
        r#"<?php
abstract class A { abstract public static function make($v); public static function go($v) { return static::make($v); } }
class B extends A { public static function make($v) { return "b$v"; } }
class C extends A { public static function make($v) { return $v * 2; } }
var_dump(B::go(1)); var_dump(C::go(4)); var_dump(C::make(5));
"#,
    );
    assert_eq!(out, "string(2) \"b1\"\nint(8)\nint(10)\n");
}

/// An interface between the receiver type and an abstract class keeps both dispatch paths aligned.
#[test]
fn test_interface_and_abstract_untyped_family() {
    let out = compile_and_run(
        r#"<?php
interface I { public function f($v); }
abstract class A implements I { abstract public function f($v); }
class B extends A { public function f($v) { return $v . "b"; } }
class C extends A { public function f($v) { return $v + 1; } }
function viaI(I $i) { var_dump($i->f(1)); }
function viaA(A $a) { var_dump($a->f(2)); }
foreach ([new B(), new C()] as $o) { viaI($o); viaA($o); }
"#,
    );
    assert_eq!(out, "string(2) \"1b\"\nstring(2) \"2b\"\nint(2)\nint(3)\n");
}

/// Regression: a fully typed family whose overrides already agree compiles and runs unchanged.
#[test]
fn test_typed_float_family_still_dispatches_unchanged() {
    let out = compile_and_run(
        r#"<?php
abstract class Shape {
    abstract public function area(): float;
    public function describe(): string { return static::class . "=" . $this->area(); }
}
class Sq extends Shape { public function __construct(private float $s) {} public function area(): float { return $this->s * $this->s; } }
class Ci extends Shape { public function __construct(private float $r) {} public function area(): float { return 3 * $this->r; } }
foreach ([new Sq(2), new Ci(1)] as $s) { echo $s->describe(), "\n"; var_dump($s->area()); }
"#,
    );
    assert_eq!(out, "Sq=4\nfloat(4)\nCi=3\nfloat(3)\n");
}

/// A declared `float` return boxed by its family still coerces the int it returns.
#[test]
fn test_family_boxed_declared_float_return_still_coerces() {
    let out = compile_and_run(
        r#"<?php
abstract class A { abstract public function f(): mixed; }
class B extends A { public function f(): string { return "b"; } }
class C extends A { public function f(): float { return 5; } }
function go(A $a) { var_dump($a->f()); }
go(new B()); go(new C()); var_dump((new C())->f());
"#,
    );
    assert_eq!(out, "string(1) \"b\"\nfloat(5)\nfloat(5)\n");
}

/// A `static` override boxed into its parent's `static|false` call ABI still chains on the object.
#[test]
fn test_late_static_override_boxed_by_family_abi() {
    let out = compile_and_run(
        r#"<?php
class M { public function dup(): static|false { return false; } }
final class A extends M {
    public function dup(): static { return $this; }
    public function label(): string { return "clone"; }
}
function go(M $m) { var_dump($m->dup() === false); }
go(new M()); go(new A());
echo (new A())->dup()->label(), "\n";
"#,
    );
    assert_eq!(out, "bool(true)\nbool(false)\nclone\n");
}

/// A declared `string` parameter boxed by its family still receives the coerced string, while
/// an untyped sibling receives the int unchanged.
#[test]
fn test_family_boxed_declared_string_param_coerces_on_entry() {
    let out = compile_and_run(
        r#"<?php
class A { public function f(string $v) { var_dump($v); return strlen($v); } }
class B extends A { public function f($v) { var_dump($v); return "b"; } }
function go(A $a) { var_dump($a->f(12345)); }
go(new A); go(new B);
"#,
    );
    assert_eq!(out, "string(5) \"12345\"\nint(5)\nint(12345)\nstring(1) \"b\"\n");
}

/// Declared `int` and `bool` parameters boxed by their family coerce like a typed call.
#[test]
fn test_family_boxed_int_and_bool_params_coerce_on_entry() {
    let out = compile_and_run(
        r#"<?php
abstract class A { abstract public function f(int $v); }
class B extends A { public function f(int $v) { var_dump($v); return $v + 1; } }
class C extends A { public function f($v) { var_dump($v); return "c"; } }
class P { public function g(bool $v) { var_dump($v); return 1; } }
class Q extends P { public function g($v) { var_dump($v); return "q"; } }
function go(A $a) { var_dump($a->f("7")); var_dump($a->f(true)); }
function gp(P $p) { var_dump($p->g(2)); }
go(new B); go(new C); gp(new P); gp(new Q);
"#,
    );
    assert_eq!(
        out,
        "int(7)\nint(8)\nint(1)\nint(2)\nstring(1) \"7\"\nstring(1) \"c\"\nbool(true)\nstring(1) \"c\"\n\
         bool(true)\nint(1)\nint(2)\nstring(1) \"q\"\n"
    );
}

/// A boxed declared `int` parameter rejects a non-numeric string with PHP's TypeError.
#[test]
fn test_family_boxed_int_param_throws_type_error() {
    let out = compile_and_run(
        r#"<?php
abstract class A { abstract public function f(int $v); }
class B extends A { public function f(int $v) { return $v + 1; } }
class C extends A { public function f($v) { return "c"; } }
function go(A $a, $x) { try { var_dump($a->f($x)); } catch (TypeError $e) { echo substr($e->getMessage(), 0, 52), "\n"; } }
foreach (["5", "abc", 2.0, null] as $x) { go(new B, $x); }
go(new C, "abc");
"#,
    );
    assert_eq!(
        out,
        "int(6)\nB::f(): Argument #1 ($v) must be of type int, string\nint(3)\n\
         B::f(): Argument #1 ($v) must be of type int, null g\nstring(1) \"c\"\n"
    );
}

/// Declared scalar returns boxed by their family coerce first, through the parent and directly.
#[test]
fn test_family_boxed_declared_scalar_returns_coerce() {
    let out = compile_and_run(
        r#"<?php
abstract class A { abstract public function f($v); }
class B extends A { public function f($v): string { return $v * 2; } }
class C extends A { public function f($v) { return [$v]; } }
class D extends A { public function f($v): bool { return $v; } }
class E extends A { public function f($v): int { return $v > 1; } }
function go(A $a) { var_dump($a->f(3)); }
go(new B); go(new C); go(new D); go(new E);
$b = new B; var_dump($b->f(5));
"#,
    );
    assert_eq!(
        out,
        "string(1) \"6\"\narray(1) {\n  [0]=>\n  int(3)\n}\nbool(true)\nint(1)\nstring(2) \"10\"\n"
    );
}

/// Regression: a caller passing an object-returning family call to a typed parameter compiles,
/// as it did before override families shared a call ABI.
#[test]
fn test_object_return_family_keeps_caller_type() {
    let out = compile_and_run(
        r#"<?php
class A { public function obj() { return new A; } public function n() { return "A"; } }
class B extends A { public function obj() { return null; } }
function needA(A $a) { return $a->n(); }
echo needA((new A)->obj()), "\n";
$x = (new B)->obj(); var_dump($x);
"#,
    );
    assert_eq!(out, "A\nNULL\n");
}

/// A free function returning a family call is inferred from the shared return, not from the
/// root's own body (it is checked before any method body).
#[test]
fn test_free_function_returning_family_call() {
    let out = compile_and_run(
        r#"<?php
class A { public function m($v) { return $v + 0; } }
class B extends A { public function m($v) { return "s$v"; } }
function go(A $c) { return $c->m(1); }
echo go(new B), " ", go(new A), "\n";
"#,
    );
    assert_eq!(out, "s1 1\n");
}
