//! Purpose:
//! End-to-end coverage of PHP-compatible override and interface-implementation signatures
//! (LSP): variadic renames, union covariance,
//! `#[\ReturnTypeWillChange]` and the implicit `__toString(): string`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every fixture dispatches through the parent or interface type, so an ABI mismatch between
//!   the parent contract and the override shows up as wrong output instead of passing silently.
//! - Expected output is PHP 8.5's.

use super::*;

/// An override may rename the variadic (`...$arrays` as `...$lists`); arguments still collect positionally through the parent.
#[test]
fn test_override_renames_variadic_parameter() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public function merge(array ...$arrays): int { return count($arrays); }
}
class Child extends Base {
    public function merge(array ...$lists): int { return 100 + count($lists); }
}
function viaBase(Base $b): int { return $b->merge([1], [2], [3]); }
echo viaBase(new Base()), "\n";
echo viaBase(new Child()), "\n";
"#,
    );
    assert_eq!(out, "3\n103\n");
}

/// A union return narrowed member by member with the class under construction (`Node|bool` as `Leaf|false`) dispatches through the parent.
#[test]
fn test_override_union_return_with_subclass_member() {
    let out = compile_and_run(
        r#"<?php
class Node {
    public function __construct(public int $v) {}
    public function find(int $x): Node|bool { return $x > 0 ? new Node($x) : false; }
}
class Leaf extends Node {
    public function find(int $x): Leaf|false { return $x > 0 ? new Leaf($x * 10) : false; }
}
function viaBase(Node $n, int $x): string {
    $r = $n->find($x);
    return $r === false ? "false" : get_class($r) . ":" . $r->v;
}
echo viaBase(new Node(1), 2), "\n";
echo viaBase(new Leaf(1), 2), "\n";
echo viaBase(new Leaf(1), 0), "\n";
"#,
    );
    assert_eq!(out, "Node:2\nLeaf:20\nfalse\n");
}

/// A static implementation may narrow an interface `static|false` return to `static`.
#[test]
fn test_interface_static_union_return_narrowed_to_static() {
    let out = compile_and_run(
        r#"<?php
interface Maker { public static function make(string $s): static|false; }
class Thing implements Maker {
    public function __construct(public string $s) {}
    public static function make(string $s): static { return new static($s); }
}
class Other extends Thing {}
$t = Thing::make("hi");
echo get_class($t), ":", $t->s, "\n";
$o = Other::make("yo");
echo get_class($o), ":", $o->s, "\n";
"#,
    );
    assert_eq!(out, "Thing:hi\nOther:yo\n");
}

/// `#[\ReturnTypeWillChange]` lets an untyped method implement a builtin interface method with a tentative return type.
#[test]
fn test_return_type_will_change_untyped_builtin_interface_method() {
    let out = compile_and_run(
        r#"<?php
class Box implements IteratorAggregate, Countable {
    private array $items = [1, 2, 3];
    #[\ReturnTypeWillChange]
    public function count() { return count($this->items); }
    public function getIterator(): Iterator { return new ArrayIterator($this->items); }
}
$b = new Box();
echo count($b), "\n";
echo $b->count(), "\n";
foreach ($b as $i) { echo $i; }
echo "\n";
"#,
    );
    assert_eq!(out, "3\n3\n123\n");
}

/// An untyped `__toString()` gets PHP 8's implicit `: string` under `Stringable` and a builtin parent, including an inheriting subclass that does not override it.
#[test]
fn test_untyped_tostring_gets_implicit_string_return() {
    let out = compile_and_run(
        r#"<?php
class Name implements Stringable {
    public function __construct(private string $n) {}
    public function __toString() { return "name:" . $this->n; }
}
class Nick extends Name {}
class Oops extends Exception {
    public function __toString() { return "oops"; }
}
function show(Stringable $s): string { return $s->__toString(); }
echo new Name("x"), " ", new Nick("y"), " ", new Oops(), "\n";
echo show(new Name("z")), " ", show(new Nick("w")), "\n";
var_dump(new Nick("q") instanceof Stringable);
"#,
    );
    assert_eq!(out, "name:x name:y oops\nname:z name:w\nbool(true)\n");
}

