//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object-oriented PHP inheritance, including class protected members are accessible inside class methods, class protected static method is callable inside class, and inheritance dynamic dispatch uses child override.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies protected member `$value` and protected method `next()` are callable
/// from public method `reveal()` inside the same class, returning 42.
#[test]
fn test_class_protected_members_are_accessible_inside_class_methods() {
    let out = compile_and_run(
        r#"<?php
class SecretBox {
    protected $value = 41;

    protected function next() {
        return $this->value + 1;
    }

    public function reveal() {
        return $this->next();
    }
}

$box = new SecretBox();
echo $box->reveal();
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies protected static method `base()` is callable via fully-qualified name
/// `SecretMath::base()` from within public static method `answer()`, returning 42.
#[test]
fn test_class_protected_static_method_is_callable_inside_class() {
    let out = compile_and_run(
        r#"<?php
class SecretMath {
    protected static function base() {
        return 41;
    }

    public static function answer() {
        return SecretMath::base() + 1;
    }
}

echo SecretMath::answer();
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies dynamic dispatch selects the `Dog::speak()` override when `$dog->run()`
/// calls `$this->speak()`, returning "dog".
#[test]
fn test_inheritance_dynamic_dispatch_uses_child_override() {
    let out = compile_and_run(
        r#"<?php
class Animal {
    public function speak() {
        return "animal";
    }

    public function run() {
        return $this->speak();
    }
}

class Dog extends Animal {
    public function speak() {
        return "dog";
    }
}

$dog = new Dog();
echo $dog->run();
"#,
    );
    assert_eq!(out, "dog");
}

/// Inferring a parent's method return must leave a child's overriding return ABI intact.
#[test]
fn test_inherited_return_inference_preserves_child_override() {
    let out = compile_and_run(
        r#"<?php
class BaseValue { public function value() { return 3; } }
class ChildValue extends BaseValue { public function value() { return 'child'; } }
$base = new BaseValue();
$child = new ChildValue();
echo $base->value() . ',' . $child->value();
"#,
    );
    assert_eq!(out, "3,child");
}

/// Verifies private methods use lexical binding: `Base::reveal()` calls `Base::secret()`
/// even when the object is a `Child` instance, returning "base". Private methods are
/// not polymorphic and are resolved at the defining class at compile time.
#[test]
fn test_inheritance_parent_private_method_stays_lexically_bound() {
    let out = compile_and_run(
        r#"<?php
class Base {
    private function secret() {
        return "base";
    }

    public function reveal() {
        return $this->secret();
    }
}

class Child extends Base {
    public function secret() {
        return "child";
    }
}

$child = new Child();
echo $child->reveal();
"#,
    );
    assert_eq!(out, "base");
}

/// Verifies `self::label()` is lexically bound to `Base::label()` even when called on
/// a `Child` instance, returning "base". Self resolves at the compile-time class.
#[test]
fn test_self_static_call_uses_lexical_class() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public static function label() {
        return "base";
    }

    public function reveal() {
        return self::label();
    }
}

class Child extends Base {
    public static function label() {
        return "child";
    }
}

$child = new Child();
echo $child->reveal();
"#,
    );
    assert_eq!(out, "base");
}

/// Verifies `self::label()` resolves to `Base::label()` (lexical binding) even when
/// called on a `Child` instance via `Base::reveal()`, returning "base".
#[test]
fn test_self_instance_call_stays_lexically_bound() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public function reveal() {
        return self::label();
    }

    public function label() {
        return "base";
    }
}

class Child extends Base {
    public function label() {
        return "child";
    }
}

$child = new Child();
echo $child->reveal();
"#,
    );
    assert_eq!(out, "base");
}

/// Verifies `static::who()` (late static binding) resolves to the actual runtime class
/// `Child` when called from an instance method `reveal()` on a `Child` object, returning "child".
#[test]
fn test_static_late_binding_uses_child_override_from_instance_method() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public static function who() {
        return "base";
    }

    public function reveal() {
        return static::who();
    }
}

class Child extends Base {
    public static function who() {
        return "child";
    }
}

$child = new Child();
echo $child->reveal();
"#,
    );
    assert_eq!(out, "child");
}

/// Verifies `static::who()` (late static binding) resolves to `Child` when called from
/// `Child::relay()`, returning "child". Late static binding works from static methods.
#[test]
fn test_static_late_binding_uses_child_override_from_static_method() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public static function who() {
        return "base";
    }

    public static function relay() {
        return static::who();
    }
}

class Child extends Base {
    public static function who() {
        return "child";
    }
}

echo Child::relay();
"#,
    );
    assert_eq!(out, "child");
}

