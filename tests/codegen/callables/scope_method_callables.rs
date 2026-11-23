//! Purpose:
//! End-to-end coverage for callable arrays naming private and protected methods from inside a
//! class: PHP checks and resolves a callable from the calling scope.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expectation is the PHP 8.5 output of the same fixture.
//! - Covers the declared-`callable` entry check, `is_callable()`, `call_user_func()`,
//!   `call_user_func_array()` and invoking the bound value, inside and outside the class.

use crate::support::*;

/// Verifies `[$this, 'private']`, `[$this, 'protected']` and `[self::class, 'privateStatic']` are
/// accepted by a `callable` parameter of a closure and of a method, reported by `is_callable()`
/// and resolved by `call_user_func()` / `call_user_func_array()` inside the class, while
/// `is_callable()` outside the class still says false (issue 2477).
#[test]
fn test_scope_private_and_protected_method_arrays_are_callable_inside_their_class() {
    let out = compile_and_run(
        r#"<?php
class K {
    private function p(int $x): int { return $x + 1; }
    protected function q(int $x): int { return $x + 2; }
    private static function ps(int $x): int { return $x + 3; }
    public function viaMethod(callable $f, int $x): int { return $f($x); }
    public function run(): void {
        $apply = function (callable $f, int $x): int { return $f($x); };
        echo $apply([$this, 'p'], 1), " ", $apply([$this, 'q'], 1), "\n";
        echo $this->viaMethod([$this, 'p'], 10), " ", $this->viaMethod([$this, 'q'], 10), " ",
            $this->viaMethod([self::class, 'ps'], 10), "\n";
        echo var_export(is_callable([$this, 'p']), true), " ", var_export(is_callable([$this, 'q']), true), " ",
            var_export(is_callable([self::class, 'ps']), true), " ", var_export(is_callable([$this, 'nope']), true), "\n";
        echo call_user_func([$this, 'p'], 1), " ", call_user_func([$this, 'q'], 1), " ",
            call_user_func_array([$this, 'p'], [5]), " ", call_user_func([self::class, 'ps'], 1), "\n";
    }
}
(new K())->run();
echo var_export(is_callable([new K(), 'p']), true), " ", var_export(is_callable([new K(), 'q']), true), " ",
    var_export(is_callable(['K', 'ps']), true), "\n";
"#,
    );
    assert_eq!(out, "2 3\n11 12 13\ntrue true true false\n2 3 6 4\nfalse false false\n");
}

/// Verifies the scope rules across a hierarchy: a subclass may call an inherited protected
/// method (instance and static) but not its parent's private one, a parent scope may call its
/// own private method on a child receiver, and a `callable` parameter is checked from the scope
/// of the function that declares it, so a child's private method is refused by a parent's method.
#[test]
fn test_scope_method_callables_follow_inheritance_and_the_checking_scope() {
    let out = compile_and_run(
        r#"<?php
class A {
    private function priv(int $x): int { return $x + 1; }
    protected function prot(int $x): int { return $x + 2; }
    protected static function qs(int $x): int { return $x + 4; }
    public function viaA(callable $f, int $x): int { return $f($x); }
    public function checks(mixed $v): string { return var_export(is_callable($v), true); }
    public function fromA(): void {
        echo var_export(is_callable([$this, 'priv']), true), " ", call_user_func([$this, 'priv'], 1), " ",
            call_user_func([static::class, 'qs'], 1), "\n";
    }
}
class B extends A {
    private function own(int $x): int { return $x * 10; }
    public function run(): void {
        echo var_export(is_callable([$this, 'prot']), true), " ", var_export(is_callable([$this, 'priv']), true), " ",
            var_export(is_callable([self::class, 'qs']), true), "\n";
        echo call_user_func([$this, 'prot'], 1), " ", $this->viaA([$this, 'prot'], 1), " ",
            call_user_func([parent::class, 'qs'], 1), "\n";
        try { echo $this->viaA([$this, 'own'], 2), "\n"; } catch (TypeError $e) { echo "TypeError\n"; }
        echo $this->checks([$this, 'own']), " ", $this->checks([$this, 'priv']), " ", $this->checks([1, 'x']), " ",
            $this->checks([$this]), "\n";
    }
}
(new B())->run();
(new B())->fromA();
"#,
    );
    assert_eq!(out, "true false true\n3 3 5\nTypeError\nfalse true false false\ntrue 2 5\n");
}