/// Regression guard: exact override signatures (same optional layout, covariant object return) keep compiling and dispatching.
#[test]
fn test_exact_override_signatures_still_dispatch() {
    let out = compile_and_run(
        r#"<?php
abstract class Animal {
    abstract public function speak(string $how = "loud"): string;
    public function intro(): Animal { return $this; }
}
class Dog extends Animal {
    public function speak(string $how = "loud"): string { return "woof " . $how; }
    public function intro(): Dog { return $this; }
}
function talk(Animal $a): string { return $a->speak() . "/" . $a->speak("soft"); }
echo talk(new Dog()), "\n";
echo get_class((new Dog())->intro()), "\n";
"#,
    );
    assert_eq!(out, "woof loud/woof soft\nDog\n");
}

/// `#[\ReturnTypeWillChange]` lets a static override of a builtin class method return `?static` while keeping the boxed return ABI.
#[test]
fn test_return_type_will_change_static_builtin_override() {
    let out = compile_and_run(
        r#"<?php
class MyDate extends DateTime {
    #[\ReturnTypeWillChange]
    public static function createFromFormat(string $format, string $datetime, ?DateTimeZone $timezone = null): ?static
    {
        $d = new MyDate("2020-01-02 03:04:05");
        return $datetime === "" ? null : $d;
    }
}
$d = MyDate::createFromFormat("Y-m-d", "2020-01-02");
echo get_class($d), " ", $d->format("Y-m-d"), "\n";
var_dump(MyDate::createFromFormat("Y-m-d", ""));
"#,
    );
    assert_eq!(out, "MyDate 2020-01-02\nNULL\n");
}

/// A renamed variadic with the same element type, called positionally, by name and with spreads.
#[test]
fn test_override_renamed_variadic_with_named_and_spread_args() {
    let out = compile_and_run(
        r#"<?php
class Base { public function m(int $k, array ...$arrays): string { return "B" . $k . ":" . json_encode($arrays); } }
class Child extends Base { public function m(int $k, array ...$lists): string { return "C" . $k . ":" . json_encode($lists); } }
function viaBase(Base $b): string { return $b->m(1, [1], [2]); }
echo viaBase(new Base()), " ", viaBase(new Child()), "\n";
$c = new Child();
echo $c->m(2, lists: [3]), "\n";
echo $c->m(k: 3), "\n";
function viaBaseNamed(Base $b): string { return $b->m(4, arrays: [5]); }
echo viaBaseNamed(new Child()), "\n";
function viaBaseSpread(Base $b): string { return $b->m(5, ...[[6], [7]]); }
echo viaBaseSpread(new Child()), "\n";
"#,
    );
    assert_eq!(out, "B1:[[1],[2]] C1:[[1],[2]]\nC2:{\"lists\":[3]}\nC3:[]\nC4:{\"arrays\":[5]}\nC5:[[6],[7]]\n");
}

/// `= self::X` on parent and override naming the same parent constant is the same default (the `Application::handle(..., $type = self::MAIN_REQUEST)` shape).
#[test]
fn test_override_keeps_inherited_self_constant_default() {
    let out = compile_and_run(
        r#"<?php
abstract class P { const X = 'p'; abstract public function f(string $v = self::X); }
class C extends P { public function f(string $v = self::X) { return "c" . $v; } }
function via(P $b) { echo $b->f(), "\n"; }
via(new C());
echo (new C)->f(), "\n";
"#,
    );
    assert_eq!(out, "cp\ncp\n");
}

/// `= self::MAIN` resolving to the interface constant from both sides.
#[test]
fn test_interface_impl_keeps_interface_self_constant_default() {
    let out = compile_and_run(
        r#"<?php
interface HK { const MAIN = 1; public function handle(int $type = self::MAIN, bool $c = true); }
class App implements HK { public function handle(int $type = self::MAIN, bool $c = true) { return $type; } }
function via(HK $h) { echo $h->handle(), "\n"; }
via(new App()); echo (new App)->handle(), "\n";
"#,
    );
    assert_eq!(out, "1\n1\n");
}