/// Verifies named static call `A::who()` is non-forwarding (uses `A`'s vtable) while
/// `self::who()` is forwarding (resolved lexically to `A::who()`). Output is "A B".
#[test]
fn test_named_static_call_is_non_forwarding_but_self_is_forwarding() {
    let out = compile_and_run(
        r#"<?php
class A {
    public static function who() {
        return static::tag();
    }

    public static function relayNamed() {
        return A::who();
    }

    public static function relaySelf() {
        return self::who();
    }

    public static function tag() {
        return "A";
    }
}

class B extends A {
    public static function tag() {
        return "B";
    }
}

echo B::relayNamed() . " " . B::relaySelf();
"#,
    );
    assert_eq!(out, "A B");
}

/// Verifies `parent::who()` forwards the static call while still using runtime late binding
/// for `static::tag()`, returning "B". Parent:: forwards but does not reset the runtime class.
#[test]
fn test_parent_static_call_is_forwarding() {
    let out = compile_and_run(
        r#"<?php
class A {
    public static function who() {
        return static::tag();
    }

    public static function tag() {
        return "A";
    }
}

class B extends A {
    public static function relay() {
        return parent::who();
    }

    public static function tag() {
        return "B";
    }
}

echo B::relay();
"#,
    );
    assert_eq!(out, "B");
}

/// Verifies `static::m()` on a private static `A::m()` looks `m` up in the late-bound class,
/// from a static and an instance method: a class that does not redeclare `m` runs `A::m()`, a
/// public or protected redeclaration (or one inherited from it) runs instead with `static` still
/// bound to the called class, and a private one throws `Error`. `$this->own()` on a private
/// instance method stays on `A::own()`.
#[test]
fn test_static_late_binding_private_scope_method_uses_called_class_lookup() {
    let out = compile_and_run(
        r#"<?php
class A {
    private static function m(): string { return "A::m"; }
    private function own(): string { return "A::own"; }
    public static function run(): string {
        try { return static::m(); } catch (Error $e) { return get_class($e) . ": " . $e->getMessage(); }
    }
    public function inst(): string {
        try { $late = static::m(); } catch (Error $e) { $late = $e->getMessage(); }
        return $late . " | " . $this->own();
    }
}
class B extends A {}
class C extends A { public static function m(): string { return "C::m@" . static::class; } }
class D extends A {
    private static function m(): string { return "D::m"; }
    public function own(): string { return "D::own"; }
}
class E extends A { protected static function m(): string { return "E::m"; } }
class C2 extends C {}
class D2 extends D {}
class E2 extends E { public static function m(): string { return "E2::m"; } }
echo "A: ", A::run(), " / ", (new A())->inst(), "\n";
echo "B: ", B::run(), " / ", (new B())->inst(), "\n";
echo "C: ", C::run(), " / ", (new C())->inst(), "\n";
echo "D: ", D::run(), " / ", (new D())->inst(), "\n";
echo "E: ", E::run(), " / ", (new E())->inst(), "\n";
echo "C2: ", C2::run(), " / ", (new C2())->inst(), "\n";
echo "D2: ", D2::run(), " / ", (new D2())->inst(), "\n";
echo "E2: ", E2::run(), " / ", (new E2())->inst(), "\n";
"#,
    );
    assert_eq!(
        out,
        "A: A::m / A::m | A::own\n\
         B: A::m / A::m | A::own\n\
         C: C::m@C / C::m@C | A::own\n\
         D: Error: Call to private method D::m() from scope A / Call to private method D::m() from scope A | A::own\n\
         E: E::m / E::m | A::own\n\
         C2: C::m@C2 / C::m@C2 | A::own\n\
         D2: Error: Call to private method D::m() from scope A / Call to private method D::m() from scope A | A::own\n\
         E2: E2::m / E2::m | A::own\n"
    );
}

/// Verifies a late-bound redeclaration of a private static method binds the call's arguments
/// with its own signature (a nullable parameter keeps `null`, an added optional parameter takes
/// its default), returns through its own return type, and works for `void` methods.
#[test]
fn test_static_late_binding_private_scope_method_binds_redeclared_signature() {
    let out = compile_and_run(
        r#"<?php
class A {
    private static function m(string $s): string { return "A::m " . var_export($s, true); }
    public static function run(mixed $v): string { return static::m($v); }
    private static function log(string $s): void { echo "A::log $s\n"; }
    public static function note(): void { static::log("n"); }
}
class C extends A {
    public static function m(?string $x): string { return "C::m " . var_export($x, true); }
    public static function log(string $s): void { echo "C::log $s\n"; }
}
class F extends A {
    public static function m(string $x, int $n = 7): int { return strlen($x) + $n; }
}
echo A::run("x"), "\n";
echo C::run(null), "\n";
echo C::run("y"), "\n";
var_dump(F::run("abc"));
A::note();
C::note();
F::note();
"#,
    );
    assert_eq!(
        out,
        "A::m 'x'\nC::m NULL\nC::m 'y'\nstring(2) \"10\"\nA::log n\nC::log n\nA::log n\n"
    );
}