/// Verifies a callable array whose method name is only known at run time resolves private,
/// protected and public methods from inside the class through every call form, that a private
/// comparator works as a `usort()` callback, and that a `mixed` callback relayed through
/// `call_user_func_array()` inside the class reaches private and protected methods.
#[test]
fn test_scope_method_callables_with_runtime_method_names() {
    let out = compile_and_run(
        r#"<?php
class A {
    private function priv(int $x): int { return $x + 1; }
    protected function prot(int $x): int { return $x + 2; }
    public function pub(int $x): int { return $x + 3; }
    private function cmp(int $a, int $b): int { return $b <=> $a; }
    public function a(string $m): void {
        $f = [$this, $m];
        echo $f(1), " ", call_user_func($f, 1), " ", call_user_func_array($f, [1]), " ",
            var_export(is_callable($f), true), "\n";
    }
    public function relay(mixed $cb, array $args): mixed { return call_user_func_array($cb, $args); }
    public function relayed(): string { return $this->relay([$this, 'priv'], [1]) . " " . $this->relay([$this, 'prot'], [1]); }
    public function sorted(): string { $xs = [3, 1, 2]; usort($xs, [$this, 'cmp']); return implode(",", $xs); }
}
$a = new A();
foreach (['priv', 'prot', 'pub'] as $m) { $a->a($m); }
echo $a->sorted(), " ", $a->relayed(), "\n";
echo var_export(is_callable([$a, 'pub']), true), " ", var_export(is_callable([$a, 'priv']), true), "\n";
"#,
    );
    assert_eq!(out, "2 2 2 true\n3 3 3 true\n4 4 4 true\n3,2,1 2 3\ntrue false\n");
}

/// Verifies a private method of the calling class wins over a same-named public method of a
/// child receiver, as PHP binds the scope's private declaration, while code outside the class
/// still reaches the child's public method.
#[test]
fn test_scope_private_method_shadows_child_public_method_in_callable_arrays() {
    let out = compile_and_run(
        r#"<?php
class A {
    private function m(int $x): string { return "A::m $x"; }
    public function test(A $o): void {
        echo call_user_func([$o, 'm'], 1), " | ", call_user_func_array([$o, 'm'], [2]), " | ", var_export(is_callable([$o, 'm']), true), "\n";
    }
}
class B extends A { public function m(int $x): string { return "B::m $x"; } }
(new A())->test(new B());
(new A())->test(new A());
function outer(A $o) { echo call_user_func([$o, 'm'], 3), "\n"; }
outer(new B());
"#,
    );
    assert_eq!(out, "A::m 1 | A::m 2 | true\nA::m 1 | A::m 2 | true\nB::m 3\n");
}

/// Verifies the calling class's private static method is callable through a subclass name
/// written as a literal (`['Child', 'p']`, `[Child::class, 'p']`), as it already was through a
/// runtime name, while an unrelated class name and code outside the class still answer false.
#[test]
fn test_scope_private_static_method_through_a_subclass_name() {
    let out = compile_and_run(
        r#"<?php
class A {
    private static function p(int $x): int { return $x + 1; }
    public function t(): void {
        $n = 'B';
        echo var_export(is_callable(['B', 'p']), true), " ", var_export(is_callable([B::class, 'p']), true), " ",
            var_export(is_callable([$n, 'p']), true), " ", call_user_func(['B', 'p'], 1), " ", call_user_func([$n, 'p'], 2), "\n";
        echo var_export(is_callable(['C', 'p']), true), " ", var_export(is_callable(['B', 'nope']), true), " ",
            var_export(is_callable(['D', 'p']), true), "\n";
    }
}
class B extends A {}
class D {}
class C extends A { public static function p(int $x): int { return $x + 10; } }
(new A())->t();
echo var_export(is_callable(['B', 'p']), true), "\n";
"#,
    );
    assert_eq!(out, "true true true 2 3\ntrue false false\nfalse\n");
}
