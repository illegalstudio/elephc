//! Purpose:
//! End-to-end codegen tests for overrides and interface implementations that append trailing
//! optional parameters to the method they replace, called through the narrow and the wide type.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - A call through the narrower ancestor or interface passes fewer arguments than the override
//!   declares; the retired vtable slot's (or interface entry's) EIR adapter supplies the defaults.
//! - `func_num_args()` must count the arguments the caller passed, not the override's arity.
//! - Every fixture is plain PHP, so `ELEPHC_PHP_CHECK=1` compares it against the interpreter.

use super::*;

/// An abstract method and a concrete parent method both gain optional parameters in the child;
/// calls through the abstract parent, through `$this` inside the parent and through the child
/// all reach the child's body, with the defaults applied only when the caller omitted them.
#[test]
fn test_override_appends_optional_parameters_through_parent_type() {
    let out = compile_and_run(
        r#"<?php
abstract class A {
    abstract public function f(): string;
    public function g(): string { return "g"; }
    public function run(): string { return $this->f() . $this->g(); }
}
class B extends A {
    public function f(bool $x = true): string { return $x ? "t" : "f"; }
    public function g(?string $e = null): string { return "G" . ($e ?? ""); }
}
function viaA(A $a): string { return $a->f() . "," . $a->g() . "," . $a->run(); }
function viaB(B $b): string { return $b->f(false) . "," . $b->g("e") . "," . $b->f(); }
echo viaA(new B()), "|", viaB(new B()), "\n";
"#,
    );
    assert_eq!(out, "t,G,tG|f,Ge,t\n");
}

/// Two successive widenings retire two slots; an inheriting grandchild keeps the widest body,
/// named arguments resolve against the static receiver's signature, and defaults may be
/// constants and constant expressions. Calls through the narrowest type get the runtime
/// class's own defaults for every parameter.
#[test]
fn test_override_widening_chains_named_arguments_and_expression_defaults() {
    let out = compile_and_run(
        r#"<?php
const SUFFIX = "!";
class A { public function g(): string { return "A"; } }
class B extends A {
    public function g(?string $e = null, string $s = SUFFIX . "?"): string { return "B" . ($e ?? "-") . $s; }
}
class C extends B {
    public function g(?string $e = "c", string $s = SUFFIX . "?", int $n = 2 * 21): string { return "C" . ($e ?? "-") . $s . $n; }
}
class D extends C {}
function viaA(A $a): string { return $a->g(); }
function viaB(B $b): string { return $b->g("e", "s") . "," . $b->g(null, s: "named"); }
foreach ([new A(), new B(), new C(), new D()] as $o) {
    echo get_class($o), ":", viaA($o), ($o instanceof B ? "," . viaB($o) : ""), "\n";
}
$c = new C();
echo $c->g(n: 7), ",", $c->g("x", "y", 1), "\n";
"#,
    );
    assert_eq!(
        out,
        "A:A\nB:B-!?,Bes,B-named\nC:Cc!?42,Ces42,C-named42\nD:Cc!?42,Ces42,C-named42\nCc!?7,Cxy1\n"
    );
}

/// `func_num_args()` and `func_get_args()` in a widened override count only what the caller
/// passed, both through the narrow parent and through the override's own type.
#[test]
fn test_override_widening_func_num_args_counts_passed_arguments() {
    let out = compile_and_run(
        r#"<?php
class A { public function f(int $a): string { return "A" . func_num_args(); } }
class B extends A {
    public function f(int $a, int $b = 2, int $c = 3): string {
        return "B$a$b$c:" . func_num_args() . json_encode(func_get_args());
    }
}
function viaA(A $x): string { return $x->f(1); }
function viaB(B $x): string { return $x->f(1) . " " . $x->f(1, 5) . " " . $x->f(1, c: 7); }
echo viaA(new A()), " ", viaA(new B()), " ", viaB(new B()), "\n";
"#,
    );
    assert_eq!(out, "A1 B123:1[1] B123:1[1] B153:2[1,5] B127:3[1,2,7]\n");
}

/// A parent whose body uses `func_num_args()` carries a surplus collector; surplus arguments
/// passed through it land in the override's appended parameters, as in PHP.
#[test]
fn test_override_widening_forwards_surplus_arguments_of_a_collecting_parent() {
    let out = compile_and_run(
        r#"<?php
class A { public function f($a) { return "A" . $a . func_num_args(); } }
class B extends A {
    public function f($a, $b = 3) { return "B$a|$b|" . func_num_args() . json_encode(func_get_args()); }
}
function viaA(A $x) { return $x->f(1) . " " . $x->f(1, 9); }
echo viaA(new A()), " ", viaA(new B()), "\n";
"#,
    );
    assert_eq!(out, "A11 A12 B1|3|1[1] B1|9|2[1,9]\n");
}