/// Verifies each late-bound redeclaration of a private static method checks the call's
/// arguments against its own signature, only when it is selected: a missing parameter throws
/// `ArgumentCountError`, a renamed one rejects the named argument, and the scope's method keeps
/// binding. The dispatch result keeps the selected method's type inside an array literal.
#[test]
fn test_static_late_binding_private_scope_method_checks_each_selected_binding() {
    let out = compile_and_run(
        r#"<?php
class A {
    private static function m(int $x): string { return "A::m $x"; }
    private static function t(): string { return "7"; }
    public static function run(): string {
        try { return static::m(1); } catch (\ArgumentCountError $e) { return get_class($e) . ": " . strstr($e->getMessage(), ",", true); }
    }
    public static function named(): string {
        try { return static::m(x: 2); } catch (\Error $e) { return get_class($e) . ": " . explode(",", $e->getMessage())[0]; }
    }
    public static function types(): void { var_dump([static::t()]); }
}
class C extends A {
    public static function m(int $x, int $extra): string { return "C::m $x $extra"; }
    public static function t(): int { return 7; }
}
class R extends A { public static function m(int $y): string { return "R::m $y"; } }
echo A::run(), "\n", C::run(), "\n", R::run(), "\n";
echo A::named(), "\n", C::named(), "\n", R::named(), "\n";
A::types();
C::types();
"#,
    );
    assert_eq!(
        out,
        "A::m 1\nArgumentCountError: Too few arguments to function C::m()\nR::m 1\n\
         A::m 2\nArgumentCountError: Too few arguments to function C::m()\n\
         Error: Unknown named parameter $x\n\
         array(1) {\n  [0]=>\n  string(1) \"7\"\n}\narray(1) {\n  [0]=>\n  int(7)\n}\n"
    );
}

/// Verifies `$r =& static::m()` over a reference-returning private method binds the scope
/// method's property for the scope class, and a fresh cell for a by-value redeclaration.
#[test]
fn test_static_late_binding_private_scope_method_reference_assignment() {
    let out = compile_and_run(
        r#"<?php
error_reporting(E_ALL & ~E_NOTICE);
class Box { public int $v = 1; }
class A {
    public static Box $box;
    private static function &m(): int { $b = self::$box; return $b->v; }
    public static function run(): string {
        $r =& static::m();
        $r++;
        return static::class . ": r=$r v=" . self::$box->v;
    }
}
class C extends A { public static function m(): int { return 40; } }
A::$box = new Box();
echo A::run(), "\n", C::run(), "\n", A::run(), "\n";
"#,
    );
    assert_eq!(out, "A: r=2 v=2\nC: r=41 v=2\nA: r=3 v=3\n");
}

/// Verifies `static::m()` over a private static `A::m()` when the called class redeclares `m`
/// as an instance method: a static caller throws the non-static `Error` and an instance caller
/// runs it on `$this` (inherited by `G2`, protected in `K`), both before evaluating the
/// Verifies `$r =& static::m()` raises php-src's `Only variables should be assigned by
/// reference` notice when the selected redeclaration returns by value, and not for the private
/// method that returns a reference.
#[test]
fn test_static_late_binding_private_scope_method_reference_assignment_notice() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $no, string $msg): bool { echo "[$no:$msg]"; return true; });
class Box { public int $v = 1; }
class A {
    public static Box $box;
    private static function &m(): int { $b = self::$box; return $b->v; }
    public static function run(): string {
        $r =& static::m();
        $r++;
        return static::class . ": r=$r v=" . self::$box->v;
    }
}
class C extends A { public static function m(): int { return 40; } }
A::$box = new Box();
echo A::run(), "\n";
echo C::run(), "\n";
echo A::run(), "\n";
"#,
    );
    assert_eq!(out, "A: r=2 v=2\n[8:Only variables should be assigned by reference]C: r=41 v=2\nA: r=3 v=3\n");
}

