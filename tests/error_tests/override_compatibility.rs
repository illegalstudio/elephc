//! Purpose:
//! Diagnostics for override and interface-implementation signatures (LSP) that PHP also
//! rejects, plus the PHP-valid shapes elephc refuses instead of miscompiling.
//!
//! Called from:
//! - `cargo test --test error_tests` through Rust's test harness.
//!
//! Key details:
//! - Complements `tests/codegen/oop/override_compatibility.rs`, which covers the accepted shapes.

use super::*;

/// PHP accepts it, but fixed-arity dispatch cannot supply the added default through the parent: refused, not miscompiled.
#[test]
fn test_error_override_adds_optional_parameter() {
    expect_error(
        r#"<?php class Base { public function greet($n) { return $n; } } class Child extends Base { public function greet($n, $s = "!") { return $n . $s; } }"#,
        r#"Cannot add optional parameters when overriding method: Child::greet"#,
    );
}

/// Same refusal against a builtin interface (`json_encode` calls `jsonSerialize()` with no argument).
#[test]
fn test_error_json_serialize_adds_optional_parameter() {
    expect_error(
        r#"<?php class Bag implements JsonSerializable { public function jsonSerialize($depth = 1): mixed { return 1; } }"#,
        r#"Cannot add optional parameters when implementing interface method: Bag::jsonserialize"#,
    );
}

/// PHP rejects making an optional parameter required.
#[test]
fn test_error_override_makes_optional_parameter_required() {
    expect_error(
        r#"<?php class Base { public function f($a = 1) { return $a; } } class Child extends Base { public function f($a) { return $a; } }"#,
        r#"Cannot change optional parameter layout when overriding method: Child::f"#,
    );
}

/// PHP rejects dropping a parameter.
#[test]
fn test_error_override_removes_required_parameter() {
    expect_error(
        r#"<?php class P { public function f($a, $b) { return 1; } } class C extends P { public function f($a) { return 1; } }"#,
        r#"Cannot change parameter count when overriding method: C::f"#,
    );
}

/// PHP rejects a wider (contravariant) return.
#[test]
fn test_error_override_contravariant_return() {
    expect_error(
        r#"<?php class Node {} class Leaf extends Node {} class P { public function f(): Leaf { return new Leaf(); } } class C extends P { public function f(): Node { return new Node(); } }"#,
        r#"Cannot override method C::f with incompatible return type Node (parent returns Leaf)"#,
    );
}

/// Union covariance is member by member: `bool` does not narrow `false`.
#[test]
fn test_error_override_union_return_widens_member() {
    expect_error(
        r#"<?php class Node {} class Leaf extends Node {} class P { public function f(): Node|false { return false; } } class C extends P { public function f(): Leaf|bool { return true; } }"#,
        r#"Cannot override method C::f with incompatible return type Leaf|bool (parent returns Node|false)"#,
    );
}

/// `#[\ReturnTypeWillChange]` only relaxes internal parents, as in PHP.
#[test]
fn test_error_return_type_will_change_on_source_parent() {
    expect_error(
        r#"<?php class P { public function f(): int { return 1; } } class C extends P { #[\ReturnTypeWillChange] public function f(): string { return "x"; } }"#,
        r#"Cannot override method C::f with incompatible return type string (parent returns int)"#,
    );
}

/// A relaxed return that changes the parent's physical return representation is refused.
#[test]
fn test_error_return_type_will_change_changes_class_return_abi() {
    expect_error(
        r#"<?php class X extends ArrayIterator { #[\ReturnTypeWillChange] public function count(): string { return "n"; } }"#,
        r#"Cannot override method X::count: #[\ReturnTypeWillChange] return type string cannot be dispatched through the parent's native return type int"#,
    );
}

/// Interface dispatch only boxes into a `mixed` contract; a scalar contract cannot absorb a string.
#[test]
fn test_error_return_type_will_change_changes_interface_return_abi() {
    expect_error(
        r#"<?php class X implements Countable { #[\ReturnTypeWillChange] public function count(): string { return "n"; } }"#,
        r#"Cannot implement interface method X::count with #[\ReturnTypeWillChange] return type string: elephc cannot dispatch it through the interface's native return type int"#,
    );
}

/// Renaming a variadic is allowed; removing it is not.
#[test]
fn test_error_override_drops_variadic() {
    expect_error(
        r#"<?php class P { public function f(...$a) { return 1; } } class C extends P { public function f($a) { return 1; } }"#,
        r#"Cannot change variadic parameter shape when overriding method: C::f"#,
    );
}

/// PHP rejects `static|false` against `?static`.
#[test]
fn test_error_interface_static_nullable_static_vs_false() {
    expect_error(
        r#"<?php interface Maker { public static function make(string $s): ?static; } class Thing implements Maker { public static function make(string $s): static|false { return false; } }"#,
        r#"Cannot implement interface static method Thing::make with incompatible return type"#,
    );
}