/// symfony/string's shape: an abstract `static`-returning method overridden with an optional
/// flag and called through the abstract parent type.
#[test]
fn test_override_widening_of_abstract_static_return() {
    let out = compile_and_run(
        r#"<?php
abstract class S {
    public string $v = "S";
    abstract public function folded(): static;
}
class T extends S {
    public function folded(bool $compat = true): static {
        $n = clone $this;
        $n->v .= $compat ? "c" : "n";
        return $n;
    }
}
function fold(S $s): string { return $s->folded()->v; }
echo fold(new T()), " ", (new T())->folded(false)->v, "\n";
"#,
    );
    assert_eq!(out, "Sc Sn\n");
}

/// psr/event-dispatcher against symfony/event-dispatcher-contracts: a child interface
/// redeclares `dispatch()` with an optional name, and the implementation is called through
/// both interfaces, positionally and by name.
#[test]
fn test_interface_redeclaration_widening_dispatches_through_both_interfaces() {
    let out = compile_and_run(
        r#"<?php
interface PsrDispatcher { public function dispatch(object $event); }
interface SfDispatcher extends PsrDispatcher {
    public function dispatch(object $event, ?string $eventName = null): object;
}
final class Ev { public string $n = "ev"; }
final class Dispatcher implements SfDispatcher {
    public function dispatch(object $event, ?string $eventName = null): object {
        echo "[", $eventName ?? "null", "]";
        return $event;
    }
}
function viaPsr(PsrDispatcher $d, object $e) { return $d->dispatch($e); }
function viaSf(SfDispatcher $d, object $e): object { return $d->dispatch($e, "x"); }
function viaSfNamed(SfDispatcher $d, object $e): object { return $d->dispatch(eventName: "nm", event: $e); }
$d = new Dispatcher();
echo viaPsr($d, new Ev())->n, viaSf($d, new Ev())->n, viaSfNamed($d, new Ev())->n, "\n";
"#,
    );
    assert_eq!(out, "[null]ev[x]ev[nm]ev\n");
}

/// psr/container against symfony/dependency-injection: an implementation adds an optional
/// behaviour flag, and a subclass overrides it with a different default, which a call through
/// the interface (which omits the flag) must use.
#[test]
fn test_interface_implementation_widening_with_overridden_default() {
    let out = compile_and_run(
        r#"<?php
interface PsrContainer { public function get(string $id); }
class Container implements PsrContainer {
    public function get(string $id, int $invalidBehavior = 1) { return $id . ":" . $invalidBehavior; }
}
class Sub extends Container {
    public function get(string $id, int $invalidBehavior = 2) { return "sub " . parent::get($id, $invalidBehavior); }
}
function viaPsr(PsrContainer $c): string { return $c->get("svc"); }
function viaContainer(Container $c): string { return $c->get("svc", 3); }
echo viaPsr(new Container()), ",", viaPsr(new Sub()), ",", viaContainer(new Sub()), "\n";
"#,
    );
    assert_eq!(out, "svc:1,sub svc:2,sub svc:3\n");
}

/// A by-reference parameter inherited unchanged keeps writing back through the narrow parent
/// when the override appends an optional by-value parameter.
#[test]
fn test_override_widening_keeps_by_reference_parameters() {
    let out = compile_and_run(
        r#"<?php
class R { public function bump(int &$x): void { $x++; } }
class R2 extends R { public function bump(int &$x, int $by = 10): void { $x += $by; } }
function viaR(R $r, int &$x): void { $r->bump($x); }
$x = 1;
viaR(new R(), $x);
viaR(new R2(), $x);
$y = 1;
(new R2())->bump($y, 5);
echo $x, " ", $y, "\n";
"#,
    );
    assert_eq!(out, "12 6\n");
}

/// A protected method widened by the child is still reached from the parent's own body, and a
/// class that both extends the parent and implements an interface dispatches through each.
#[test]
fn test_override_widening_protected_method_and_interface_family() {
    let out = compile_and_run(
        r#"<?php
interface I { public function g(); }
class A {
    public function g() { return "A"; }
    protected function p(int $x) { return "p$x"; }
    public function callP() { return $this->p(1); }
}
class B extends A implements I {
    public function g($e = "d") { return "B$e"; }
    protected function p(int $x, int $y = 5) { return "q$x$y"; }
}
function viaI(I $i) { return $i->g(); }
function viaA(A $a) { return $a->g() . $a->callP(); }
echo viaA(new A()), " ", viaA(new B()), " ", viaI(new B()), "\n";
"#,
    );
    assert_eq!(out, "Ap1 Bdq15 Bd\n");
}