/// arguments when they throw. A private instance redeclaration throws the private-call `Error`.
#[test]
fn test_static_late_binding_private_scope_method_instance_redeclaration() {
    let out = compile_and_run(
        r#"<?php
function arg(int $v): int { echo "[arg $v]"; return $v; }
class A {
    private static function m(int $x = 1): string { return "A::m"; }
    public static function run(): void {
        try { echo static::class, " static: "; echo static::m(arg(5)), "\n"; }
        catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
    }
    public function inst(): void {
        try { echo static::class, " inst: "; echo static::m(arg(6)), "\n"; }
        catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
    }
}
class G extends A {
    public string $tag = "g";
    public function m(int $x): string { return "G::m $x " . get_class($this) . " " . $this->tag . " " . static::class; }
}
class G2 extends G {}
class H extends A { private function m(): string { return "H::m"; } }
class K extends A { protected function m(int $x): int { return $x * 10; } }
A::run(); G::run(); G2::run(); H::run(); K::run();
(new A)->inst(); (new G)->inst(); (new G2)->inst(); (new H)->inst(); (new K)->inst();
"#,
    );
    assert_eq!(out, "A static: [arg 5]A::m\nG static: Error: Non-static method G::m() cannot be called statically\nG2 static: Error: Non-static method G::m() cannot be called statically\nH static: Error: Call to private method H::m() from scope A\nK static: Error: Non-static method K::m() cannot be called statically\nA inst: [arg 6]A::m\nG inst: [arg 6]G::m 6 G g G\nG2 inst: [arg 6]G::m 6 G2 g G2\nH inst: Error: Call to private method H::m() from scope A\nK inst: [arg 6]60\n");
}

/// Verifies an abstract redeclaration, which has no body, links: a caller with `$this` never
/// sees an abstract called class, and a static caller whose called class is abstract (`B`, or
/// `S2` inheriting `S`'s abstract static `m`) raises php-src's "Cannot call abstract method".
#[test]
fn test_static_late_binding_private_scope_method_abstract_redeclaration() {
    let out = compile_and_run(
        r#"<?php
class A {
    private static function m(): string { return "A::m"; }
    public static function run(): void {
        try { echo static::class, " static: ", static::m(), "\n"; }
        catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
    }
    public function inst(): void { echo static::class, " inst: ", static::m(), "\n"; }
}
abstract class B extends A { abstract public function m(): string; }
class C extends B { public function m(): string { return "C::m"; } }
abstract class S extends A { abstract public static function m(): string; }
abstract class S2 extends S {}
class T extends S2 { public static function m(): string { return "T::m"; } }
(new A)->inst(); (new C)->inst(); (new T)->inst();
A::run(); B::run(); C::run(); S::run(); S2::run(); T::run();
"#,
    );
    assert_eq!(out, "A inst: A::m\nC inst: C::m\nT inst: T::m\nA static: A::m\nB static: Error: Cannot call abstract method B::m()\nC static: Error: Non-static method C::m() cannot be called statically\nS static: Error: Cannot call abstract method S::m()\nS2 static: Error: Cannot call abstract method S::m()\nT static: T::m\n");
}

/// Verifies a private redeclaration the scope cannot call falls back to the called class's
/// `__call()` from an instance caller and to its `__callStatic()` otherwise, with `static`
/// forwarded (`D2`), while a class with `__callStatic()` and no redeclaration keeps `A::m()`
/// and a public instance redeclaration still throws the non-static `Error` statically.
#[test]
fn test_static_late_binding_private_scope_method_magic_fallback() {
    let out = compile_and_run(
        r#"<?php
function arg(int $v): int { echo "[arg $v]"; return $v; }
class A {
    private static function m(int $x = 1): string { return "A::m"; }
    public static function run(): void {
        try { echo static::class, " static: "; echo static::m(arg(5)), "\n"; }
        catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
    }
    public function inst(): void {
        try { echo static::class, " inst: "; echo static::m(arg(6)), "\n"; }
        catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
    }
}
class B extends A {
    public static function __callStatic(string $n, array $a): string { return "B::__callStatic($n)"; }
}
class D extends A {
    private static function m(): string { return "D::m"; }
    public static function __callStatic(string $n, array $a): string { return "D::__callStatic($n," . implode(",", $a) . ") " . static::class; }
    public function __call(string $n, array $a): string { return "D::__call($n," . implode(",", $a) . ")"; }
}
class D2 extends D {}
class E extends A {
    private static function m(): string { return "E::m"; }
    public function __call(string $n, array $a): string { return "E::__call($n)"; }
}
class H extends A {
    private function m(): string { return "H::m"; }
    public static function __callStatic(string $n, array $a): string { return "H::__callStatic($n)"; }
}
class J extends A {
    public function m(int $x): string { return "J::m"; }
    public static function __callStatic(string $n, array $a): string { return "J::__callStatic($n)"; }
}
A::run(); B::run(); D::run(); D2::run(); E::run(); H::run(); J::run();
(new B)->inst(); (new D)->inst(); (new D2)->inst(); (new E)->inst(); (new H)->inst(); (new J)->inst();
"#,
    );
    assert_eq!(out, "A static: [arg 5]A::m\nB static: [arg 5]A::m\nD static: [arg 5]D::__callStatic(m,5) D\nD2 static: [arg 5]D::__callStatic(m,5) D2\nE static: Error: Call to private method E::m() from scope A\nH static: [arg 5]H::__callStatic(m)\nJ static: Error: Non-static method J::m() cannot be called statically\nB inst: [arg 6]A::m\nD inst: [arg 6]D::__call(m,6)\nD2 inst: [arg 6]D::__call(m,6)\nE inst: [arg 6]E::__call(m)\nH inst: [arg 6]H::__callStatic(m)\nJ inst: [arg 6]J::m\n");
}