/// `Foo` -> `?Foo = null` would read the parent's object word as a boxed value.
#[test]
fn test_error_override_widens_object_param_to_nullable_default() {
    expect_error(
        r#"<?php class Foo { public $v = "foo"; } class Base { public function f(Foo $x): string { return "B"; } } class Child extends Base { public function f(?Foo $x = null): string { return "C"; } }"#,
        r#"Cannot make parameter $x optional when overriding method: Child::f (not yet supported"#,
    );
}

/// An untyped RTWC override of a `mixed` builtin crashed (SIGSEGV).
#[test]
fn test_error_return_type_will_change_untyped_over_mixed_builtin() {
    expect_error(
        r#"<?php class X extends ArrayIterator { #[\ReturnTypeWillChange] public function current() { return "cur"; } }"#,
        r#"Cannot override method X::current: an untyped #[\ReturnTypeWillChange] override cannot be dispatched through the parent's native return type mixed"#,
    );
}

/// RTWC always requires the parent's return representation, even when the types are compatible.
#[test]
fn test_error_return_type_will_change_typed_over_mixed_builtin() {
    expect_error(
        r#"<?php class X extends ArrayIterator { #[\ReturnTypeWillChange] public function current(): string { return "cur"; } }"#,
        r#"Cannot override method X::current: #[\ReturnTypeWillChange] return type string cannot be dispatched through the parent's native return type mixed"#,
    );
}

/// Calls through the parent passed the parent's default.
#[test]
fn test_error_override_changes_inherited_default() {
    expect_error(
        r#"<?php class A { public function f(int $n, string $u = "a"): string { return "A$n$u"; } } class B extends A { public function f(int $n, string $u = "b"): string { return "B$n$u"; } }"#,
        r#"Cannot change the default value of parameter $u when overriding method: B::f (not yet supported"#,
    );
}

/// Same rule for interface contracts: the interface's default is filled at the call site.
#[test]
fn test_error_interface_impl_changes_inherited_default() {
    expect_error(
        r#"<?php interface I { public function f(int $n = 1): string; } class C implements I { public function f(int $n = 2): string { return "C$n"; } }"#,
        r#"Cannot change the default value of parameter $n when implementing interface method: C::f"#,
    );
}

/// A renamed variadic still has to keep the parent's element type (crashed with exit 139).
#[test]
fn test_error_override_variadic_renamed_with_incompatible_type() {
    expect_error(
        r#"<?php class P { public function f(int ...$a) { return count($a); } } class C extends P { public function f(string ...$b) { return 1; } }"#,
        r#"Cannot widen parameter representation when overriding method: C::f parameter $b"#,
    );
}

/// Variadic parameters are contravariant: a narrower element type is rejected, as in PHP.
#[test]
fn test_error_override_variadic_narrows_object_type() {
    expect_error(
        r#"<?php class Foo {} class Bar extends Foo {} class P { public function f(Foo ...$a) { return 1; } } class C extends P { public function f(Bar ...$b) { return 1; } }"#,
        r#"Cannot narrow variadic parameter $b when overriding method: C::f"#,
    );
}

/// String conversion calls `__toString()` statically, so the implicit `: string` is not applied when a subclass overrides it.
#[test]
fn test_error_untyped_tostring_overridden_by_subclass_keeps_contract_error() {
    expect_error(
        r#"<?php class N implements Stringable { public function __toString() { return "n"; } } class M extends N { public function __toString() { return "m"; } }"#,
        r#"Cannot implement interface method N::__tostring without declaring a compatible return type"#,
    );
}

/// A `mixed` receiver omitting the argument found no candidate and threw a catchable Error (printed `caught`); refused for now.
#[test]
fn test_error_override_makes_required_parameter_optional() {
    expect_error(
        r#"<?php class Base { public function f(int $n, string $u): string { return "B$n$u"; } } class Child extends Base { public function f(int $n, string $u = "kg"): string { return "C$n$u"; } } function u($o) { return $o->f(5); } try { echo u(new Child()); } catch (Throwable $e) { echo "caught"; }"#,
        r#"Cannot make parameter $u optional when overriding method: Child::f (not yet supported"#,
    );
}

/// The same refusal for interface implementations.
#[test]
fn test_error_interface_impl_makes_required_parameter_optional() {
    expect_error(
        r#"<?php interface I { public function f(int $n, string $u); } class C implements I { public function f(int $n, string $u = "kg") { return "C$n$u"; } }"#,
        r#"Cannot make parameter $u optional when implementing interface method: C::f"#,
    );
}