/// Union covariance that keeps the boxed representation still dispatches through the parent.
#[test]
fn test_override_union_return_keeping_boxed_representation() {
    let out = compile_and_run(
        r#"<?php
class Node { public $n = "N"; public function find(): Node|string { return "s"; } }
class Twig extends Node { public $n = "T"; public function find(): Twig|string { return $this; } }
class Leaf extends Node { public $n = "L"; public function find(): Leaf|string { return "q"; } }
function via(Node $x) { $r = $x->find(); var_dump(is_object($r) ? $r->n : $r); }
via(new Node()); via(new Twig()); via(new Leaf());
"#,
    );
    assert_eq!(out, "string(1) \"s\"\nstring(1) \"T\"\nstring(1) \"q\"\n");
}

/// Parent `self::X` and override `Other::X` resolve to the same value.
#[test]
fn test_override_default_named_constant_matches_parent_self_constant() {
    let out = compile_and_run(
        r#"<?php
class Other { const X = 1; }
class P { const X = 1; public function f(int $v = self::X) { return "p" . $v; } }
class C extends P { public function f(int $v = Other::X) { return "c" . $v; } }
function via(P $p) { echo $p->f(), "\n"; }
via(new P()); via(new C()); echo (new C)->f(), "\n";
"#,
    );
    assert_eq!(out, "p1\nc1\nc1\n");
}

/// Parent `parent::X` and override `self::X` both name the grandparent constant.
#[test]
fn test_override_default_parent_constant_from_grandparent() {
    let out = compile_and_run(
        r#"<?php
class G { const X = 'g'; }
class P extends G { public function f(string $v = parent::X) { return "p" . $v; } }
class C extends P { public function f(string $v = self::X) { return "c" . $v; } }
function via(P $p) { echo $p->f(), "\n"; }
via(new P()); via(new C()); echo (new C)->f(), "\n";
"#,
    );
    assert_eq!(out, "pg\ncg\ncg\n");
}

/// `self::X` defined as `self::A * 3` resolves and folds on both sides.
#[test]
fn test_override_default_constant_expression_folds() {
    let out = compile_and_run(
        r#"<?php
class P { const A = 2; const X = self::A * 3; public function f(int $v = self::X) { return "p" . $v; } }
class C extends P { public function f(int $v = self::X) { return "c" . $v; } }
function via(P $p) { echo $p->f(), "\n"; }
via(new P()); via(new C()); echo (new C)->f(), "\n";
"#,
    );
    assert_eq!(out, "p6\nc6\nc6\n");
}

/// `X = self::A` is written in the parent, so a child redefining `A` keeps the same default.
#[test]
fn test_override_default_constant_chain_with_redefined_operand() {
    let out = compile_and_run(
        r#"<?php
class P { const A = 2; const X = self::A; public function f(int $v = self::X) { return "p" . $v; } }
class C extends P { const A = 5; public function f(int $v = self::X) { return "c" . $v; } }
function via(P $p) { echo $p->f(), "\n"; }
via(new P()); via(new C()); echo (new C)->f(), "\n";
"#,
    );
    assert_eq!(out, "p2\nc2\nc2\n");
}

/// The Laravel kernel shape one level down (`MyApp extends App implements HK`, `handle($type = self::MAIN)`).
#[test]
fn test_grandchild_override_default_interface_constant() {
    let out = compile_and_run(
        r#"<?php
interface HK { const MAIN = 1; public function handle(int $type = self::MAIN, bool $c = true); }
class App implements HK { public function handle(int $type = self::MAIN, bool $c = true) { return "a$type"; } }
class MyApp extends App { public function handle(int $type = self::MAIN, bool $c = true) { return "m$type"; } }
function via(HK $h) { echo $h->handle(), "\n"; }
function viaA(App $h) { echo $h->handle(), "\n"; }
via(new App()); via(new MyApp()); viaA(new MyApp()); echo (new MyApp)->handle(), "\n";
"#,
    );
    assert_eq!(out, "a1\nm1\nm1\nm1\n");
}