/// Verifies a redeclaration's `ArgumentCountError` names the call's file and line as php-src
/// does, for an exact and a minimum parameter count, and a variadic redeclaration binds.
#[test]
fn test_static_late_binding_private_scope_method_argument_count_location() {
    let out = compile_and_run(
        r#"<?php
class A {
    private static function m(int $x): string { return "A::m $x"; }
    public static function run(): string {
        try { return static::m(1); }
        catch (\ArgumentCountError $e) { return get_class($e) . ": " . str_replace(__FILE__, "FILE", $e->getMessage()); }
    }
}
class C extends A { public static function m(int $x, int $extra): string { return "C::m $x $extra"; } }
class V extends A { public static function m(int $x, int ...$more): string { return "V::m"; } }
class W extends A { public static function m(int $x, int $y, int $z = 0): string { return "W::m"; } }
echo A::run(), "\n", C::run(), "\n", V::run(), "\n", W::run(), "\n";
"#,
    );
    assert_eq!(out, "A::m 1\nArgumentCountError: Too few arguments to function C::m(), 1 passed in FILE on line 5 and exactly 2 expected\nV::m\nArgumentCountError: Too few arguments to function W::m(), 1 passed in FILE on line 5 and at least 2 expected\n");
}

/// Verifies the `__callStatic()`/`__call()` fallback for a private redeclaration receives named
/// arguments under their names, after the positional ones, as php-src passes them.
#[test]
fn test_static_late_binding_private_scope_method_magic_fallback_named_arguments() {
    let out = compile_and_run(
        r#"<?php
function show(array $a): string { $s = ""; foreach ($a as $k => $v) { $s .= "$k=$v,"; } return $s; }
class A {
    private static function m(int $x, int $y = 0): string { return "A::m $x $y"; }
    public static function run(): void { echo static::class, " ", static::m(1, y: 5), "\n"; }
    public function inst(): void { echo static::class, " ", static::m(x: 2, y: 3), "\n"; }
}
class D extends A {
    private static function m(): string { return "D::m"; }
    public static function __callStatic(string $n, array $a): string { return "__callStatic($n) " . show($a); }
    public function __call(string $n, array $a): string { return "__call($n) " . show($a); }
}
A::run();
D::run();
(new A)->inst();
(new D)->inst();
"#,
    );
    assert_eq!(out, "A A::m 1 5\nD __callStatic(m) 0=1,y=5,\nA A::m 2 3\nD __call(m) x=2,y=3,\n");
}

/// Verifies an error handler that throws on the by-reference notice of `$r =& static::m()`
/// releases the by-value result first: its destructor runs before the catch, and nothing leaks
/// once the handler is restored.
#[test]
fn test_static_late_binding_private_scope_method_reference_notice_throw_releases_result() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Obj { public function __construct(public string $n) {} public function __destruct() { echo "destruct {$this->n}\n"; } }
class Holder { public Obj $o; public function __construct() { $this->o = new Obj("a"); } }
set_error_handler(function (int $no, string $msg): bool { throw new ErrorException($msg, 0, $no); });
class A {
    public static Holder $h;
    private static function &m(): Obj { $h = self::$h; return $h->o; }
    public static function run(): void {
        try { $r =& static::m(); echo "bound ", $r->n, "\n"; }
        catch (ErrorException $e) { echo "caught: ", $e->getMessage(), "\n"; }
        echo "after ", static::class, "\n";
    }
}
class C extends A { public static function m(): Obj { return new Obj("c"); } }
A::$h = new Holder();
A::run();
C::run();
restore_error_handler();
echo "end\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "bound a\nafter A\ndestruct c\ncaught: Only variables should be assigned by reference\nafter C\nend\ndestruct a\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Verifies same-signature redeclarations dispatched inside one call: `C::m()` runs for `C` and
/// its subclass `C2` and protected `P::m()` for `P`. `C::m()` writes a static property, so the
/// call keeps its effects although `A::m()` is pure: the unused call and both `static::m(7)`
/// run.
#[test]
fn test_static_late_binding_private_scope_method_same_signature_dispatch() {
    let out = compile_and_run(
        r#"<?php
class A {
    private static function m(int $i): int { return $i; }
    public static function run(): int {
        static::m(1);
        $sum = 0;
        for ($k = 0; $k < 3; $k++) { $sum += static::m($k); }
        return $sum + static::m(7) + static::m(7);
    }
}
class B extends A {}
class C extends A {
    public static int $calls = 0;
    public static function m(int $i): int { self::$calls++; return $i + 100; }
}
class C2 extends C {}
class P extends A { protected static function m(int $i): int { return -$i; } }
echo A::run(), " ", B::run(), " ", C::run(), " ", C2::run(), " ", P::run(), " ", C::$calls, "\n";
"#,
    );
    assert_eq!(out, "17 17 517 517 -17 12\n");
}