/// The same refusal for static overrides.
#[test]
fn test_error_static_override_makes_required_parameter_optional() {
    expect_error(
        r#"<?php class Base { public static function f(int $n, string $u): string { return "B"; } } class Child extends Base { public static function f(int $n, string $u = "kg"): string { return "C"; } }"#,
        r#"Cannot make parameter $u optional when overriding static method: Child::f"#,
    );
}

/// A by-reference parameter keeps the parent's payload representation too (crashed with SIGSEGV).
#[test]
fn test_error_override_widens_by_reference_parameter() {
    expect_error(
        r#"<?php class Base { public function f(int &$x): string { return "B"; } } class Child extends Base { public function f(int|string &$x): string { return "C"; } }"#,
        r#"Cannot widen parameter representation when overriding method: Child::f parameter $x (the parent passes int, the override expects int|string"#,
    );
}

/// `self::X` spelled the same resolves against different classes, so it is a changed default (printed `Ca` instead of `Cb`).
#[test]
fn test_error_override_class_relative_default_counts_as_changed() {
    expect_error(
        r#"<?php class Base { const X = "a"; public function f(string $u = self::X): string { return "B$u"; } } class Child extends Base { const X = "b"; public function f(string $u = self::X): string { return "C$u"; } }"#,
        r#"Cannot change the default value of parameter $u when overriding method: Child::f"#,
    );
}

/// The override redefines `X`, so `self::X` is a changed default.
#[test]
fn test_error_override_redefined_self_constant_default() {
    expect_error(
        r#"<?php class B { const X = "b"; public function f(string $v = self::X) { return $v; } } class C extends B { const X = "c"; public function f(string $v = self::X) { return "c" . $v; } }"#,
        r#"Cannot change the default value of parameter $v when overriding method: C::f"#,
    );
}

/// Inside an override family the representation is boxed, but PHP's contravariance still applies (it compiled once the family exemption skipped the representation rule).
#[test]
fn test_error_family_override_parameter_still_contravariant() {
    expect_error(
        r#"<?php class P { public function f(int $a) { return 1; } } class C extends P { public function f(string $a) { return 2; } }"#,
        r#"Cannot narrow parameter $a when overriding method: C::f (expects string, parent accepts int)"#,
    );
}

/// The family boxes `string` -> `string|array`, but only re-applies PHP's
/// coercion for plain scalars; the union override would see `12345` as an int.
#[test]
fn test_error_family_param_widened_string_to_union() {
    expect_error(
        r#"<?php class P { public function f(string $x) { var_dump($x); } } class D extends P { public function f(string|array $x) { var_dump($x); } }"#,
        r#"Cannot widen parameter representation when overriding method: D::f parameter $x (the parent passes string, the override expects the union string|array"#,
    );
}

/// `bool` -> `bool|int` (PHP coerces `"a"` to `true`).
#[test]
fn test_error_family_param_widened_bool_to_union() {
    expect_error(
        r#"<?php class P { public function f(bool $x) { var_dump($x); } } class D extends P { public function f(bool|int $x) { var_dump($x); } }"#,
        r#"Cannot widen parameter representation when overriding method: D::f parameter $x"#,
    );
}

/// `int` -> `int|float` (PHP coerces `"3"` to `3`).
#[test]
fn test_error_family_param_widened_int_to_int_float() {
    expect_error(
        r#"<?php class A { public function f(int $x, string $y) { return 1; } } class B extends A { public function f(int|float $x, $y) { return 2; } }"#,
        r#"Cannot widen parameter representation when overriding method: B::f parameter $x"#,
    );
}

/// `float` -> `float|string` (PHP coerces `5` to `5.0`).
#[test]
fn test_error_family_param_widened_float_to_union() {
    expect_error(
        r#"<?php class P { public function f(float $x) { var_dump($x); } } class D extends P { public function f(float|string $x) { var_dump($x); } }"#,
        r#"Cannot widen parameter representation when overriding method: D::f parameter $x"#,
    );
}

/// `int` -> `int|string` read a float argument uncoerced (PHP turns `2.0` into `2`).
#[test]
fn test_error_family_param_widened_int_to_int_string() {
    expect_error(
        r#"<?php class Base { public function f(int $x): string { return "B$x"; } } class Child extends Base { public function f(int|string $x): string { return "C$x"; } }"#,
        r#"Cannot widen parameter representation when overriding method: Child::f parameter $x"#,
    );
}

/// A nullable scalar is a union too (`?string` keeps an int argument uncoerced).
#[test]
fn test_error_family_param_widened_string_to_nullable() {
    expect_error(
        r#"<?php class P { public function f(string $x) { return $x; } } class D extends P { public function f(?string $x) { return $x; } }"#,
        r#"Cannot widen parameter representation when overriding method: D::f parameter $x"#,
    );
}