/// An untyped override of `int $x` is boxed by the family and dispatches through the parent.
#[test]
fn test_family_override_untyped_param_of_declared_int() {
    let out = compile_and_run(
        r#"<?php
class Base { public function f(int $x): string { return "B$x"; } }
class Child extends Base { public function f($x): string { return "C$x"; } }
function viaBase(Base $b): string { return $b->f(7); }
echo viaBase(new Base()), viaBase(new Child()), "\n";
"#,
    );
    assert_eq!(out, "B7C7\n");
}

/// `Foo $x` widened to `?Foo $x` is boxed by the family.
#[test]
fn test_family_override_nullable_object_param() {
    let out = compile_and_run(
        r#"<?php
class Foo { public $v = "foo"; }
class Base { public function f(Foo $x): string { return "B" . $x->v; } }
class Child extends Base { public function f(?Foo $x): string { return "C" . ($x === null ? "null" : $x->v); } }
function viaBase(Base $b): string { return $b->f(new Foo()); }
echo viaBase(new Base()), viaBase(new Child()), (new Child())->f(null), "\n";
"#,
    );
    assert_eq!(out, "BfooCfooCnull\n");
}

/// `int|null` and `int|Twig` under `Node|int|null` are boxed into the family return.
#[test]
fn test_family_override_union_return_narrowed_to_tagged_scalar() {
    let out = compile_and_run(
        r#"<?php
class Node { public $n = "N"; public function find(): Node|int|null { return 4; } }
class Leaf extends Node { public $n = "L"; public function find(): int|null { return 9; } }
class Twig extends Node { public function find(): int|Twig { return 5; } }
function via(Node $x) { $r = $x->find(); var_dump(is_object($r) ? $r->n : $r); }
via(new Node()); via(new Leaf()); via(new Twig());
"#,
    );
    assert_eq!(out, "int(4)\nint(9)\nint(5)\n");
}

/// `false` under `Node|false` keeps `bool(false)` through the family return.
#[test]
fn test_family_override_union_return_narrowed_to_false() {
    let out = compile_and_run(
        r#"<?php
class Node { public $n = "N"; public function find(): Node|false { return false; } }
class Twig extends Node { public $n = "T"; public function find(): Twig|false { return $this; } }
class Leaf extends Node { public $n = "L"; public function find(): false { return false; } }
function via(Node $x) { $r = $x->find(); var_dump(is_object($r) ? $r->n : $r); }
via(new Node()); via(new Twig()); via(new Leaf());
"#,
    );
    assert_eq!(out, "bool(false)\nstring(1) \"T\"\nbool(false)\n");
}

/// `?N` narrowed to `N` inside an override family; the object returned through
/// the parent must be boxed by the family return so `$r->n` reads the property.
#[test]
fn test_family_override_object_return_narrowed_from_nullable() {
    let out = compile_and_run(
        r#"<?php
class N { public $n = "N"; }
class P { public function f(): ?N { return null; } }
class C extends P { public function f(): N { return new N(); } }
function via(P $p) { $r = $p->f(); var_dump($r === null ? null : $r->n); }
via(new P()); via(new C());
"#,
    );
    assert_eq!(out, "NULL\nstring(1) \"N\"\n");
}

/// `static|false` narrowed to `static`; the call through the
/// subclass resolves `static` to the receiver, so a property only the subclass declares reads.
#[test]
fn test_family_static_return_narrowed_reads_subclass_property() {
    let out = compile_and_run(
        r#"<?php
class M { public function f(): static|false { return false; } }
class C extends M { public $extra = "X"; public function f(): static { return $this; } }
class D extends C { public $more = "Y"; }
echo (new C())->f()->extra, "\n";
$c = new C(); $r = $c->f(); echo $r->extra, "\n";
$d = new D(); echo $d->f()->more, $d->f()->extra, "\n";
function via(M $m) { $r = $m->f(); echo $r === false ? "false" : get_class($r), "\n"; }
via(new M()); via(new C()); via(new D());
"#,
    );
    assert_eq!(out, "X\nX\nYX\nfalse\nC\nD\n");
}