/// Verifies inherited properties (`$a`) and methods are accessible from child, and
/// `parent::greet()` calls the parent's version, returning "42 hi!".
#[test]
fn test_inheritance_parent_method_call_and_inherited_properties() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public $a = 40;

    public function greet() {
        return "hi";
    }
}

class Child extends Base {
    public $b = 2;

    public function total() {
        return $this->a + $this->b;
    }

    public function greet() {
        return parent::greet() . "!";
    }
}

$child = new Child();
echo $child->total() . " " . $child->greet();
"#,
    );
    assert_eq!(out, "42 hi!");
}

/// Verifies protected method `readValue()` and protected property `$value` are accessible
/// from a subclass via `$this`, returning 42.
#[test]
fn test_inheritance_protected_members_are_accessible_from_subclass() {
    let out = compile_and_run(
        r#"<?php
class Base {
    protected $value = 41;

    protected function readValue() {
        return $this->value;
    }
}

class Child extends Base {
    public function reveal() {
        return $this->readValue() + 1;
    }
}

$child = new Child();
echo $child->reveal();
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies first-class callable syntax `MathBox::double(...)` compiles and calls the
/// static method correctly, returning 18.
#[test]
fn test_first_class_callable_static_method_indirect_call() {
    let out = compile_and_run(
        r#"<?php
class MathBox {
    public static function double($n) {
        return $n * 2;
    }
}

$fn = MathBox::double(...);
echo $fn(9);
"#,
    );
    assert_eq!(out, "18");
}

/// Verifies first-class callable on an untyped static method accepts string arguments
/// and returns "Hello World".
#[test]
fn test_first_class_callable_untyped_static_method_accepts_string_args() {
    let out = compile_and_run(
        r#"<?php
class Greeter {
    public static function greet($name) {
        return "Hello " . $name;
    }
}

$f = Greeter::greet(...);
echo $f("World");
"#,
    );
    assert_eq!(out, "Hello World");
}

/// Verifies typed property redeclaration with an initializer overrides the parent's
/// default value: `Child::$x = 5` shadows `Base::$x = 1`, returning "5".
#[test]
fn test_property_redeclaration_concrete_overrides_default() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public int $x = 1;
}

class Child extends Base {
    public int $x = 5;
}

$c = new Child();
echo $c->x;
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies untyped property redeclaration with an initializer overrides the parent's
/// default value: `Child::$value = 2` shadows `Base::$value = 1`, returning "2".
#[test]
fn test_property_redeclaration_untyped() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public $value = 1;
}

class Child extends Base {
    public $value = 2;
}

$c = new Child();
echo $c->value;
"#,
    );
    assert_eq!(out, "2");
}

/// Verifies property redeclaration can widen visibility from `protected` to `public`
/// while preserving the value, returning "20:20".
#[test]
fn test_property_redeclaration_widens_visibility() {
    let out = compile_and_run(
        r#"<?php
class Base {
    protected int $value = 10;

    public function get() {
        return $this->value;
    }
}

class Child extends Base {
    public int $value = 20;
}

$c = new Child();
echo $c->value;
echo ":";
echo $c->get();
"#,
    );
    assert_eq!(out, "20:20");
}

/// Verifies property redeclaration preserves the parent slot offset for non-redeclared
/// properties: `Child::$a = 10` redeclares `$a` but `$b` stays at Base's offset,
/// so `pair()` returns 12 (10+2), and direct access returns "12:10:2".
#[test]
fn test_property_redeclaration_preserves_slot_offset() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public int $a = 1;
    public int $b = 2;

    public function pair() {
        return $this->a + $this->b;
    }
}

