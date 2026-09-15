//! Purpose:
//! Integration or regression tests for PHP object cloning codegen.
//! Covers shallow object copies, declared property slots, stdClass dynamic properties, and `__clone` hooks.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures compile to native binaries and compare stdout against PHP clone semantics.

use super::*;

/// Verifies cloning declared scalar/string properties creates an independent object slot copy.
#[test]
fn test_clone_copies_declared_properties_independently() {
    let out = compile_and_run(
        r#"<?php
class Item {
    public int $n = 1;
    public string $label = "one";
}
$a = new Item();
$b = clone $a;
$b->n = 2;
$b->label = "two";
echo $a->n . ":" . $a->label . "|" . $b->n . ":" . $b->label;
"#,
    );
    assert_eq!(out, "1:one|2:two");
}

/// Verifies `__clone()` is invoked after the shallow copy and mutates the clone, not the source.
#[test]
fn test_clone_invokes_magic_clone_on_the_copy() {
    let out = compile_and_run(
        r#"<?php
class Counter {
    public int $n = 1;
    public function __clone(): void {
        echo "hook;";
        $this->n = $this->n + 10;
    }
}
$a = new Counter();
$b = clone $a;
echo $a->n . "|" . $b->n;
"#,
    );
    assert_eq!(out, "hook;1|11");
}

/// Verifies `__clone()` can replace a string property without corrupting the source object.
#[test]
fn test_clone_persists_string_property_before_magic_clone_mutation() {
    let out = compile_and_run(
        r#"<?php
class LabelBox {
    public string $label = "A";
    public function __clone(): void {
        $this->label = $this->label . ":copy";
    }
}
$a = new LabelBox();
$b = clone $a;
echo $a->label . "|" . $b->label;
"#,
    );
    assert_eq!(out, "A|A:copy");
}

/// Verifies object-valued properties are shallow-copied, so nested object mutations remain shared.
#[test]
fn test_clone_keeps_nested_objects_shared() {
    let out = compile_and_run(
        r#"<?php
class Child {
    public int $x = 1;
}
class Boxed {
    public Child $child;
    public function __construct() {
        $this->child = new Child();
    }
}
$a = new Boxed();
$b = clone $a;
$b->child->x = 7;
echo $a->child->x . "|" . $b->child->x;
"#,
    );
    assert_eq!(out, "7|7");
}

/// Verifies stdClass dynamic properties are copied into a separate hash table during cloning.
#[test]
fn test_clone_copies_stdclass_dynamic_properties_independently() {
    let out = compile_and_run(
        r#"<?php
$a = new stdClass();
$a->name = "source";
$b = clone $a;
$b->name = "copy";
$b->extra = "new";
echo $a->name . "|" . $b->name . "|" . (isset($a->extra) ? "Y" : "N");
"#,
    );
    assert_eq!(out, "source|copy|N");
}

// --- PHP 8.5 `clone()` FUNCTION -------------------------------------------------------------
//
// These pin the FIRST AOT slice of the function form. It shares the boxed shallow-copy adapter
// with Magician, validates its argument at run time, and REFUSES the `$withProperties` overrides
// it cannot apply yet rather than dropping them silently.

/// Verifies the function form copies declared property slots into an independent object.
#[test]
fn test_clone_function_copies_declared_properties_independently() {
    let out = compile_and_run(
        r#"<?php
class Item {
    public int $n = 1;
    public string $label = "one";
}
$a = new Item();
$b = clone($a);
$b->n = 2;
$b->label = "two";
echo $a->n . ":" . $a->label . "|" . $b->n . ":" . $b->label;
"#,
    );
    assert_eq!(out, "1:one|2:two");
}

/// Verifies the function form runs the runtime-selected `__clone()` hook on the copy.
#[test]
fn test_clone_function_invokes_magic_clone_on_the_copy() {
    let out = compile_and_run(
        r#"<?php
class Counter {
    public int $n = 1;
    public function __clone(): void {
        echo "hook;";
        $this->n = $this->n + 10;
    }
}
class Plain {
    public int $k = 5;
}
$a = new Counter();
$b = clone($a);
echo $a->n . "|" . $b->n . ";";
$p = new Plain();
$q = clone($p);
$q->k = 9;
echo $p->k . "|" . $q->k;
"#,
    );
    assert_eq!(out, "hook;1|11;5|9");
}

/// Verifies `clone(...)` reaches the same lowering through a runtime callable, with and
/// without an explicit empty override array.
#[test]
fn test_clone_function_works_through_a_runtime_callable() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public int $n = 1;
    public function bump(): void { $this->n = $this->n + 5; }
}
$fn = clone(...);
$a = new Box();
$b = $fn($a);
$b->bump();
echo get_class($b) . ":" . $a->n . "|" . $b->n . ";";
$c = $fn($a, []);
echo get_class($c) . ":" . $c->n;
"#,
    );
    assert_eq!(out, "Box:1|6;Box:1");
}

/// Verifies a non-object argument raises a catchable `TypeError`, whether the backend sees the
/// wrong type statically or only through a runtime-shaped value.
#[test]
fn test_clone_function_rejects_non_object_arguments() {
    let out = compile_and_run(
        r#"<?php
class Box { public int $n = 2; }
function pick(int $k): mixed { if ($k === 0) { return 5; } return new Box(); }
$fn = clone(...);
try {
    $x = $fn(5);
    echo "no throw;";
} catch (TypeError $e) {
    echo $e->getMessage() . ";";
}
try {
    $y = clone(pick(0));
    echo "no throw;";
} catch (TypeError $e) {
    echo $e->getMessage() . ";";
}
echo get_class(clone(pick(1)));
"#,
    );
    assert_eq!(
        out,
        "clone(): Argument #1 ($object) must be of type object, int given;\
clone(): Argument #1 ($object) must be of type object;Box"
    );
}

/// Verifies a runtime-shaped invalid override is rejected before allocating the clone or running
/// its hook. Only the source destructor may run when the surrounding function returns.
#[test]
fn test_clone_function_validates_mixed_overrides_before_copy_and_hook() {
    let out = compile_and_run(
        r#"<?php
class Probe {
    public function __clone(): void { echo "hook;"; }
    public function __destruct() { echo "drop;"; }
}
function badOverrides(): mixed { echo "arg;"; return 7; }
function run(): void {
    $source = new Probe();
    try {
        clone($source, badOverrides());
        echo "no throw;";
    } catch (TypeError $e) {
        echo $e->getMessage() . ";";
    }
    echo "alive;";
}
run();
"#,
    );
    assert_eq!(
        out,
        "arg;clone(): Argument #2 ($withProperties) must be of type array, int given;alive;drop;"
    );
}

/// Verifies every callable route validates a Mixed override before `__clone` can run.
#[test]
fn test_clone_function_validates_mixed_overrides_across_callable_paths() {
    let out = compile_and_run(
        r#"<?php
class CallableProbe {
    public function __clone(): void { echo "hook;"; }
}
function badCallableOverrides(): mixed { echo "arg;"; return 7; }
$source = new CallableProbe();
$fcc = clone(...);
try { $fcc($source, badCallableOverrides()); } catch (TypeError $e) { echo "type;"; }
try { call_user_func("clone", $source, badCallableOverrides()); } catch (TypeError $e) { echo "type;"; }
try { call_user_func_array("clone", [$source, badCallableOverrides()]); } catch (TypeError $e) { echo "type;"; }
$name = "clone";
try { $name($source, badCallableOverrides()); } catch (TypeError $e) { echo "type;"; }
try { $fcc(...[$source, badCallableOverrides()]); } catch (TypeError $e) { echo "type;"; }
"#,
    );
    assert_eq!(out, "arg;type;arg;type;arg;type;arg;type;arg;type;");
}

/// Verifies unary clone accepts runtime Mixed objects, rejects runtime Mixed scalars, and reports
/// an inaccessible hook as a catchable runtime `Error` while preserving declaring-class access.
#[test]
fn test_clone_keyword_supports_mixed_values_and_runtime_hook_visibility() {
    let out = compile_and_run(
        r#"<?php
class OpenBox {
    public int $n = 4;
    public function __clone(): void { echo "open;"; }
}
function choose(bool $object): mixed { return $object ? new OpenBox() : 7; }
$source = choose(true);
$copy = clone $source;
echo get_class($copy) . ":" . $copy->n . ";";
try {
    $bad = choose(false);
    clone $bad;
    echo "no type;";
} catch (TypeError $e) {
    echo $e->getMessage() . ";";
}
class LockedClone {
    private function __clone(): void { echo "private;"; }
    public function copy(): LockedClone { return clone $this; }
}
$locked = new LockedClone();
try {
    clone $locked;
    echo "no visibility;";
} catch (Error $e) {
    echo $e->getMessage() . ";";
}
echo get_class($locked->copy());
"#,
    );
    assert_eq!(
        out,
        "open;OpenBox:4;clone(): Argument #1 ($object) must be of type object;\
Call to private method LockedClone::__clone() from global scope;private;LockedClone"
    );
}

/// Verifies a RUNTIME override array is applied to the clone and leaves the source untouched.
///
/// The array is a plain local, not a literal the backend can read at compile time, which is the
/// shape every non-trivial call site has.
#[test]
fn test_clone_function_applies_a_runtime_override_array() {
    let out = compile_and_run(
        r#"<?php
class Box { public int $n = 1; public string $s = "a"; }
$a = new Box();
$ov = ["n" => 4, "s" => "z"];
$b = clone($a, $ov);
echo $a->n . ":" . $a->s . "|" . $b->n . ":" . $b->s;
"#,
    );
    assert_eq!(out, "1:a|4:z");
}