/// `?static` -> `static|null` -> `static` over three levels keeps each
/// receiver's own class, so the leaf's own property reads.
#[test]
fn test_family_nullable_static_three_levels_reads_leaf_property() {
    let out = compile_and_run(
        r#"<?php
class A { public $n = "A"; public function f(): ?static { return null; } }
class B extends A { public $n = "B"; public $b = "b"; public function f(): static|null { return $this; } }
class C extends B { public $n = "C"; public $only = "only"; public function f(): static { return $this; } }
function via(A $a) { $r = $a->f(); echo $r === null ? "null" : get_class($r) . $r->n, "\n"; }
via(new A()); via(new B()); via(new C());
echo (new C())->f()->only, (new C())->f()->f()->only, (new B())->f()->b, "\n";
function viaB(B $b) { $r = $b->f(); return $r?->n; }
echo viaB(new B()), viaB(new C()), "\n";
"#,
    );
    assert_eq!(out, "null\nBB\nCC\nonlyonlyb\nBC\n");
}

/// A private ancestor method is not part of the family; the family is rooted
/// at the first public declaration, so the untyped override is boxed and dispatches.
#[test]
fn test_family_rooted_below_private_ancestor() {
    let out = compile_and_run(
        r#"<?php
class A { private function f(int $x) { return "A"; } }
class B extends A { public function f(int $x) { return "B" . gettype($x); } }
class C extends B { public function f($x) { return "C" . gettype($x); } }
function via(B $b) { return $b->f(4); }
echo via(new B()), via(new C()), "\n";
"#,
    );
    assert_eq!(out, "BintegerCinteger\n");
}

/// A subclass implements an interface declaring a raw `int` return,
/// but its family returns the boxed `int|string`; the interface call would read the box as an
/// int, so it is refused at compile time.
#[test]
#[should_panic(expected = "cannot share one virtual call ABI: class C implements interface I whose h() returns int")]
fn test_family_boxed_return_conflicts_with_interface_scalar_return() {
    compile_and_run(
        r#"<?php
interface I { public function h(): int; }
class P { public function h(): int|string { return "p"; } }
class C extends P implements I { public function h(): int { return 41; } }
class D extends C { public function h(): int { return 9; } }
function viaI(I $i) { return $i->h() * 2; }
echo viaI(new C()), " ", viaI(new D()), "\n";
"#,
    );
}

/// The same conflict with an object return narrowed from `?N`.
#[test]
#[should_panic(expected = "cannot share one virtual call ABI: class C implements interface I whose g() returns N")]
fn test_family_boxed_return_conflicts_with_interface_object_return() {
    compile_and_run(
        r#"<?php
class N { public $n = "N"; }
interface I { public function g(): N; }
abstract class A { abstract public function g(): ?N; }
class C extends A implements I { public function g(): N { return new N(); } }
class D extends A { public function g(): ?N { return null; } }
function viaI(I $i) { return $i->g()->n; }
function viaA(A $a) { $r = $a->g(); return $r === null ? "null" : $r->n; }
echo viaI(new C()), viaA(new C()), viaA(new D()), "\n";
"#,
    );
}

/// Non-regression: an interface whose return matches the family's
/// boxed return keeps dispatching.
#[test]
fn test_family_boxed_return_matching_interface_return() {
    let out = compile_and_run(
        r#"<?php
class N { public $n = "N"; }
interface I { public function g(): ?N; }
class P implements I { public function g(): ?N { return null; } }
class C extends P { public function g(): N { return new N(); } }
function viaI(I $i) { $r = $i->g(); return $r === null ? "null" : $r->n; }
echo viaI(new P()), viaI(new C()), "\n";
"#,
    );
    assert_eq!(out, "nullN\n");
}