class Child extends Base {
    public int $a = 10;
}

$c = new Child();
echo $c->pair();
echo ":";
echo $c->a;
echo ":";
echo $c->b;
"#,
    );
    assert_eq!(out, "12:10:2");
}

/// A legal readonly redeclaration preserves the parent modifier and initializes its slot.
#[test]
fn test_property_redeclaration_preserves_readonly() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public public(set) readonly int $value;
}

class Child extends Base {
    public public(set) readonly int $value;

    public function __construct() {
        $this->value = 7;
    }
}

$c = new Child();
echo $c->value;
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies multi-level property redeclaration: `Child::$value = 3` shadows both
/// `Parent_::$value = 2` and `GrandParent::$value = 1`, returning "3:3".
#[test]
fn test_property_redeclaration_multi_level_inheritance() {
    let out = compile_and_run(
        r#"<?php
class GrandParent {
    public int $value = 1;

    public function show() {
        return $this->value;
    }
}

class Parent_ extends GrandParent {
    public int $value = 2;
}

class Child extends Parent_ {
    public int $value = 3;
}

$c = new Child();
echo $c->value;
echo ":";
echo $c->show();
"#,
    );
    assert_eq!(out, "3:3");
}

/// Verifies property redeclaration can both widen visibility (`protected` to `public`)
/// while preserving `readonly`, returning "9:9".
#[test]
fn test_property_redeclaration_widens_visibility_and_preserves_readonly() {
    let out = compile_and_run(
        r#"<?php
class Base {
    protected readonly int $value;

    public function get() {
        return $this->value;
    }
}

class Child extends Base {
    public readonly int $value;

    public function __construct() {
        $this->value = 9;
    }
}

$c = new Child();
echo $c->value;
echo ":";
echo $c->get();
"#,
    );
    assert_eq!(out, "9:9");
}

/// Verifies property redeclaration works across trait application: `Child::$value = 5`
/// redeclares `HasValue::$value = 1` brought in via `Base`, returning "5".
#[test]
fn test_property_redeclaration_from_trait() {
    let out = compile_and_run(
        r#"<?php
trait HasValue {
    public int $value = 1;
}

class Base {
    use HasValue;
}

class Child extends Base {
    public int $value = 5;
}

$c = new Child();
echo $c->value;
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies a child class can redeclare a promoted property from a parent's constructor.
/// The parent's promoted property `$value` is initialized by the call (`new Child(42)`),
/// while the child's redeclared `$value = 7` is not used. The child also inherits the
/// parent's `show()` method which reads the parent's slot. Output is "42:42".
#[test]
fn test_property_redeclaration_redeclares_parent_promoted_property() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public function __construct(public int $value = 1) {}

    public function show() {
        return $this->value;
    }
}

class Child extends Base {
    public int $value = 7;
}

$c = new Child(42);
echo $c->show();
echo ":";
echo $c->value;
"#,
    );
    assert_eq!(out, "42:42");
}

/// Verifies a child property can shadow a private parent property with a fresh
/// slot: parent methods keep reading the private slot while child/global reads
/// see the child property.
#[test]
fn test_private_parent_property_shadowing_uses_separate_slots() {
    let out = compile_and_run(
        r#"<?php
class Base {
    private int $value;

    public function __construct() {
        $this->value = 2;
    }

    public function parentValue() {
        return $this->value;
    }
}

class Child extends Base {
    public $value = "child";

    public function childValue() {
        return $this->value;
    }
}

$c = new Child();
echo $c->parentValue();
echo ":";
echo $c->childValue();
echo ":";
echo $c->value;
"#,
    );
    assert_eq!(out, "2:child:child");
}

/// Verifies a later non-private redeclaration updates the visible parent slot,
/// while an older private grandparent slot stays separate for grandparent methods.
#[test]
fn test_private_grandparent_property_shadowing_survives_later_redeclaration() {
    let out = compile_and_run(
        r#"<?php
class GrandParentBox {
    private int $value = 1;

    public function grandParentValue() {
        return $this->value;
    }
}

class ParentBox extends GrandParentBox {
    public int $value = 2;

    public function parentValue() {
        return $this->value;
    }
}

class ChildBox extends ParentBox {
    public int $value = 3;
}

$c = new ChildBox();
echo $c->grandParentValue();
echo ":";
echo $c->parentValue();
echo ":";
echo $c->value;
"#,
    );
    assert_eq!(out, "1:3:3");
}