/// Verifies a spread-built override array reaches the applicator with its runtime entries.
#[test]
fn test_clone_function_applies_a_spread_built_override_array() {
    let out = compile_and_run(
        r#"<?php
class Box { public int $n = 1; public string $s = "a"; }
$rest = ["n" => 3, "s" => "q"];
$b = clone(new Box(), [...$rest]);
echo $b->n . ":" . $b->s;
"#,
    );
    assert_eq!(out, "3:q");
}

/// Verifies a class the shared adapter cannot copy raises a catchable `Error`.
#[test]
fn test_clone_function_reports_uncloneable_objects() {
    let out = compile_and_run(
        r#"<?php
$s = new SplFixedArray(2);
try {
    $c = clone($s);
    echo "no throw";
} catch (Error $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(out, "Trying to clone an uncloneable object");
}

/// Verifies hook visibility follows the CALLER's lexical scope, not the runtime class alone.
#[test]
fn test_clone_function_honors_private_hook_visibility() {
    let out = compile_and_run(
        r#"<?php
class Locked {
    public int $n = 1;
    private function __clone(): void { $this->n = 9; }
    public function copy(): Locked { return clone($this); }
}
$a = new Locked();
echo $a->copy()->n . ";";
try {
    $b = clone($a);
    echo "no throw";
} catch (Error $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "9;Call to private method Locked::__clone() from global scope"
    );
}

/// Verifies an escaped clone FCC uses each invocation site's scope across direct descriptor,
/// opaque callable, CUF and CUFA paths. `array_map()` remains outside this test because its
/// documented object-element limitation rejects the source array before callback invocation.
#[test]
fn test_clone_callable_uses_invocation_site_scope_across_dispatch_paths() {
    let out = compile_and_run(
        r#"<?php
class CloneScopeRoot {
    public function invokeAncestor(callable $cb, object $value) {
        return $cb($value);
    }
}
class CloneScopeDeclaring extends CloneScopeRoot {
    protected function __clone(): void { echo "hook;"; }
    public function makeCallable(): callable { return clone(...); }
    public function invokeDeclaringCuf(callable $cb, object $value) {
        return call_user_func($cb, $value);
    }
    public function invokeRuntimeCallback(callable $cb, object $value): void {
        array_filter([$value], $cb);
    }
}
class CloneScopeChild extends CloneScopeDeclaring {
    public function invokeDescendantCufa(callable $cb, object $value) {
        return call_user_func_array($cb, [$value]);
    }
}
class CloneScopeOther {
    public function invokeUnrelated(callable $cb, object $value) {
        return $cb($value);
    }
}
class ClonePrivateDeclaring {
    private function __clone(): void { echo "private;"; }
    public function makeCallable(): callable { return clone(...); }
    public function invokeDeclaring(callable $cb, object $value) { return $cb($value); }
}
class ClonePrivateChild extends ClonePrivateDeclaring {
    public function invokeChild(callable $cb, object $value) { return $cb($value); }
}
function invokeOpaqueClone(callable $cb, object $value) {
    return $cb($value);
}
function printCloneScopeError(callable $run): void {
    try {
        $run();
        echo "no throw;";
    } catch (Error $e) {
        echo $e->getMessage() . ";";
    }
}
$value = new CloneScopeChild();
$cb = $value->makeCallable();
printCloneScopeError(function() use ($cb, $value) { return $cb($value); });
printCloneScopeError(function() use ($cb, $value) { return invokeOpaqueClone($cb, $value); });
(new CloneScopeRoot())->invokeAncestor($cb, $value);
$value->invokeDeclaringCuf($cb, $value);
$value->invokeDescendantCufa($cb, $value);
$value->invokeRuntimeCallback($cb, $value);
printCloneScopeError(function() use ($cb, $value) {
    return (new CloneScopeOther())->invokeUnrelated($cb, $value);
});
$private = new ClonePrivateDeclaring();
$privateCb = $private->makeCallable();
$private->invokeDeclaring($privateCb, $private);
printCloneScopeError(function() use ($privateCb, $private) { return $privateCb($private); });
$privateChild = new ClonePrivateChild();
printCloneScopeError(function() use ($privateCb, $privateChild) {
    return $privateChild->invokeChild($privateCb, $privateChild);
});
"#,
    );
    assert_eq!(
        out,
        "Call to protected method CloneScopeDeclaring::__clone() from global scope;\
Call to protected method CloneScopeDeclaring::__clone() from global scope;\
hook;hook;hook;hook;\
Call to protected method CloneScopeDeclaring::__clone() from scope CloneScopeOther;\
private;Call to private method ClonePrivateDeclaring::__clone() from global scope;\
Call to private method ClonePrivateDeclaring::__clone() from scope ClonePrivateChild;"
    );
}

/// Verifies the fresh clone stays exception-owned: a throwing hook releases it exactly once,
/// leaving the source object to be destroyed normally at scope exit.
#[test]
fn test_clone_function_releases_the_clone_when_the_hook_throws() {
    let out = compile_and_run(
        r#"<?php
class Boom {
    public int $n = 1;
    public function __clone(): void { throw new RuntimeException("in hook"); }
    public function __destruct() { echo "gone;"; }
}
function run(): void {
    $a = new Boom();
    try {
        $b = clone($a);
        echo "no throw;";
    } catch (RuntimeException $e) {
        echo "caught:" . $e->getMessage() . ";";
    }
    echo "end;";
}
run();
echo "after";
"#,
    );
    assert_eq!(out, "gone;caught:in hook;end;gone;after");
}

/// Verifies the successful path transfers ownership exactly once: two live objects, two
/// destructor runs, with no leak and no double free.
#[test]
fn test_clone_function_transfers_ownership_of_the_copy() {
    let out = compile_and_run(
        r#"<?php
class Tracked {
    public int $n = 1;
    public function __destruct() { echo "gone;"; }
}
function useClone(): void {
    $a = new Tracked();
    $b = clone($a);
    echo "made:" . $b->n . ";";
}
useClone();
echo "after";
"#,
    );
    assert_eq!(out, "made:1;gone;gone;after");
}
/// Verifies a first-class callable carries the INVOCATION SITE's scope into property resolution.
///
/// The same `clone(...)` value reaches a private property from inside the class and is refused
/// from global scope, so the scope cannot have been baked in where the callable was created.
#[test]
fn test_clone_function_resolves_override_scope_through_a_first_class_callable() {
    let out = compile_and_run(
        r#"<?php
class S {
    private int $x = 1;
    public function get(): int { return $this->x; }
    public static function inside(S $o, array $ov) { $f = clone(...); return $f($o, $ov); }
}
$s = new S();
echo S::inside($s, ["x" => 8])->get() . ";";
$escaped = clone(...);
try { $escaped($s, ["x" => 9]); echo "no throw"; } catch (Error $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "8;Cannot access private property S::$x");
}

/// Verifies `call_user_func()` and `call_user_func_array()` transport the same invocation scope.
#[test]
fn test_clone_function_resolves_override_scope_through_call_user_func_variants() {
    let out = compile_and_run(
        r#"<?php
class S {
    private int $x = 1;
    public function get(): int { return $this->x; }
    public static function cuf(S $o, array $ov) { return call_user_func('clone', $o, $ov); }
    public static function cufa(S $o, array $ov) { return call_user_func_array('clone', [$o, $ov]); }
}
$s = new S();
echo S::cuf($s, ["x" => 4])->get() . ";" . S::cufa($s, ["x" => 5])->get() . ";";
try { call_user_func('clone', $s, ["x" => 6]); echo "no throw"; } catch (Error $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "4;5;Cannot access private property S::$x");
}

/// Verifies two same-named private slots stay apart: the SCOPE picks which one an override writes.
///
/// `P` writes the parent's slot on a `C` instance and `C` writes its own, which a resolver keyed
/// on the runtime class alone cannot express. Global scope reaches neither.
#[test]
fn test_clone_function_selects_the_scope_private_slot_on_a_child_clone() {
    let out = compile_and_run(
        r#"<?php
class P {
    private int $x = 1;
    public function show(): int { return $this->x; }
    public static function fromP(C $o, array $ov): void { $r = clone($o, $ov); echo $r->show() . "/" . $r->show2() . ";"; }
}
class C extends P {
    private int $x = 2;
    public function show2(): int { return $this->x; }
    public static function fromC(C $o, array $ov): void { $r = clone($o, $ov); echo $r->show() . "/" . $r->show2() . ";"; }
}
$c = new C();
P::fromP($c, ["x" => 55]);
C::fromC($c, ["x" => 66]);
try { clone($c, ["x" => 77]); echo "no throw"; } catch (Error $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "55/2;1/66;Cannot access private property C::$x");
}

/// Verifies protected access follows php's ancestor-OR-descendant rule in both directions.
#[test]
fn test_clone_function_honors_protected_visibility_in_both_ancestry_directions() {
    let out = compile_and_run(
        r#"<?php
class A {
    protected int $p = 1;
    public function get(): int { return $this->p; }
    public static function fromA(A $o, array $ov): int { $r = clone($o, $ov); return $r->get(); }
}
class B extends A {
    public static function fromB(A $o, array $ov): int { $r = clone($o, $ov); return $r->get(); }
}
echo A::fromA(new B(), ["p" => 3]) . ";" . B::fromB(new A(), ["p" => 4]) . ";";
try { clone(new A(), ["p" => 5]); echo "no throw"; } catch (Error $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "3;4;Cannot access protected property A::$p");
}

/// Verifies PHP 8.4 asymmetric write visibility decides overrides, naming the DECLARING class.
#[test]
fn test_clone_function_honors_asymmetric_set_visibility() {
    let out = compile_and_run(
        r#"<?php
class R {
    public private(set) int $ps = 0;
    public protected(set) int $pr = 0;
    public static function inner(R $o, array $ov): R { return clone($o, $ov); }
}
class RC extends R {
    public static function child(R $o, array $ov): R { return clone($o, $ov); }
}
$r = new R();
echo R::inner($r, ["ps" => 9])->ps . ";";
try { RC::child($r, ["ps" => 7]); echo "no throw;"; } catch (Error $e) { echo $e->getMessage() . ";"; }
echo RC::child($r, ["pr" => 6])->pr . ";";
try { clone($r, ["pr" => 3]); echo "no throw"; } catch (Error $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(
        out,
        "9;Cannot modify private(set) property R::$ps from scope RC;\
6;Cannot modify protected(set) property R::$pr from global scope"
    );
}

/// Verifies `readonly` is REINITIALIZED by an override in scope and refused out of scope.
///
/// `readonly` carries an implicit `protected(set)`, which is why the refusal says so.
#[test]
fn test_clone_function_reinitializes_readonly_properties_only_in_scope() {
    let out = compile_and_run(
        r#"<?php
class R {
    public readonly int $ro;
    public function __construct() { $this->ro = 1; }
    public function inner(array $ov): R { return clone($this, $ov); }
}
$r = new R();
echo $r->inner(["ro" => 9])->ro . ";";
try { clone($r, ["ro" => 5]); echo "no throw"; } catch (Error $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(
        out,
        "9;Cannot modify protected(set) readonly property R::$ro from global scope"
    );
}

/// Verifies overrides go through the ordinary typed-property pipeline: weak coercion, then a
/// `TypeError` for a value no coercion accepts.
#[test]
fn test_clone_function_coerces_and_rejects_typed_override_values() {
    let out = compile_and_run(
        r#"<?php
class T { public int $n = 0; public float $f = 0.0; public string $s = ""; }
$ov = ["n" => "42", "f" => 3, "s" => 7];
$c = clone(new T(), $ov);
echo $c->n . ":" . $c->f . ":" . $c->s . ";";
$bad = ["n" => "abc"];
try { clone(new T(), $bad); echo "no throw"; } catch (TypeError $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "42:3:7;Cannot assign string to property T::$n of type int");
}

/// Verifies an override on a hooked property runs the `set` hook rather than the backing slot.
#[test]
fn test_clone_function_runs_set_hooks_for_overrides() {
    let out = compile_and_run(
        r#"<?php
class H { public int $n = 0 { set(int $v) { echo "hook(" . $v . ");"; $this->n = $v * 2; } } }
$ov = ["n" => 5];
$c = clone(new H(), $ov);
echo $c->n;
"#,
    );
    assert_eq!(out, "hook(5);10");
}

/// Verifies a `set` hook that throws during an override leaves no clone behind.
#[test]
fn test_clone_function_releases_the_clone_when_a_set_hook_throws() {
    let out = compile_and_run(
        r#"<?php
class HB {
    public int $n = 0 { set(int $v) { if ($v > 5) { throw new RuntimeException("too big"); } $this->n = $v; } }
    public function __destruct() { echo "gone;"; }
}
function run(): void {
    $a = new HB();
    $ov = ["n" => 9];
    try { clone($a, $ov); echo "no throw;"; } catch (RuntimeException $e) { echo "caught:" . $e->getMessage() . ";"; }
}
run();
echo "after";
"#,
    );
    assert_eq!(out, "gone;caught:too big;gone;after");
}

/// Verifies a `clone()` override nested inside a `set` hook that itself runs under another
/// `clone()` override keeps both applications straight.
#[test]
fn test_clone_function_supports_overrides_nested_inside_a_set_hook() {
    let out = compile_and_run(
        r#"<?php
class Inner { public int $v = 0 { set(int $x) { echo "inner(" . $x . ");"; $this->v = $x; } } }
class Outer {
    public int $n = 0 {
        set(int $x) {
            echo "outer(" . $x . ");";
            $c = clone(new Inner(), ["v" => $x + 1]);
            echo "got(" . $c->v . ");";
            $this->n = $x;
        }
    }
}
$ov = ["n" => 4];
$o = clone(new Outer(), $ov);
echo $o->n;
"#,
    );
    assert_eq!(out, "outer(4);inner(5);got(5);4");
}

/// Verifies an inaccessible name and an unknown name both reach `__set()`, as php does.
#[test]
fn test_clone_function_routes_inaccessible_and_unknown_names_to_magic_set() {
    let out = compile_and_run(
        r#"<?php
class M {
    private int $p = 1;
    public function __set($k, $v) { echo "set(" . $k . "=" . $v . ");"; }
}
$ov = ["p" => 5, "zz" => 6];
clone(new M(), $ov);
echo "done";
"#,
    );
    assert_eq!(out, "set(p=5);set(zz=6);done");
}

/// Verifies integer keys become the decimal property names php uses and a NUL-prefixed name throws.
#[test]
fn test_clone_function_converts_numeric_keys_and_rejects_nul_names() {
    let out = compile_and_run(
        r#"<?php
$ov = [0 => "zero", 7 => "seven"];
$c = clone(new stdClass(), $ov);
echo $c->{"0"} . ":" . $c->{"7"} . ";";
$bad = ["\0hidden" => 1];
try { clone(new stdClass(), $bad); echo "no throw"; } catch (Error $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "zero:seven;Cannot access property starting with \"\\0\"");
}

/// Verifies an unknown override name stores on an opted-in class silently and on an ORDINARY
/// class with php 8.5's dynamic-property deprecation.
///
/// Measured against php 8.5.10: `clone(new Plain(), ["zz" => "x"])` prints
/// `Deprecated: Creation of dynamic property Plain::$zz is deprecated`, the clone answers `x`,
/// and the source keeps exactly its declared properties. The clone-only hash is what makes the
/// stored value visible to `get_object_vars()` and to a runtime-name read.
#[test]
fn test_clone_function_stores_dynamic_properties_on_ordinary_classes_with_a_deprecation() {
    let out = compile_and_run_capture(
        r#"<?php
#[AllowDynamicProperties] class D { public int $n = 1; }
class Plain { public int $n = 1; }
$ov = ["n" => 2, "zz" => "x"];
$d = clone(new D(), $ov);
echo $d->n . ":" . $d->zz . ";";
$src = new Plain();
$c = clone($src, ["zz" => "x"]);
$key = "zz";
echo $c->{$key} . ":" . $c->n . ";";
echo json_encode(get_object_vars($c)) . ";" . json_encode(get_object_vars($src)) . ";";
echo var_export(property_exists($src, "zz"), true);
"#,
    );
    assert_eq!(
        out.stdout,
        "2:x;x:1;{\"n\":1,\"zz\":\"x\"};{\"n\":1};false"
    );
    assert!(
        out.stderr
            .contains("Deprecated: Creation of dynamic property Plain::$zz is deprecated"),
        "{}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("D::$zz"),
        "an #[AllowDynamicProperties] class must not deprecate: {}",
        out.stderr
    );
}

/// Verifies a user error handler observes the dynamic-property creation as `E_DEPRECATED`.
///
/// php 8.5.10 hands the handler errno `8192` and the message
/// `Creation of dynamic property Plain::$zz is deprecated`, and still stores the value.
#[test]
fn test_clone_function_dynamic_property_deprecation_reaches_a_user_error_handler() {
    let out = compile_and_run(
        r#"<?php
class Plain { public int $n = 1; }
set_error_handler(function ($no, $msg) {
    echo "handler(" . $no . "|" . ($no === E_DEPRECATED ? "E_DEPRECATED" : "other") . "|" . $msg . ");";
    return true;
});
$c = clone(new Plain(), ["zz" => "x"]);
$key = "zz";
echo $c->{$key};
"#,
    );
    assert_eq!(
        out,
        "handler(8192|E_DEPRECATED|Creation of dynamic property Plain::$zz is deprecated);x"
    );
}

/// Verifies masking `E_DEPRECATED` silences the report without dropping the stored value.
///
/// php 8.5.10 prints nothing under `error_reporting(E_ALL & ~E_DEPRECATED)` and still answers
/// `x`, because `error_reporting` gates the DEFAULT output only.
#[test]
fn test_clone_function_dynamic_property_deprecation_honors_the_reporting_mask() {
    let out = compile_and_run_capture(
        r#"<?php
error_reporting(E_ALL & ~E_DEPRECATED);
class Plain { public int $n = 1; }
$c = clone(new Plain(), ["zz" => "x"]);
$key = "zz";
echo $c->{$key};
"#,
    );
    assert_eq!(out.stdout, "x");
    assert!(
        !out.stderr.contains("Creation of dynamic property"),
        "{}",
        out.stderr
    );
}

/// Verifies `stdClass` and an INHERITED `#[AllowDynamicProperties]` stay exempt from the
/// deprecation while still storing the override.
#[test]
fn test_clone_function_dynamic_property_exemptions_are_preserved() {
    let out = compile_and_run_capture(
        r#"<?php
#[AllowDynamicProperties] class Base { public int $n = 1; }
class Child extends Base {}
$child = clone(new Child(), ["zz" => "x"]);
$std = clone(new stdClass(), ["a" => 1]);
echo $child->zz . ":" . $std->a;
"#,
    );
    assert_eq!(out.stdout, "x:1");
    assert!(
        !out.stderr.contains("Creation of dynamic property"),
        "{}",
        out.stderr
    );
}

/// Verifies `__set()` still wins over the clone-only hash for an unknown name.
///
/// php 8.5.10 answers `set(later=x);` with no deprecation and no stored property, so the magic
/// setter takes precedence over the reserved storage exactly as it does over php's own.
#[test]
fn test_clone_function_magic_set_wins_over_dynamic_property_storage() {
    let out = compile_and_run_capture(
        r#"<?php
class M {
    public int $n = 1;
    public function __set($k, $v) { echo "set(" . $k . "=" . $v . ");"; }
}
$m = clone(new M(), ["n" => 5, "later" => "x"]);
echo $m->n . ":" . json_encode(get_object_vars($m));
"#,
    );
    assert_eq!(out.stdout, "set(later=x);5:{\"n\":5}");
    assert!(
        !out.stderr.contains("Creation of dynamic property"),
        "{}",
        out.stderr
    );
}

/// Verifies a THROWING deprecation handler leaves the source intact and releases the clone.
///
/// php 8.5.10 answers `H;dtor;caught:boom;after;1dtor;`: the partial clone's destructor runs AT the
/// throw, the source survives with its declared value, and the trailing `dtor;` is the source's own
/// shutdown release. A leaked clone would drop the first `dtor;`, and a double release would add a
/// third one.
#[test]
fn test_clone_function_dynamic_property_deprecation_handler_throw_releases_the_clone() {
    let out = compile_and_run(
        r#"<?php
class Tracked {
    public int $n = 1;
    public function __destruct() { echo "dtor;"; }
}
set_error_handler(function ($no, $msg) { echo "H;"; throw new RuntimeException("boom"); });
$src = new Tracked();
try { clone($src, ["n" => 2, "zz" => "x"]); } catch (Throwable $e) { echo "caught:" . $e->getMessage() . ";"; }
restore_error_handler();
echo "after;" . $src->n;
"#,
    );
    assert_eq!(out, "H;dtor;caught:boom;after;1dtor;");
}

/// Verifies both arguments are evaluated once in source order before anything is applied, and
/// that the first failing key stops the rest.
///
/// The source object is inspected afterwards to prove the refused run touched only the clone.
#[test]
fn test_clone_function_evaluates_arguments_in_order_and_stops_at_the_first_error() {
    let out = compile_and_run(
        r#"<?php
class Z {
    public int $a = 0;
    private int $b = 0;
    public int $c = 0;
    public function all(): string { return $this->a . "/" . $this->b . "/" . $this->c; }
}
function mk(string $tag, $v) { echo "arg(" . $tag . ");"; return $v; }
$z = new Z();
$o = mk("obj", $z);
$ov = mk("ov", ["a" => 1, "b" => 2, "c" => 3]);
try { clone($o, $ov); echo "no throw;"; } catch (Error $e) { echo $e->getMessage() . ";"; }
echo $z->all();
"#,
    );
    assert_eq!(
        out,
        "arg(obj);arg(ov);Cannot access private property Z::$b;0/0/0"
    );
}

/// Verifies an enum case is uncloneable, with or without overrides.
#[test]
fn test_clone_function_refuses_to_clone_enum_cases() {
    let out = compile_and_run(
        r#"<?php
enum Suit: string { case Hearts = 'H'; }
try { clone(Suit::Hearts, ["value" => "S"]); echo "no throw"; } catch (Error $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "Trying to clone an uncloneable object of class Suit");
}

/// Verifies a refused override releases the clone exactly once and leaves the source intact.
#[test]
fn test_clone_function_releases_the_clone_when_an_override_is_refused() {
    let out = compile_and_run(
        r#"<?php
class Tracked {
    public int $n = 1;
    private int $secret = 0;
    public function __destruct() { echo "gone;"; }
}
function run(): void {
    $a = new Tracked();
    $ov = ["n" => 2, "secret" => 3];
    try { clone($a, $ov); echo "no throw;"; } catch (Error $e) { echo "caught:" . $e->getMessage() . ";"; }
    echo "src=" . $a->n . ";";
}
run();
echo "after";
"#,
    );
    assert_eq!(
        out,
        "gone;caught:Cannot access private property Tracked::$secret;src=1;gone;after"
    );
}

/// Verifies an UNTYPED property is `mixed` for an override, exactly as it is for an ordinary
/// assignment: no coercion toward the type its default happened to infer.
///
/// The inferred-`int` slot used to silently answer `int(0)` for a string override, and the
/// defaultless slot used to fail the whole build with an invalid `Void` payload.
#[test]
fn test_clone_function_writes_untyped_properties_without_coercion() {
    let out = compile_and_run(
        r#"<?php
class Tag { public int $v = 9; }
class U {
    public $fromInt = 0;
    public $fromString = "x";
    public $noDefault;
    public $fromArray = [];
    public $fromNull = null;
}
$ov = [
    "fromInt" => "hello",
    "fromString" => 5,
    "noDefault" => [1, 2],
    "fromArray" => null,
    "fromNull" => new Tag(),
];
$c = clone(new U(), $ov);
var_dump($c->fromInt);
var_dump($c->fromString);
var_dump($c->noDefault);
var_dump($c->fromArray);
var_dump($c->fromNull);
"#,
    );
    assert_eq!(
        out,
        "string(5) \"hello\"\nint(5)\narray(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(2)\n}\nNULL\nobject(Tag)#1 (1) {\n  [\"v\"]=>\n  int(9)\n}\n"
    );
}

/// Verifies a FIRST-CLASS CALLABLE `clone(...)` gives untyped slots the same `mixed` treatment.
///
/// The builtin check hook does not run for this shape: the call is checked through the callable
/// signature, so nothing recorded the override destination. The inferred-`int` slot answered
/// `int(0)` for a string override and the defaultless slot failed the whole build with
/// `prop_set assigning PHP type Mixed to U::$noDefault with PHP type Void`.
#[test]
fn test_clone_function_writes_untyped_properties_through_a_first_class_callable() {
    let out = compile_and_run(
        r#"<?php
class U { public $fromInt = 0; public $noDefault; }
$ov = ["fromInt" => "hello", "noDefault" => [1, 2]];
$f = clone(...);
$c = $f(new U(), $ov);
var_dump($c->fromInt);
var_dump($c->noDefault);
"#,
    );
    assert_eq!(
        out,
        "string(5) \"hello\"\narray(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(2)\n}\n"
    );
}

/// Verifies `call_user_func('clone', ...)` gives untyped slots the same `mixed` treatment.
#[test]
fn test_clone_function_writes_untyped_properties_through_call_user_func() {
    let out = compile_and_run(
        r#"<?php
class U { public $fromInt = 0; public $noDefault; }
$ov = ["fromInt" => "hello", "noDefault" => [1, 2]];
$c = call_user_func('clone', new U(), $ov);
var_dump($c->fromInt);
var_dump($c->noDefault);
"#,
    );
    assert_eq!(
        out,
        "string(5) \"hello\"\narray(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(2)\n}\n"
    );
}

/// Verifies `call_user_func_array('clone', ...)` gives untyped slots the same `mixed` treatment.
#[test]
fn test_clone_function_writes_untyped_properties_through_call_user_func_array() {
    let out = compile_and_run(
        r#"<?php
class U { public $fromInt = 0; public $noDefault; }
$ov = ["fromInt" => "hello", "noDefault" => [1, 2]];
$c = call_user_func_array('clone', [new U(), $ov]);
var_dump($c->fromInt);
var_dump($c->noDefault);
"#,
    );
    assert_eq!(
        out,
        "string(5) \"hello\"\narray(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(2)\n}\n"
    );
}

/// Verifies a RUNTIME STRING callable `clone` gives untyped slots the same `mixed` treatment.
///
/// `$f = 'clone'; $f($object, [...])` resolves its callee at run time, so neither the builtin
/// check hook nor the callable-signature path ever saw a `clone` call and nothing recorded the
/// override destination. The inferred-`int` slot answered `int(0)` for a string override.
///
/// The second callable is selected out of an array at run time, which is the shape that can never
/// be narrowed to an exact string, and the last call is a plain one-argument `clone` through the
/// same variable, which must keep working untouched. Expected output is real `LC_ALL=C php` 8.5
/// output.
#[test]
fn test_clone_function_writes_untyped_properties_through_a_runtime_string_callable() {
    let out = compile_and_run(
        r#"<?php
class U { public $fromInt = 0; public $noDefault; }
$ov = ["fromInt" => "hello", "noDefault" => [1, 2]];
$f = 'clone';
$c = $f(new U(), $ov);
var_dump($c->fromInt);
var_dump($c->noDefault);
$names = ['strtolower', 'clone'];
$g = $names[1];
$d = $g(new U(), $ov);
var_dump($d->fromInt);
$plain = $f($c);
var_dump($plain->fromInt);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "string(5) \"hello\"\n",
            "array(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(2)\n}\n",
            "string(5) \"hello\"\n",
            "string(5) \"hello\"\n",
        )
    );
}

/// Verifies a DECLARED array, associative-array and nullable slot materializes a runtime override
/// value safely, including `null` for every nullable form.
///
/// `null` used to reach the slot as an invalid `Void` payload rather than the slot's null
/// representation, which is why the nullable object slot is exercised in both directions.
#[test]
fn test_clone_function_materializes_array_and_nullable_override_values() {
    let out = compile_and_run(
        r#"<?php
class Tag { public int $v = 9; }
class T {
    public array $list = [];
    public array $map = ["k" => 1];
    public ?int $maybeInt = 7;
    public ?string $maybeStr = "s";
    public ?Tag $maybeTag = null;
}
$ov = [
    "list" => [4, 5, 6],
    "map" => ["a" => 1, "b" => 2],
    "maybeInt" => null,
    "maybeStr" => null,
    "maybeTag" => new Tag(),
];
$c = clone(new T(), $ov);
echo count($c->list) . ":" . $c->list[2] . ";";
echo count($c->map) . ":" . $c->map["b"] . ";";
var_dump($c->maybeInt);
var_dump($c->maybeStr);
echo $c->maybeTag->v . ";";
$back = clone($c, ["maybeTag" => null]);
var_dump($back->maybeTag);
"#,
    );
    assert_eq!(out, "3:6;2:2;NULL\nNULL\n9;NULL\n");
}

/// Verifies a declared `iterable` property accepts every runtime shape PHP's `array|Traversable`
/// admits when the value arrives through a `clone()` override: an indexed array, an associative
/// array, an object implementing `Iterator`, and an object implementing `IteratorAggregate`.
///
/// The override value is only a boxed `Mixed` at the write, so acceptance is decided by the shared
/// weak-mode typed-property guard and the shared store path, not by a rule the applicator owns.
/// `IteratorAggregate` is listed separately because reading it back travels a DIFFERENT resolution
/// path: the loop has to call `getIterator()` and iterate its result. Expected output follows PHP
/// 8.5 semantics; no reference `php` command was run to produce it.
#[test]
fn test_clone_function_accepts_iterable_property_overrides() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $current;
    private int $end;
    public function __construct(int $start, int $end) {
        $this->current = $start;
        $this->end = $end;
    }
    public function rewind(): void {}
    public function valid(): bool { return $this->current < $this->end; }
    public function current(): int { return $this->current; }
    public function key(): int { return $this->current; }
    public function next(): void { $this->current = $this->current + 1; }
}
class Values implements IteratorAggregate {
    public function getIterator(): Iterator { return new Range(0, 3); }
}
class Holder { public iterable $it; }
function dump(iterable $items): void {
    foreach ($items as $k => $v) {
        echo $k;
        echo '=';
        echo $v;
        echo ';';
    }
    echo '|';
}
$base = new Holder();
$indexed = clone($base, ["it" => [10, 20]]);
dump($indexed->it);
$assoc = clone($base, ["it" => ["a" => 1, "b" => 2]]);
dump($assoc->it);
$iterator = clone($base, ["it" => new Range(2, 5)]);
dump($iterator->it);
$aggregate = clone($base, ["it" => new Values()]);
dump($aggregate->it);
"#,
    );
    assert_eq!(out, "0=10;1=20;|a=1;b=2;|2=2;3=3;4=4;|0=0;1=1;2=2;|");
}

/// Verifies a declared `iterable` property refuses a scalar and an ordinary non-`Traversable`
/// object with the same catchable `TypeError` every other typed property raises, and that the
/// refusal stops the override loop before any later entry is applied.
///
/// The later key routes to `__set`, so its echo is the observable proof that php's
/// stop-at-first-error rule held: the applicator throws out of the `foreach` and never reaches it.
/// Expected output follows PHP 8.5 semantics; no reference `php` command was run to produce it.
#[test]
fn test_clone_function_rejects_non_iterable_property_overrides() {
    let out = compile_and_run(
        r#"<?php
class Plain { public int $v = 1; }
class Holder {
    public iterable $it;
    public function __set(string $name, mixed $value): void { echo "set:" . $name . ";"; }
}
function dump(iterable $items): void {
    foreach ($items as $v) {
        echo $v;
        echo ';';
    }
}
$base = new Holder();
try {
    clone($base, ["it" => 5, "later" => 1]);
    echo "no throw;";
} catch (TypeError $e) {
    echo "caught:" . $e->getMessage() . ";";
}
try {
    clone($base, ["it" => new Plain(), "later" => 1]);
    echo "no throw;";
} catch (TypeError $e) {
    echo "caught:" . $e->getMessage() . ";";
}
$ok = clone($base, ["it" => [7]]);
dump($ok->it);
"#,
    );
    assert_eq!(
        out,
        "caught:Cannot assign int to property Holder::$it of type Traversable|array;\
caught:Cannot assign Plain to property Holder::$it of type Traversable|array;7;"
    );
}

/// Verifies a nullable `iterable` property accepts both `null` and a valid iterable override.
///
/// `?iterable` is stored as a boxed union rather than the raw pointer a bare `iterable` slot holds,
/// so it resolves through the union arm of the same guard: the array shapes and `Traversable` join
/// the declared `null` member instead of the slot growing its own rule. Expected output follows PHP
/// 8.5 semantics; no reference `php` command was run to produce it.
#[test]
fn test_clone_function_accepts_nullable_iterable_property_override() {
    let out = compile_and_run(
        r#"<?php
class Holder { public ?iterable $it = null; }
$base = new Holder();
$filled = clone($base, ["it" => [4, 5]]);
$items = $filled->it;
foreach ($items as $k => $v) {
    echo $k;
    echo '=';
    echo $v;
    echo ';';
}
$emptied = clone($filled, ["it" => null]);
var_dump($emptied->it);
"#,
    );
    assert_eq!(out, "0=4;1=5;NULL\n");
}

/// Verifies repeated `iterable` clone overrides leave the heap balanced under `--heap-debug`.
///
/// The `iterable` store is the one place in this change that moves an owner: it promotes the
/// PAYLOAD out of the boxed override value, retains that payload for the slot, and releases the
/// slot's previous contents through `__rt_decref_any`. Every container shape and both object
/// protocols run through it here, repeatedly and inside a function so the locals are released on
/// return, which is what turns a one-per-write leak, a double release or a use-after-free into a
/// visible failure rather than a passing assertion. Expected output follows PHP 8.5 semantics; no
/// reference `php` command was run to produce it.
#[test]
fn test_clone_function_iterable_overrides_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Range implements Iterator {
    private int $current;
    private int $end;
    public function __construct(int $start, int $end) {
        $this->current = $start;
        $this->end = $end;
    }
    public function rewind(): void {}
    public function valid(): bool { return $this->current < $this->end; }
    public function current(): int { return $this->current; }
    public function key(): int { return $this->current; }
    public function next(): void { $this->current = $this->current + 1; }
}
class Values implements IteratorAggregate {
    public function getIterator(): Iterator { return new Range(0, 2); }
}
class Holder { public iterable $it; }
function dump(iterable $items): void {
    foreach ($items as $v) {
        echo $v;
    }
    echo '|';
}
function cycle(): void {
    $base = new Holder();
    for ($i = 0; $i < 3; $i = $i + 1) {
        $indexed = clone($base, ["it" => [1, 2]]);
        dump($indexed->it);
        $assoc = clone($indexed, ["it" => ["a" => 3]]);
        dump($assoc->it);
        $iterator = clone($assoc, ["it" => new Range(4, 6)]);
        dump($iterator->it);
        $aggregate = clone($iterator, ["it" => new Values()]);
        dump($aggregate->it);
    }
}
cycle();
"#,
    );
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "12|3|45|01|12|3|45|01|12|3|45|01|", "{}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Verifies an UNTYPED property that is also a reference gives an override the `mixed` PAYLOAD.
///
/// `property_reference_slots` says the slot physically holds a shared cell; `properties[slot].1`
/// says what that cell CARRIES. The widening used to skip every reference slot, so an aliased
/// `public $u = 0;` coerced a string override back to `int(0)` while the unaliased slot next to it
/// answered the string. Both the alias, the source and the clone observe the override, because php
/// 8.5 writes a reference destination through the shared cell. Expected output is real
/// `LC_ALL=C php` 8.5 output.
#[test]
fn test_clone_function_writes_untyped_reference_properties_without_coercion() {
    let out = compile_and_run(
        r#"<?php
class U { public $u = 0; public $n; }
$o = new U();
$alias = &$o->u;
$nAlias = &$o->n;
$c = clone($o, ["u" => "hello", "n" => [1, 2]]);
var_dump($alias);
var_dump($o->u);
var_dump($c->u);
var_dump($nAlias);
var_dump($c->n);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "string(5) \"hello\"\n",
            "string(5) \"hello\"\n",
            "string(5) \"hello\"\n",
            "array(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(2)\n}\n",
            "array(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(2)\n}\n",
        )
    );
}

/// Verifies a DECLARED typed property that is also a reference keeps its DECLARED payload type.
///
/// The undeclared-payload widening must not reach it: a widened slot would have stored the string
/// `"9"` verbatim, while php applies weak-mode property typing and stores `int(9)` through the
/// shared cell. Expected output is real `LC_ALL=C php` 8.5 output.
#[test]
fn test_clone_function_keeps_declared_typed_reference_property_coercion() {
    let out = compile_and_run(
        r#"<?php
class T { public int $p = 1; }
$o = new T();
$alias = &$o->p;
$c = clone($o, ["p" => "9"]);
var_dump($alias);
var_dump($o->p);
var_dump($c->p);
"#,
    );
    assert_eq!(out, "int(9)\nint(9)\nint(9)\n");
}

/// Verifies a DECLARED typed REFERENCE destination rejects a value weak mode cannot convert.
///
/// The reference destination must run the same weak-mode typed-property guard the ordinary slot
/// runs, and it must run it BEFORE the shared cell is touched. This compiler used to coerce
/// `"nope"` to `int(0)` and publish it through the cell, so every alias of the property observed a
/// value php never stores. Expected output is real `LC_ALL=C php` 8.5 output.
#[test]
fn test_clone_function_rejects_invalid_typed_reference_destination_overrides() {
    let out = compile_and_run(
        r#"<?php
class T { public int $p = 1; }
$o = new T();
$alias = &$o->p;
try {
    clone($o, ["p" => "nope"]);
} catch (TypeError $e) {
    echo $e->getMessage(), "|";
}
var_dump($alias);
var_dump($o->p);
"#,
    );
    assert_eq!(
        out,
        "Cannot assign string to property T::$p of type int|int(1)\nint(1)\n"
    );
}

/// Verifies a REFUSED reference destination releases the clone and the override value.
///
/// The guard throws out of the applicator with the clone already allocated and the boxed override
/// value still live, so both have to be retired on the unwind path. The destructor position pins
/// the clone's release and the heap-debug leak summary pins the boxed string. Expected stdout is
/// real `LC_ALL=C php` 8.5 output.
#[test]
fn test_clone_function_releases_everything_when_a_reference_destination_is_refused() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Tracked {
    public int $n = 1;
    public function __destruct() { echo "gone;"; }
}
function run(): void {
    $a = new Tracked();
    $alias = &$a->n;
    try { clone($a, ["n" => "nope"]); echo "no throw;"; } catch (TypeError $e) { echo "caught:" . $e->getMessage() . ";"; }
    echo "src=" . $alias . ";";
}
run();
echo "after";
"#,
    );
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout,
        "gone;caught:Cannot assign string to property Tracked::$n of type int;src=1;gone;after",
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Verifies the same guard on a `float` and a nullable `?int` REFERENCE destination.
///
/// `float` and `?int` reach different backend storage from the plain `int` slot (a float register
/// and the tagged-scalar pair), so each needs its own proof that the reference destination refuses
/// what php refuses and coerces what php coerces. Expected output is real `LC_ALL=C php` 8.5
/// output.
#[test]
fn test_clone_function_guards_float_and_nullable_reference_destinations() {
    let out = compile_and_run(
        r#"<?php
class F { public float $f = 1.5; public ?int $n = 3; }
$o = new F();
$fAlias = &$o->f;
$nAlias = &$o->n;
try {
    clone($o, ["f" => "nope"]);
} catch (TypeError $e) {
    echo $e->getMessage(), "|";
}
try {
    clone($o, ["n" => [1]]);
} catch (TypeError $e) {
    echo $e->getMessage(), "|";
}
var_dump($fAlias);
var_dump($nAlias);
$c = clone($o, ["f" => "2.5", "n" => null]);
var_dump($fAlias);
var_dump($nAlias);
var_dump($c->f);
var_dump($c->n);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "Cannot assign string to property F::$f of type float|",
            "Cannot assign array to property F::$n of type ?int|",
            "float(1.5)\n",
            "int(3)\n",
            "float(2.5)\n",
            "NULL\n",
            "float(2.5)\n",
            "NULL\n",
        )
    );
}

/// Verifies an override whose DESTINATION property is a reference writes THROUGH the shared cell.
///
/// php 8.5 permits this: every alias of the property observes the override. This compiler used to
/// refuse the whole call with `Cannot modify by-reference property`, which is a diagnostic php
/// never produces for a reference destination.
#[test]
fn test_clone_function_writes_through_a_reference_destination_property() {
    let out = compile_and_run(
        r#"<?php
class RefProp { public int $p = 1; }
$o = new RefProp();
$alias = &$o->p;
$ov = ["p" => 9];
$c = clone($o, $ov);
echo $alias . "|" . $c->p . "|" . $o->p;
"#,
    );
    assert_eq!(out, "9|9|9");
}

/// Verifies an override ARRAY ELEMENT that is itself a reference never reaches the applicator.
///
/// php 8.5 raises `Cannot assign by reference when cloning with updated properties` at run time.
/// This compiler has no by-reference element representation at all, so both syntactic ways of
/// building one are refused before any clone runs. The test pins that refusal so the day element
/// references become representable, the clone path is revisited rather than silently applying an
/// aliased value.
#[test]
fn test_clone_function_never_receives_a_by_reference_override_element() {
    let source = r#"<?php
class R { public int $n = 1; }
$v = 5;
$ov = ["n" => &$v];
clone(new R(), $ov);
"#;
    let tokens = elephc::lexer::tokenize(source).expect("tokenize failed");
    let error = elephc::parser::parse(&tokens).expect_err("a by-reference element must be refused");
    assert!(
        error
            .message
            .contains("Reference elements in array literals"),
        "unexpected diagnostic: {}",
        error.message
    );
}

/// Verifies an inherited SAME-NAME private property resolves to the slot the invocation scope
/// owns, in both ancestry directions, and stays inaccessible from global scope.
///
/// The applicator writes an ancestor's private slot through a scoped setter helper. Falling back
/// to `$this->p = $v` when that helper is missing would hit the CHILD's shadowing slot instead,
/// so a missing helper now stops the build rather than writing a different property.
#[test]
fn test_clone_function_writes_the_scope_private_slot_of_a_shadowed_property() {
    let out = compile_and_run(
        r#"<?php
class Base {
    private int $p = 1;
    public function cloneP(Child $o): Child { return clone($o, ["p" => 10]); }
    public function readP(): int { return $this->p; }
}
class Child extends Base {
    private int $p = 2;
    public function cloneC(Child $o): Child { return clone($o, ["p" => 20]); }
    public function readC(): int { return $this->p; }
}
$c = new Child();
$byBase = $c->cloneP($c);
echo $byBase->readP() . ":" . $byBase->readC() . ";";
$byChild = $c->cloneC($c);
echo $byChild->readP() . ":" . $byChild->readC() . ";";
try { clone($c, ["p" => 99]); echo "no throw"; } catch (Error $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(out, "10:2;1:20;Cannot access private property Child::$p");
}

/// Verifies an override key that collides with a STRICT ancestor's private property creates a
/// dynamic property instead of writing that private slot, from every scope but the declaring one.
///
/// php 7.4 removed shadow properties, so `P`'s `private int $n` is simply not in `C`'s by-name
/// table: measured against php 8.5.10, the same fixture prints `7;1/8;1/9;1` and deprecates the
/// creation of `C::$n` twice. The declaring scope still selects the private slot (7), while the
/// child scope and global scope leave it at 1 and store 8 and 9 in the per-instance hash, which
/// is exactly what the runtime-name reads answer. The source object keeps its own 1 throughout.
///
/// The applicator PLANNED this correctly all along, `crate::ir_lower::clone_overrides::arms`
/// answers `DynamicAssign` here, but the backend's runtime-name ladder was built from the
/// physical slot table with no scope input and matched the name against the ancestor's slot.
#[test]
fn test_clone_function_creates_a_dynamic_property_for_an_ancestor_private_name() {
    let out = compile_and_run_capture(
        r#"<?php
class P {
    private int $n = 1;
    public function readN(): int { return $this->n; }
    public function cloneParent(): P { return clone($this, ["n" => 7]); }
}
class C extends P {
    public function cloneChild(): C { return clone($this, ["n" => 8]); }
}
$c = new C();
$key = "n";
$byParent = $c->cloneParent();
echo $byParent->readN() . ";";
$byChild = $c->cloneChild();
echo $byChild->readN() . "/" . $byChild->{$key} . ";";
$byGlobal = clone($c, ["n" => 9]);
echo $byGlobal->readN() . "/" . $byGlobal->{$key} . ";";
echo $c->readN();
"#,
    );
    assert_eq!(out.stdout, "7;1/8;1/9;1");
    assert_eq!(
        out.stderr
            .matches("Creation of dynamic property C::$n is deprecated")
            .count(),
        2,
        "the child-scope and global-scope clones each create the dynamic property once: {}",
        out.stderr
    );
}

/// Verifies user functions whose names look exactly like the generated clone helpers cannot
/// shadow them.
///
/// The applicator and the scoped setters used to be named `_clone_apply_<id>_<n>` and
/// `_clone_set_<id>_<n>`, which are legal PHP function names: declaring one made the module skip
/// the generated body, call the user body with `(clone, overrides)` and drop every override.
#[test]
fn test_clone_function_symbols_cannot_be_shadowed_by_user_functions() {
    let out = compile_and_run(
        r#"<?php
class Box { public int $n = 1; }
function _clone_apply_4_0($a, $b) { echo "user_apply;"; }
function _clone_set_4_0($a, $b) { echo "user_set;"; }
function _clone_apply_5_0($a, $b) { echo "user_apply5;"; }
$ov = ["n" => 5];
$c = clone(new Box(), $ov);
echo $c->n;
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies the `clone` KEYWORD refuses an enum case exactly like the `clone()` function does.
///
/// The two-argument function form answered from the clone's runtime class id, while the keyword
/// form knew the class statically and handed back a second copy of the singleton.
#[test]
fn test_clone_keyword_refuses_to_clone_enum_cases() {
    let out = compile_and_run(
        r#"<?php
enum E: string { case A = 'a'; }
try { $x = clone E::A; echo "no throw"; } catch (Error $e) { echo get_class($e) . ":" . $e->getMessage(); }
"#,
    );
    assert_eq!(out, "Error:Trying to clone an uncloneable object of class E");
}

/// Verifies an override array built from an explicit pair PLUS a spread keeps every spread entry,
/// in either order, with php's later-wins overwrite rule.
///
/// The parser used to drop every spread once a literal had turned associative, so
/// `["n" => 4, ...$rest]` silently reached `clone()` carrying only `n`.
#[test]
fn test_clone_function_applies_a_mixed_literal_and_spread_override_array() {
    let out = compile_and_run(
        r#"<?php
class Box { public int $n = 1; public string $s = "a"; public int $m = 2; }
$rest = ["s" => "z", "m" => 7];
$b = clone(new Box(), ["n" => 4, ...$rest]);
echo $b->n . ":" . $b->s . ":" . $b->m . ";";
$c = clone(new Box(), [...$rest, "s" => "late", "n" => 9]);
echo $c->n . ":" . $c->s . ":" . $c->m . ";";
$d = clone(new Box(), ["s" => "first", ...$rest]);
echo $d->n . ":" . $d->s . ":" . $d->m;
"#,
    );
    assert_eq!(out, "4:z:7;9:late:7;1:z:7");
}

/// Verifies an associative array literal containing a spread keeps php's key rules exactly.
///
/// Explicit integer keys stay put and seed the auto-key cursor, spread integer keys are
/// renumbered from that cursor, string keys overwrite in source order, and a positional item
/// after a spread takes the run-time next free key.
#[test]
fn test_assoc_array_literal_spread_preserves_php_key_semantics() {
    let out = compile_and_run(
        r#"<?php
$r = ["a" => 1, 7 => "seven", "b" => 2];
$idx = [10, 20];
$k = "dyn";
var_export([5 => 'x', ...$r]); echo "\n";
var_export([1, 2, ...$r, "z" => 9]); echo "\n";
var_export(["a" => 100, ...$r, "a" => 999]); echo "\n";
var_export(["k" => 1, ...$idx, 5, 6]); echo "\n";
var_export([$k => 1, ...$r]); echo "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "array (\n  5 => 'x',\n  'a' => 1,\n  6 => 'seven',\n  'b' => 2,\n)\n",
            "array (\n  0 => 1,\n  1 => 2,\n  'a' => 1,\n  2 => 'seven',\n  'b' => 2,\n  'z' => 9,\n)\n",
            "array (\n  'a' => 999,\n  0 => 'seven',\n  'b' => 2,\n)\n",
            "array (\n  'k' => 1,\n  0 => 10,\n  1 => 20,\n  2 => 5,\n  3 => 6,\n)\n",
            "array (\n  'dyn' => 1,\n  'a' => 1,\n  0 => 'seven',\n  'b' => 2,\n)\n",
        )
    );
}

/// Verifies a runtime override name that matches no declared slot is never silently dropped.
///
/// `stdClass` stores it as a dynamic property, and a class with `__set` routes it through the
/// magic setter. The ordinary-class case is pinned by
/// `test_clone_function_stores_dynamic_properties_on_ordinary_classes_with_a_deprecation`.
#[test]
fn test_clone_function_routes_unknown_names_to_dynamic_storage_or_magic_set() {
    let out = compile_and_run(
        r#"<?php
$o = new stdClass();
$o->a = 1;
$ov = ["a" => 2, "fresh" => "new"];
$c = clone($o, $ov);
echo $c->a . ":" . $c->fresh . ":" . $o->a . ";";
class M {
    public int $n = 1;
    public function __set($k, $v) { echo "set(" . $k . "=" . $v . ");"; }
}
$m = clone(new M(), ["n" => 5, "later" => "x"]);
echo $m->n;
"#,
    );
    assert_eq!(out, "2:new:1;set(later=x);5");
}

/// Verifies a live foreach-by-reference alias makes the corresponding clone override illegal.
#[test]
fn test_clone_function_rejects_live_foreach_reference_override() {
    let out = compile_and_run(
        r#"<?php
class RefBox { public int $u = 0; }
$overrides = ["u" => 1];
foreach ($overrides as &$value) {}
try {
    clone(new RefBox(), $overrides);
    echo "no throw";
} catch (Error $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Cannot assign by reference when cloning with updated properties"
    );
}

/// Verifies removing the only local alias leaves an ordinary by-value clone override.
#[test]
fn test_clone_function_accepts_foreach_reference_after_last_alias_unset() {
    let out = compile_and_run(
        r#"<?php
class RefBox { public int $u = 0; }
$overrides = ["u" => 1];
foreach ($overrides as &$value) {}
unset($value);
$copy = clone(new RefBox(), $overrides);
echo $copy->u;
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies one surviving local alias keeps the entry referenced after its sibling is unset.
#[test]
fn test_clone_function_keeps_multiple_foreach_aliases_referenced() {
    let out = compile_and_run(
        r#"<?php
class RefBox { public int $u = 0; }
$overrides = ["u" => 1];
foreach ($overrides as &$value) {}
$other =& $value;
unset($value);
try {
    clone(new RefBox(), $overrides);
    echo "no throw";
} catch (Error $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Cannot assign by reference when cloning with updated properties"
    );
}

/// Verifies a COW split preserves the shared PHP reference cell in both resulting hashes.
#[test]
fn test_clone_function_preserves_reference_identity_across_hash_cow() {
    let out = compile_and_run(
        r#"<?php
class RefBox { public int $u = 0; }
$original = ["u" => 1];
foreach ($original as &$value) {}
$copy = $original;
$copy["extra"] = 2;
unset($value);
try {
    clone(new RefBox(), $original);
    echo "original:no throw;";
} catch (Error $e) {
    echo "original:" . $e->getMessage() . ";";
}
try {
    clone(new RefBox(), $copy);
    echo "copy:no throw";
} catch (Error $e) {
    echo "copy:" . $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "original:Cannot assign by reference when cloning with updated properties;\
copy:Cannot assign by reference when cloning with updated properties"
    );
}

/// Verifies writing through a foreach alias changes the value without ending the reference set.
#[test]
fn test_clone_function_rejects_reference_override_after_write_through() {
    let out = compile_and_run(
        r#"<?php
class RefBox { public int $u = 0; }
$overrides = ["u" => 1];
foreach ($overrides as &$value) {}
$value = 9;
echo $overrides["u"] . ":";
try {
    clone(new RefBox(), $overrides);
    echo "no throw";
} catch (Error $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "9:Cannot assign by reference when cloning with updated properties"
    );
}

/// Verifies an override applied BEFORE a referenced entry survives that entry's refusal.
///
/// php evaluates `clone($o, $withProperties)` entry by entry, so it writes every earlier
/// override and only throws once iteration REACHES the reference. The refusal used to be a
/// whole-array pre-scan that threw before the applicator ran at all, which silently dropped
/// writes php had already performed. The earlier write is observed through a destination
/// reference alias the clone shares with the source object, so it cannot be mistaken for a
/// value read back out of the discarded clone.
#[test]
fn test_clone_function_applies_prior_override_before_later_reference_error() {
    let out = compile_and_run(
        r#"<?php
class Pair { public int $a = 1; public int $b = 2; }
$o = new Pair();
$alias = &$o->a;
$overrides = ["a" => 7, "b" => 8];
foreach ($overrides as &$value) {}
try {
    clone($o, $overrides);
    echo "no throw";
} catch (Error $e) {
    echo $alias . ":" . $o->a . ":" . $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "7:7:Cannot assign by reference when cloning with updated properties"
    );
}

/// Verifies an INDEXED override array carries the same live-alias refusal as a hash one.
///
/// A by-reference `foreach` over an indexed array used to bind the alias straight to the packed
/// payload slot, which has nowhere to record that the entry joined a PHP reference set, so this
/// clone silently applied both overrides. The loop now promotes the local to integer-keyed hash
/// storage with boxed Mixed entries, which is where the persistent marker lives.
///
/// One program pins three facts at once: the write through the live alias reaches the SOURCE
/// (`$overrides[1]` reads `9`), the earlier numeric key is applied before the refusal
/// (`set(0=1)` is printed), and the referenced entry raises php 8.5's exact message.
#[test]
fn test_clone_function_rejects_live_indexed_foreach_reference_override() {
    let out = compile_and_run(
        r#"<?php
class Sink {
    public int $n = 0;
    public function __set($k, $v) { echo "set(" . $k . "=" . $v . ");"; }
}
$overrides = [1, 2];
foreach ($overrides as &$value) {}
$value = 9;
echo $overrides[1] . ";";
try {
    clone(new Sink(), $overrides);
    echo "no throw";
} catch (Error $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "9;set(0=1);Cannot assign by reference when cloning with updated properties"
    );
}

/// Verifies dropping the only alias leaves an indexed override array fully by-value again.
///
/// Both numeric keys must survive the promotion as the property names php stringifies them to,
/// which is also what proves the promotion preserved integer keys and insertion order.
#[test]
fn test_clone_function_accepts_indexed_foreach_reference_after_last_alias_unset() {
    let out = compile_and_run(
        r#"<?php
$overrides = [1, 2];
foreach ($overrides as &$value) {}
unset($value);
$copy = clone(new stdClass(), $overrides);
echo $copy->{"0"} . ":" . $copy->{"1"};
"#,
    );
    assert_eq!(out, "1:2");
}

/// Verifies a copy taken BEFORE the by-reference loop keeps its own values and no reference state.
///
/// The loop's copy-on-write split must leave the promotion, the alias write and the entry marker
/// on the iterated branch only: `$copy` still reads `1,2`, and cloning with it applies both
/// numeric keys instead of refusing.
#[test]
fn test_clone_function_preserves_indexed_reference_cow() {
    let out = compile_and_run(
        r#"<?php
$original = [1, 2];
$copy = $original;
foreach ($original as &$value) {}
$value = 9;
echo $original[0] . "," . $original[1] . "|" . $copy[0] . "," . $copy[1] . ";";
try {
    $c = clone(new stdClass(), $copy);
    echo "copy:" . $c->{"0"} . $c->{"1"} . ";";
} catch (Error $e) {
    echo "copy:" . $e->getMessage() . ";";
}
try {
    clone(new stdClass(), $original);
    echo "original:no throw";
} catch (Error $e) {
    echo "original:" . $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "1,9|1,2;copy:12;\
original:Cannot assign by reference when cloning with updated properties"
    );
}

/// Verifies the internal entry marker is absent from ordinary reads and `var_dump()` output.
#[test]
fn test_clone_function_hides_reference_marker_from_reads_and_var_dump() {
    let out = compile_and_run(
        r#"<?php
$overrides = ["u" => 1];
foreach ($overrides as &$value) {}
unset($value);
var_dump($overrides);
echo $overrides["u"];
"#,
    );
    assert_eq!(out, "array(1) {\n  [\"u\"]=>\n  int(1)\n}\n1");
}

/// Verifies every target emits both the explicit hash provenance and clone guard paths.
#[test]
fn test_every_supported_target_emits_clone_reference_override_guard() {
    let dir = std::env::temp_dir().join(format!(
        "elephc_clone_reference_override_targets_{}_{:?}",
        std::process::id(),
        std::thread::current().id(),
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("clone_dynamic.php"),
        r#"<?php
class TargetCloneReference {
    public int $u = 0;
}
#[Export]
function exerciseCloneReference(): int {
    $overrides = ["u" => 1];
    foreach ($overrides as &$value) {}
    try { clone(new TargetCloneReference(), $overrides); } catch (Error $e) {}
    return 1;
}
exerciseCloneReference();
"#,
    )
    .unwrap();

    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let assembly = emit_clone_dynamic_property_assembly(&dir, target);
        for expected in [
            "__rt_hash_to_mixed",
            "Cannot assign by reference when cloning with updated properties",
        ] {
            assert!(
                assembly.contains(expected),
                "{target}: clone reference lowering is missing {expected}"
            );
        }
        let target_needles: &[&str] = if target == "linux-x86_64" {
            &[
                "mov rcx, QWORD PTR [r10 + 24]",
                "mov r9, QWORD PTR [r10 + 40]",
                "cmp r9, 11",
                "mov r10d, DWORD PTR [rcx - 12]",
                "cmp r10d, 1",
                "ja ",
                "clone_reference_override_reject",
            ]
        } else {
            &[
                "ldr x3, [x6, #24]",
                "ldr x5, [x6, #40]",
                "cmp x5, #11",
                "ldr w10, [x3, #-12]",
                "cmp w10, #1",
                "b.hi ",
                "clone_reference_override_reject",
            ]
        };
        for &expected in target_needles {
            assert!(
                assembly.contains(expected),
                "{target}: clone reference lowering is missing {expected}"
            );
        }
    }

    std::fs::remove_dir_all(&dir).ok();
}

/// Verifies every supported target emits the clone-only dynamic-property path from shared helpers.
///
/// Only two of the five targets can run here, and the creation probe, the deprecation fragments and
/// the hash store are the layout-sensitive half of this feature: a target whose emitter lost one of
/// them would store silently, report nothing, or report on every write instead of on creation.
#[test]
fn test_every_supported_target_emits_the_clone_dynamic_property_deprecation_and_store() {
    let dir = std::env::temp_dir().join(format!(
        "elephc_clone_dynamic_property_targets_{}_{:?}",
        std::process::id(),
        std::thread::current().id(),
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("clone_dynamic.php"),
        r#"<?php
class TargetClonePlain { public int $n = 1; }
$name = "zz";
$clone = clone(new TargetClonePlain(), ["zz" => "x"]);
echo $clone->{$name};
"#,
    )
    .unwrap();

    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let assembly = emit_clone_dynamic_property_assembly(&dir, target);
        // Slice the generated applicator out of the module so a match cannot come from an
        // unrelated function or from the data section at the end of the file. Mach-O comments
        // start with `;` and ELF comments with `#`, so the marker is matched from `@fn name=`.
        let applicator = assembly
            .split_once("@fn name=@clone_apply")
            .unwrap_or_else(|| panic!("{target}: no clone override applicator emitted:\n{assembly}"))
            .1;
        let applicator = applicator
            .split_once("@fn name=")
            .map_or(applicator, |(body, _)| body);
        for expected in [
            // The creation probe: php reports only when the key is absent.
            "__rt_hash_get",
            "__rt_diag_warning_fragment",
            "__rt_diag_warning",
            // The store itself, so a silenced report can never pass as a fix.
            "__rt_hash_set",
        ] {
            assert!(
                applicator.contains(expected),
                "{target}: applicator is missing {expected}"
            );
        }
        assert!(
            assembly.contains("Deprecated: Creation of dynamic property TargetClonePlain::$"),
            "{target}: the deprecation prefix is not interned"
        );
        assert!(
            assembly.contains(" is deprecated"),
            "{target}: the deprecation suffix is not interned"
        );
    }

    std::fs::remove_dir_all(&dir).ok();
}

/// Emits the clone-override fixture's assembly for one target and returns its text.
///
/// iOS targets refuse a standalone executable, so they are asked for a static library; the
/// generated applicator the assertion reads is emitted either way.
fn emit_clone_dynamic_property_assembly(dir: &std::path::Path, target: &str) -> String {
    let binary = std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    });
    let mut command = std::process::Command::new(binary);
    command.env("XDG_CACHE_HOME", dir.join("cache-root"));
    command.current_dir(dir);
    command.args(["--emit-asm", "--target", target]);
    if target.starts_with("ios") {
        command.args(["--emit", "staticlib"]);
    }
    let output = command
        .arg("clone_dynamic.php")
        .output()
        .expect("failed to run elephc");
    assert!(
        output.status.success(),
        "{target}: emitting assembly failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::read_to_string(dir.join("clone_dynamic.s")).expect("emitted assembly")
}

/// Verifies a two-argument `clone()` on a BUILTIN receiver refuses instead of failing the build.
///
/// The `mixed` parameter makes the site runtime-shaped, which is the branch that plans an
/// applicator for every candidate class. A catalog builtin owns its own layout, so it gets no
/// applicator and the class-id dispatch misses into the existing override guard. The trailing
/// `count($box)` proves the builtin's own metadata was never restamped on the way through.
#[test]
fn test_clone_function_refuses_overrides_on_a_builtin_receiver() {
    let out = compile_and_run(
        r#"<?php
function copyWith(mixed $object): mixed { return clone($object, ["storage" => "x"]); }
$box = new ArrayObject([1, 2]);
try { copyWith($box); echo "no throw;"; } catch (Error $e) { echo "caught:" . $e->getMessage() . ";"; }
echo count($box);
"#,
    );
    assert_eq!(
        out,
        "caught:clone(): Argument #2 ($withProperties) property overrides are not supported for this class;2"
    );
}

/// Verifies an inherited UNTYPED parent slot is described by ONE type in the parent and the child.
///
/// The override widening used to restamp the destination class and its subclasses only, so an
/// inherited `private $n = 1;` became `mixed` in `C` and stayed `int` in `P` while both named the
/// SAME physical storage. `P`'s own accessor then read the child-written box back as a raw
/// pointer, and the untouched SOURCE object answered a pointer too because its slot had been
/// initialized through the child's widened view. Expected output is real `LC_ALL=C php` 8.5 output.
#[test]
fn test_clone_function_widens_inherited_untyped_slots_consistently() {
    let out = compile_and_run(
        r#"<?php
class P {
    private $n = 1;
    public function readN(): string { return gettype($this->n) . "(" . var_export($this->n, true) . ")"; }
    public static function fromP(C $o, array $ov): void { $r = clone($o, $ov); echo "clone:" . $r->readN() . ";"; }
}
class C extends P {}
$c = new C();
P::fromP($c, ["n" => "x"]);
echo "src:" . $c->readN() . ";";
"#,
    );
    assert_eq!(out, "clone:string('x');src:integer(1);");
}

/// Verifies the same inherited slot holds an OBJECT value without the process crashing.
///
/// A closure in an inherited untyped parent slot is the shape where the two disagreeing views were
/// fatal rather than merely wrong: the parent stored a bare object pointer, the child's widened
/// view released it as a boxed value, and the produced binary died with SIGSEGV before printing
/// anything. Expected output is real `LC_ALL=C php` 8.5 output.
#[test]
fn test_clone_function_keeps_inherited_object_valued_untyped_slots_alive() {
    let out = compile_and_run(
        r#"<?php
class P {
    private $cb;
    public function __construct() { $this->cb = strlen(...); }
    public function kind(): string { return gettype($this->cb); }
    public static function fromP(C $o, array $ov): void { $r = clone($o, $ov); echo "clone:" . $r->kind() . ";"; }
}
class C extends P {}
$c = new C();
P::fromP($c, ["cb" => "x"]);
echo "src:" . $c->kind() . ";";
"#,
    );
    assert_eq!(out, "clone:string;src:object;");
}

/// Verifies a USER subclass of a catalog builtin still clones with overrides, layout untouched.
///
/// Slot propagation can reach inherited storage rooted at `ArrayObject`. The catalog owns that
/// storage, so the subclass must keep its own slot widened and leave every slot it merely INHERITED
/// from the builtin exactly as the catalog laid it out. `count($r)` and `count($b)` read the
/// builtin's own storage through its own accessor, which is what a restamped inherited slot would
/// break. Expected output is real `LC_ALL=C php` 8.5 output.
#[test]
fn test_clone_function_preserves_builtin_layout_for_user_subclasses() {
    let out = compile_and_run(
        r#"<?php
class MyBox extends ArrayObject {
    public int $tag = 1;
    public static function copyWith(MyBox $o, array $ov): MyBox { return clone($o, $ov); }
}
$b = new MyBox([1, 2]);
try { $r = MyBox::copyWith($b, ["tag" => 5]); echo "ok:" . $r->tag . ":" . count($r) . ";"; }
catch (Error $e) { echo "caught:" . $e->getMessage() . ";"; }
echo "src:" . count($b) . ";";
"#,
    );
    assert_eq!(out, "ok:5:2;src:2;");
}