/// A class built before its parent still answers its own ancestry for covariant returns.
///
/// `ATrigger` sorts first, so building it recurses into `ZChild` before `ZChild` is registered.
/// The covariant `?ZChild` override must still see that `ZChild` extends `ZParent`.
#[test]
fn test_covariant_return_validates_while_parent_is_under_construction() {
    let out = compile_and_run(r#"<?php
class ZParent {
    public function make(): ?ZParent { return null; }
}
class ZChild extends ZParent {
    public function make(): ?ZChild { return null; }
    public function name(): string { return 'child'; }
}
class ATrigger extends ZChild {}
$child = new ATrigger();
var_dump($child->make(), $child->name());
"#);
    assert_eq!(out, "NULL\nstring(5) \"child\"\n");
}

/// A covariant return may name a class the name-ordered class-build walk has not reached yet:
/// `BDef::make(): ZEnumNode` overrides `ADef::make(): Node` before `ZEnumNode` is built, both
/// for a subclass of the parent's return class and for a class implementing the parent's
/// return interface through an ancestor.
#[test]
fn test_covariant_return_to_a_class_built_later() {
    let out = compile_and_run(
        r#"<?php
namespace N;
interface Node {}
interface Scalar extends Node {}
class Variable implements Scalar {}
class ZEnumNode extends Variable {}
class ADef { public function make(): Node { return new Variable(); } public function get(): Node { return $this->make(); } }
class BDef extends ADef { public function make(): ZEnumNode { return new ZEnumNode(); } }
class CDef extends ADef { public function make(): Scalar { return new ZEnumNode(); } }
class ScalarDef { protected function make(): Variable { return new Variable(); } public function get(): Variable { return $this->make(); } }
class EnumDef extends ScalarDef { protected function make(): ZEnumNode { return new ZEnumNode(); } }
echo get_class((new BDef())->get()), "|", get_class((new CDef())->get()), "|", get_class((new EnumDef())->get());
"#,
    );
    assert_eq!(out, "N\\ZEnumNode|N\\ZEnumNode|N\\ZEnumNode");
}

/// Enums are built after every class, so a covariant return to an enum implementing the
/// parent's (or interface's) return interface is related through the enum's declaration.
#[test]
fn test_covariant_return_to_an_enum_implementing_the_interface() {
    let out = compile_and_run(
        r#"<?php
namespace T;
interface TokenType { public function label(): string; }
interface Pattern { public function getTokenType(): TokenType; }
enum TokenTypeEnum: string implements TokenType { case KEYWORD = 'keyword'; public function label(): string { return $this->value; } }
final class ApacheCloseTagPattern implements Pattern { public function getTokenType(): TokenTypeEnum { return TokenTypeEnum::KEYWORD; } }
abstract class Base { abstract public function kind(): TokenType; }
final class Child extends Base { public function kind(): TokenTypeEnum { return TokenTypeEnum::KEYWORD; } }
echo (new ApacheCloseTagPattern())->getTokenType()->label(), "|", (new Child())->kind()->label();
"#,
    );
    assert_eq!(out, "keyword|keyword");
}

/// Every enum implicitly implements `UnitEnum` (and a backed one `BackedEnum`), which its
/// declaration does not spell: a class method may still narrow `UnitEnum`/`?UnitEnum`/
/// `BackedEnum` to an enum, which is only built after every class.
#[test]
fn test_covariant_return_to_an_enum_through_its_implicit_enum_interface() {
    let out = compile_and_run(
        r#"<?php
enum Kind: string { case A = 'a'; }
abstract class Backed { abstract public function make(): BackedEnum; }
class BackedKind extends Backed { public function make(): Kind { return Kind::A; } }
abstract class Unit { abstract public function make(): ?UnitEnum; }
class UnitKind extends Unit { public function make(): ?Kind { return Kind::A; } }
echo (new BackedKind())->make()->value, "|", (new UnitKind())->make()->name;
"#,
    );
    assert_eq!(out, "a|A");
}

/// An `iterable` return may be narrowed to a class the name-ordered class-build walk has not
/// reached yet: one implementing `IteratorAggregate` through a user interface, and one that is
/// `Iterator` through its built `ArrayIterator` parent.
#[test]
fn test_covariant_iterable_return_to_a_class_built_later() {
    let out = compile_and_run(
        r#"<?php
interface Items extends IteratorAggregate {}
class ZItems implements Items { public function getIterator(): Iterator { return new ArrayIterator([1, 2]); } }
class ZList extends ArrayIterator {}
class AFactory { public function make(): iterable { return [0]; } }
class BFactory extends AFactory { public function make(): ZItems { return new ZItems(); } }
class CFactory extends AFactory { public function make(): ZList { return new ZList([3]); } }
foreach ((new BFactory())->make() as $v) { echo $v; }
echo "|";
foreach ((new CFactory())->make() as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "12|3");
}
