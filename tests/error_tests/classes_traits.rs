//! Purpose:
//! Integration or regression tests for diagnostic coverage of class and trait diagnostics, including instanceof parent requires parent class, trait method conflict requires insteadof, and trait property conflict must be compatible.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

/// Late-static class names cannot be evaluated in instance or static property defaults.
#[test]
fn test_error_late_static_class_name_in_property_defaults() {
    for source in [
        "<?php class Probe { public string $name = static::class; }",
        "<?php class Probe { public static string $name = static::class; }",
        "<?php class Probe { public array $names = [static::class]; }",
    ] {
        expect_error(source, "static::class cannot be used for compile-time class name resolution");
    }
}

/// Early-bound class names remain valid in property defaults after late-static validation.
#[test]
fn test_property_defaults_accept_early_bound_class_names() {
    check_source("<?php class Base {} class Probe extends Base { public string $name = self::class; public static string $parent = parent::class; public array $names = [Base::class]; }")
        .expect("early-bound class names are valid constant property defaults");
}

/// Runs the frontend with PDO prelude injection and asserts its first diagnostic.
fn expect_pdo_error(src: &str, expected_substr: &str) {
    let tokens = tokenize(src).expect("PDO diagnostic fixture must tokenize");
    let ast = parse(&tokens).expect("PDO diagnostic fixture must parse");
    let ast = elephc::autoload::collect_aliases(ast).expect("alias collection failed");
    let mut prelude_inventory = elephc::optimize::reachability::PreludeInventory::new();
    let ast = elephc::pdo_prelude::inject_if_used(ast, false, &mut prelude_inventory);
    let ast = elephc::name_resolver::resolve(ast).expect("PDO fixture names must resolve");
    let ast = elephc::optimize::fold_constants(ast);
    let message = types::check(&ast)
        .expect_err("PDO diagnostic fixture must fail")
        .message;
    assert!(
        message.contains(expected_substr),
        "Error '{}' doesn't contain '{}'",
        message,
        expected_substr
    );
}

/// Verifies that `instanceof parent` reports "Class has no parent class" when the class
/// has no parent.
#[test]
fn test_error_instanceof_parent_requires_parent_class() {
    expect_error(
        "<?php class A { public function f(A $x) { return $x instanceof parent; } }",
        "Class has no parent class",
    );
}

/// Verifies that a class using two traits with conflicting method names (both `foo`)
/// reports PHP's collision error when no `insteadof` resolution is provided.
#[test]
fn test_error_trait_method_conflict_requires_insteadof() {
    expect_error(
        r#"<?php
trait A { public function foo() { return 1; } }
trait B { public function foo() { return 2; } }
class C { use A, B; }
"#,
        "Trait method B::foo has not been applied as C::foo, because of collision with A::foo",
    );
}

/// Two traits that reach one trait method are not a collision (see the codegen tests), but the
/// collision rule still compares the method's origin: a trait that overrides the shared method,
/// a copy whose visibility `as` changed, or an alias that meets another trait's method of that
/// name still collides, with php 8.5.12's message.
#[test]
fn test_error_trait_method_collision_compares_origin() {
    expect_error(
        r#"<?php
trait T { public function foo() { return "T"; } }
trait A { use T; public function foo() { return "A"; } }
trait B { use T; }
class C { use A, B; }
"#,
        "Trait method B::foo has not been applied as C::foo, because of collision with A::foo",
    );
    expect_error(
        r#"<?php
trait T { public function foo() { return "T"; } }
trait A { use T { foo as protected; } }
trait B { use T; }
class C { use A, B; }
"#,
        "Trait method B::foo has not been applied as C::foo, because of collision with A::foo",
    );
    expect_error(
        r#"<?php
trait T { public function foo() { return "T"; } }
trait A { use T; }
trait B { use T; }
class C { use A, B { A::foo as protected; } }
"#,
        "Trait method B::foo has not been applied as C::foo, because of collision with A::foo",
    );
    expect_error(
        r#"<?php
trait T { public function foo() { return "T"; } }
trait A { use T { foo as bar; } }
trait B { public function bar() { return "B"; } }
class C { use A, B; }
"#,
        "Trait method B::bar has not been applied as C::bar, because of collision with A::bar",
    );
    expect_error(
        r#"<?php
namespace N;
trait T { public function foo() { return "T"; } }
trait A { public function foo() { return "A"; } }
class C { use T, A; }
"#,
        "Trait method N\\A::foo has not been applied as N\\C::foo, because of collision with N\\T::foo",
    );
}

/// A class that USES a trait still may not re-declare an inherited concrete method abstract:
/// only the trait's own abstract methods are requirements an inherited method satisfies.
#[test]
fn test_error_trait_using_class_cannot_make_inherited_method_abstract() {
    expect_error(
        r#"<?php
class P { public function foo(): int { return 1; } }
trait T { public function bar(): int { return 2; } }
abstract class C extends P { use T; abstract public function foo(): int; }
"#,
        "Cannot make concrete method abstract: C::foo",
    );
}

/// An inherited method satisfies a trait requirement only with a compatible signature: it may
/// not narrow a parameter nor break the declared return type, as PHP reports.
#[test]
fn test_error_inherited_method_must_match_trait_requirement_signature() {
    expect_error(
        r#"<?php
class P { public function foo(string $x): int { return 1; } }
trait T { abstract public function foo(int $x): int; }
final class C extends P { use T; }
"#,
        "Cannot narrow parameter $x when implementing trait method: C::foo",
    );
    expect_error(
        r#"<?php
class P { public static function make(): int { return 1; } }
trait T { abstract public static function make(): string; }
final class C extends P { use T; }
"#,
        "Cannot change return type when implementing trait method: C::make",
    );
}

/// An inherited method returning by value does not satisfy a trait requirement that returns by
/// reference, and an `as` alias of an abstract trait method is checked against the inherited
/// method of the alias name, as PHP reports `Declaration of P::bar() must be compatible`.
#[test]
fn test_error_inherited_method_must_keep_trait_requirement_by_ref_and_alias_signature() {
    expect_error(
        r#"<?php
class P { public function foo(): int { return 1; } }
trait T { abstract public function &foo(): int; }
final class C extends P { use T; }
"#,
        "Cannot remove by-reference return when implementing trait method: C::foo",
    );
    expect_error(
        r#"<?php
class P { public function foo(): int { return 1; } public function bar(): string { return "s"; } }
trait T { abstract public function foo(): int; }
final class C extends P { use T { foo as bar; } }
"#,
        "Cannot change return type when implementing trait method: C::bar",
    );
}

/// `#[\Override]` on a trait-using class still needs a target: a parent method, an interface
/// method, or an abstract method of a used trait.
#[test]
fn test_error_override_in_trait_using_class_requires_a_target() {
    expect_error(
        r#"<?php
class P {}
trait T { public function bar(): int { return 2; } }
final class C extends P { use T; #[\Override] public function baz(): int { return 1; } }
"#,
        "C::baz() has #[\\Override] attribute, but no matching parent method was found",
    );
}

/// An alias of a trait method that the trait implements itself is concrete, even when the
/// trait also uses a trait declaring that method abstract: `#[\Override]` on a class method of
/// the alias name has no target, as in PHP.
#[test]
fn test_error_override_on_alias_of_trait_implemented_requirement() {
    expect_error(
        r#"<?php
trait N { abstract public function foo(): string; }
trait A { use N; public function foo(): string { return "A::foo"; } }
final class C { use A { foo as bar; } #[\Override] public function bar(): string { return "C::bar"; } }
"#,
        "C::bar() has #[\\Override] attribute, but no matching parent method was found",
    );
}

/// Two CONCRETE methods of the same name from separate `use` statements still conflict; only an
/// abstract trait method (a requirement) merges with another declaration.
#[test]
fn test_error_trait_concrete_methods_from_separate_uses_conflict() {
    expect_error(
        r#"<?php
trait A { public function foo() { return 1; } }
trait B { public function foo() { return 2; } }
class C { use A; use B; }
"#,
        "Trait method B::foo has not been applied as C::foo, because of collision with A::foo",
    );
}

/// A concrete trait method satisfies another trait's abstract one only when both are static or
/// both are instance methods, in one `use A, B` as in separate `use` statements, also when the
/// abstract method is a later `as final` copy.
#[test]
fn test_error_trait_requirement_static_kind_mismatch() {
    expect_error(
        r#"<?php
trait A { abstract public function foo(): string; }
trait B { public static function foo(): string { return "s"; } }
final class C { use A, B; }
"#,
        "Cannot change method kind when overriding C::foo",
    );
    expect_error(
        r#"<?php
trait A { abstract public static function foo(): string; }
trait B { public function foo(): string { return "i"; } }
final class C { use A, B; }
"#,
        "Cannot change method kind when overriding C::foo",
    );
    // A later `as final` copy of a kept trait method is not overridden by a method of the
    // other kind: PHP reports the static/instance conflict (#2352).
    let message = check_source(
        r#"<?php
trait T { abstract public function foo(); }
trait A { use T; }
trait B { use T { foo as final; } }
trait S { public static function foo() { return 1; } }
abstract class C { use A, B, S; }
"#,
    )
    .expect_err("static/instance conflict should fail");
    assert!(message.contains("Cannot change method kind when overriding C::foo"), "{message}");
    assert!(!message.contains("Cannot override final method"), "{message}");
    // Two abstract requirements of different kinds conflict on kind before an earlier
    // `as final` one counts as overridden, in either order of kinds.
    for (first, second) in [("", "static "), ("static ", "")] {
        let message = check_source(&format!(
            "<?php\ntrait A {{ abstract public {first}function foo(); }}\ntrait B {{ abstract public {second}function foo(); }}\nabstract class C {{ use A, B {{ A::foo as final; }} }}\n"
        ))
        .expect_err("static/instance conflict should fail");
        assert!(message.contains("Cannot change method kind when overriding C::foo"), "{message}");
        assert!(!message.contains("Cannot override final method"), "{message}");
    }
    // Concrete methods of both kinds still meet a final parent method first.
    for source in [
        "<?php\nclass P { final public function foo() {} }\ntrait S { public static function foo() { return 1; } }\nclass C extends P { use S; public function foo() {} }\n",
        "<?php\nclass P { final public static function foo() {} }\ntrait S { public function foo() { return 1; } }\nclass C extends P { use S; public static function foo() { return 2; } }\n",
    ] {
        let message = check_source(source).expect_err("final parent override should fail");
        assert!(message.contains("Cannot override final method P::foo"), "{message}");
        assert!(!message.contains("Cannot change method kind"), "{message}");
    }
}

/// A trait that uses one trait requiring a method and another implementing it has a concrete
/// method: its users get no requirement, so `#[\Override]` on a class method of that name has
/// no target, whether the wrapper's traits come in one `use` or in two (php 8.5.12).
#[test]
fn test_error_override_on_requirement_satisfied_inside_a_wrapper_trait() {
    expect_error(
        r#"<?php
trait NeedsFoo { abstract public function foo(): string; }
trait GivesFoo { public function foo(): string { return "G"; } }
trait W { use NeedsFoo, GivesFoo; }
class C { use W; #[\Override] public function foo(): string { return "C"; } }
"#,
        "C::foo() has #[\\Override] attribute, but no matching parent method was found",
    );
    expect_error(
        r#"<?php
trait NeedsFoo { abstract public function foo(): string; }
trait GivesFoo { public function foo(): string { return "G"; } }
trait W { use NeedsFoo; use GivesFoo; }
class C { use W; #[\Override] public function foo(): string { return "C"; } }
"#,
        "C::foo() has #[\\Override] attribute, but no matching parent method was found",
    );
}

/// A trait method that satisfies another trait's abstract method, or a second abstract
/// declaration of it, must keep its signature: php 8.5.12 reports `Declaration of ... must be
/// compatible with A::foo(...)` for each case below, so none may merge silently. A concrete
/// implementation gets PHP's compatibility check; two abstract declarations must be identical.
#[test]
fn test_error_trait_requirement_merge_needs_a_compatible_signature() {
    expect_error(
        "<?php trait A { abstract public function foo(int $x): string; } trait B { public function foo(): array { return []; } } class C { use A; use B; }",
        "when implementing trait method: C::foo",
    );
    expect_error(
        "<?php trait A { abstract public function foo(int $x): int; } trait B { public function foo(string $s): string { return $s; } } final class C { use A, B; }",
        "when implementing trait method: C::foo",
    );
    for source in [
        "<?php trait A { abstract public function foo(): int; } trait B { abstract public function foo(): string; } class C { use A; use B; public function foo(): int { return 1; } }",
        "<?php trait A { abstract public function foo(): int; } trait B { abstract public function foo(): string; } abstract class C { use A, B; }",
    ] {
        expect_error(source, "imports abstract trait method 'foo' twice with different signatures");
    }
}

/// The method that satisfies an abstract trait method, whether another trait's or the class's
/// own, must be compatible with it, as php 8.5.12 checks (`Declaration of C::foo(string $x): int
/// must be compatible with X::foo(int $x): string`).
#[test]
fn test_error_trait_requirement_signature_must_be_compatible() {
    expect_error(
        r#"<?php
trait X { abstract public function foo(int $x): string; }
trait Y { public function foo(string $x): int { return 1; } }
class C { use X, Y; }
"#,
        "Cannot narrow parameter $x when implementing trait method: C::foo",
    );
    expect_error(
        r#"<?php
trait X { abstract public function foo(int $x): string; }
trait Y { public function foo(string $x): int { return 1; } }
class C { use Y; use X; }
"#,
        "Cannot narrow parameter $x when implementing trait method: C::foo",
    );
    expect_error(
        r#"<?php
trait X { abstract public function foo(int $a, int $b): string; }
trait Y { public function foo(int $a): string { return "y"; } }
class C { use X, Y; }
"#,
        "Incompatible parameter shape when implementing trait method: C::foo",
    );
    expect_error(
        r#"<?php
trait X { abstract public function foo(): string; }
trait Y { public function foo(): int { return 1; } }
class C { use X; use Y; }
"#,
        "Cannot change return type when implementing trait method: C::foo",
    );
    expect_error(
        r#"<?php
trait X { abstract public function &foo(): array; }
trait Y { public function foo(): array { return []; } }
class C { use X, Y; }
"#,
        "Cannot remove by-reference return when implementing trait method: C::foo",
    );
    expect_error(
        r#"<?php
trait X { abstract public function foo(int $x): string; }
class C { use X; public function foo(string $x): int { return 1; } }
"#,
        "Cannot narrow parameter $x when implementing trait method: C::foo",
    );
    expect_error(
        r#"<?php
trait X { abstract public function foo(): string; }
class C { use X; public function foo() { return "c"; } }
"#,
        "Cannot change return type when implementing trait method: C::foo",
    );    // A by-reference variadic keeps its declared element type for the comparison.
    expect_error(
        r#"<?php
trait X { abstract function foo(int &...$xs): void; }
trait Y { function foo(string &...$xs): void {} }
class C { use X, Y; }
"#,
        "Cannot narrow parameter $xs when implementing trait method: C::foo",
    );
    expect_error(
        r#"<?php
trait X { abstract function foo(int &...$xs): void; }
trait Y { function foo(string &...$xs): void {} }
enum E { use X, Y; case A; }
"#,
        "Cannot narrow parameter $xs when implementing trait method: E::foo",
    );
    expect_error(
        r#"<?php
trait X { abstract function foo(int &...$xs): void; }
class C { use X; function foo(string &...$xs): void {} }
"#,
        "Cannot narrow parameter $xs when implementing trait method: C::foo",
    );
}

/// `void` is not a value type: a `void` method satisfies only a `void` requirement, and a
/// `void` requirement only a `void` or `never` method, for classes and enums alike. php 8.5.12
/// rejects each pair below ("Declaration of C::foo(): void must be compatible with
/// X::foo(): mixed") and accepts `null`/`void`/`never`/untyped requirements as listed.
#[test]
fn test_error_trait_requirement_void_return_must_match() {
    for (required, actual) in [("mixed", "void"), ("?int", "void"), ("void", "?int")] {
        let body = if actual == "void" { "" } else { "return 1;" };
        expect_error(
            &format!(
                "<?php\ntrait X {{ abstract function foo(): {required}; }}\n\
                 trait Y {{ function foo(): {actual} {{ {body} }} }}\nclass C {{ use X, Y; }}\n"
            ),
            "Cannot change return type when implementing trait method: C::foo",
        );
        expect_error(
            &format!(
                "<?php\ntrait X {{ abstract function foo(): {required}; }}\n\
                 trait Y {{ function foo(): {actual} {{ {body} }} }}\nenum E {{ use X, Y; }}\n"
            ),
            "Enum method E::foo has incompatible return type for abstract trait method",
        );
    }
    for (required, actual, body) in [
        (": ?int", ": null", "return null;"),
        (": mixed", ": null", "return null;"),
        (": void", ": void", ""),
        (": void", ": never", "throw new Exception();"),
        ("", ": void", ""),
    ] {
        expect_no_error(&format!(
            "<?php\ntrait X {{ abstract function foo(){required}; }}\n\
             trait Y {{ function foo(){actual} {{ {body} }} }}\nclass C {{ use X, Y; }}\n"
        ));
    }
}

/// Two instantiations of one generic trait reached along two `use` paths are two traits, so
/// their methods collide even when only a body mentions the type parameter, while one
/// instantiation reached twice is one method (`tests/codegen/oop/traits.rs`).
#[test]
fn test_error_generic_trait_instantiations_reached_twice_collide() {
    expect_error(
        r#"<?php
class P { public function n(): string { return "P"; } }
class Q { public function n(): string { return "Q"; } }
trait Make<T> { public function make(): string { $o = new T(); return $o->n(); } }
trait A { use Make<P>; }
trait B { use Make<Q>; }
class C { use A, B; }
"#,
        "Trait method B::make has not been applied as C::make, because of collision with A::make",
    );
}

/// A trait's own method, or another used trait's, that satisfies a nested requirement must be
/// compatible with it, as php 8.5.12 reports `Declaration of A::foo(): string must be compatible
/// with N::foo(): int`, even though the requirement no longer reaches the class.
#[test]
fn test_error_requirement_satisfied_inside_a_trait_must_be_compatible() {
    expect_error(
        r#"<?php
trait N { abstract public function foo(): int; }
trait A { use N; public function foo(): string { return "a"; } }
class C { use A; }
"#,
        "Cannot change return type when implementing trait method: C::foo",
    );
    expect_error(
        r#"<?php
trait N { abstract public function foo(): int; }
trait G { public function foo(): string { return "g"; } }
trait W { use N, G; }
class C { use W; }
"#,
        "Cannot change return type when implementing trait method: C::foo",
    );
}

/// An alias of a method excluded by `insteadof` still satisfies a requirement of the trait that
/// takes it, so it must be compatible with it, as php 8.5.12 reports
/// `Declaration of W::bar(): string must be compatible with N::bar(): int`.
#[test]
fn test_error_alias_of_excluded_method_must_satisfy_requirement() {
    expect_error(
        r#"<?php
trait N { abstract public function bar(): int; }
trait P { public function foo(): int { return 1; } }
trait Q { public function foo(): string { return "q"; } }
trait W { use N, P, Q { P::foo insteadof Q; Q::foo as bar; } }
class C { use W; }
"#,
        "Cannot change return type when implementing trait method: C::bar",
    );
}

/// Two instantiations of one generic trait are two traits whose methods share the template's
/// declaration: `Needs<int>::foo` and `Needs<string>::foo` are two different abstract
/// declarations, not one requirement, so the program is rejected (the requirement walk also keys
/// its copies by the instantiated trait).
#[test]
fn test_error_each_generic_trait_instantiation_requirement_is_checked() {
    expect_error(
        r#"<?php
trait Needs<T> { abstract public function foo(): T; }
class C { use Needs<int>, Needs<string>; public function foo(): int { return 1; } }
"#,
        "class C imports abstract trait method 'foo' twice with different signatures",
    );
}

/// A requirement a trait satisfies that names `self` is checked for every class using the
/// trait, so the outcome does not depend on which class is declared first: with
/// `N::f(): self` and `W::f(): Base`, the class `Other` breaks it in either order. (php 8.5.12
/// checks the pair once at `W`, with `self` naming `W`, and rejects even `Base` alone.)
#[test]
fn test_error_trait_satisfied_self_requirement_is_checked_for_every_user() {
    let traits = "trait N { abstract public function f(): self; } \
                  trait W { use N; public function f(): Base { return new Base(); } }";
    for classes in [
        "class Base { use W; } class Other { use W; }",
        "class Other { use W; } class Base { use W; }",
    ] {
        expect_error(
            &format!("<?php {traits} {classes}"),
            "Cannot change return type when implementing trait method: Other::f",
        );
    }
}

/// Verifies that a class using two traits with a property of the same name but
/// incompatible visibility reports "incompatible duplicate property".
#[test]
fn test_error_trait_property_conflict_must_be_compatible() {
    expect_error(
        r#"<?php
trait A { public $value = 1; }
trait B { private $value = 1; }
class C { use A, B; }
"#,
        "incompatible duplicate property",
    );
}

/// Verifies that circular trait composition (trait A uses B, B uses A) is detected
/// and reported as an error.
#[test]
fn test_error_circular_trait_composition() {
    expect_error(
        r#"<?php
trait A { use B; }
trait B { use A; }
class C { use A; }
"#,
        "Circular trait composition detected",
    );
}

/// Verifies that accessing a protected property from outside the class hierarchy
/// reports "Cannot access protected property: Secret::value".
#[test]
fn test_error_cannot_access_protected_property_outside_class() {
    expect_error(
        r#"<?php
class Secret {
    protected $value = 7;
}
$s = new Secret();
echo $s->value;
"#,
        "Cannot access protected property: Secret::value",
    );
}

/// Verifies that declaring two classes differing only by case (Box vs box) reports
/// "Duplicate class declaration: box".
#[test]
fn test_error_duplicate_classes_differing_only_by_case() {
    expect_error(
        "<?php class Box {} class box {}",
        "Duplicate class declaration: box",
    );
}

/// Verifies that declaring two interfaces differing only by case reports
/// "Duplicate interface declaration: named".
#[test]
fn test_error_duplicate_interfaces_differing_only_by_case() {
    expect_error(
        "<?php interface Named {} interface named {}",
        "Duplicate interface declaration: named",
    );
}

/// Verifies that declaring two traits differing only by case reports
/// "Duplicate trait declaration: reusable".
#[test]
fn test_error_duplicate_traits_differing_only_by_case() {
    expect_error(
        "<?php trait Reusable {} trait reusable {}",
        "Duplicate trait declaration: reusable",
    );
}

/// Verifies that declaring two enums differing only by case reports
/// "Duplicate class or enum declaration: mode".
#[test]
fn test_error_duplicate_enums_differing_only_by_case() {
    expect_error(
        "<?php enum Mode { case A; } enum mode { case B; }",
        "Duplicate class or enum declaration: mode",
    );
}

/// Verifies that a class with two methods differing only by case (Save vs save) reports
/// "Duplicate method declaration in Box: save".
#[test]
fn test_error_duplicate_methods_differing_only_by_case() {
    expect_error(
        "<?php class Box { public function Save() { return 1; } public function save() { return 2; } }",
        "Duplicate method declaration in Box: save",
    );
}

/// Verifies that calling `parent::boot()` inside a class with no parent reports
/// "Class Solo has no parent class".
#[test]
fn test_error_parent_without_parent_class() {
    expect_error(
        "<?php class Solo { public function boot() { return parent::boot(); } } $s = new Solo(); $s->boot();",
        "Class Solo has no parent class",
    );
}

/// Verifies that a subclass cannot override a final trait method and reports
/// "Cannot override final method Base::run".
#[test]
fn test_error_trait_final_method_cannot_be_overridden_by_subclass() {
    expect_error(
        "<?php trait T { final public function run() { return 1; } } class Base { use T; } class Child extends Base { public function run() { return 2; } }",
        "Cannot override final method Base::run",
    );
}

/// Verifies that a subclass cannot override a final trait property and reports
/// "Cannot override final property Base::$value".
#[test]
fn test_error_trait_final_property_cannot_be_overridden_by_subclass() {
    expect_error(
        "<?php trait T { final public $value; } class Base { use T; } class Child extends Base { public $value; }",
        "Cannot override final property Base::$value",
    );
}

/// Verifies that a member modifier right after `as` is read as the alias modifier, never as an
/// alias name: `foo as static;` and `foo as abstract;` get PHP's trait alias errors and
/// `foo as readonly;` its method modifier error, while `foo as protected static;` (a word after
/// the modifier) is a valid alias.
#[test]
fn test_error_trait_alias_modifier_without_visibility_is_not_an_alias() {
    for (modifier, message) in [
        ("static", "Cannot use \"static\" as method modifier in trait alias"),
        ("static bar", "Cannot use \"static\" as method modifier in trait alias"),
        ("abstract", "Cannot use \"abstract\" as method modifier in trait alias"),
        ("readonly", "Cannot use the readonly modifier on a method"),
    ] {
        expect_error(
            &format!(
                "<?php\ntrait T {{ public function foo() {{ return 1; }} }}\nclass C {{ use T {{ foo as {modifier}; }} }}\n"
            ),
            message,
        );
    }
}

/// Verifies that a `Trait::` method reference needs a word after `::`: a variable
/// (`T::$foo as bar;`) is rejected, as in PHP.
#[test]
fn test_error_trait_adaptation_scoped_target_requires_method_name() {
    expect_error(
        r#"<?php
trait T { public function foo() { return 1; } }
class C { use T { T::$foo as bar; } }
"#,
        "Expected method name after 'TraitName::' in adaptation",
    );
}

/// Verifies that `self::class` outside a class context reports
/// "Cannot use self::class or static::class outside a class context".
#[test]
fn test_error_self_class_outside_class() {
    expect_error(
        "<?php echo self::class;",
        "Cannot use self::class or static::class outside a class context",
    );
}

/// Verifies that `parent::class` inside a class with no parent reports
/// "Class 'C' has no parent class".
#[test]
fn test_error_parent_class_without_parent() {
    expect_error(
        "<?php class C { public static function name() { return parent::class; } }",
        "Class 'C' has no parent class",
    );
}

/// Verifies that using `static::` in a class constant expression reports
/// PHP's compile-time constant diagnostic.
#[test]
fn test_error_static_constant_reference_in_class_constant_expression() {
    expect_error(
        "<?php class C { const A = 1; const B = static::A + 1; } echo C::B;",
        "\"static::\" is not allowed in compile-time constants",
    );
}

/// Verifies a typed class constant rejects an initializer outside its declared type.
#[test]
fn test_error_typed_class_constant_initializer_mismatch() {
    expect_error(
        "<?php class C { public const int VALUE = 'wrong'; }",
        "Cannot use string as value for class constant C::VALUE of type int",
    );
}

/// Verifies PHP-forbidden callable class-constant types are rejected.
#[test]
fn test_error_typed_class_constant_forbids_callable() {
    expect_error(
        "<?php class C { public const callable VALUE = null; }",
        "Class constant C::VALUE cannot have type callable",
    );
}

/// Verifies PHP-forbidden `void` and `never` class-constant types are rejected.
#[test]
fn test_error_typed_class_constant_forbids_void_and_never() {
    expect_error(
        "<?php class C { public const void VALUE = null; }",
        "Class constant C::VALUE cannot have type void",
    );
    expect_error(
        "<?php class C { public const never VALUE = null; }",
        "Class constant C::VALUE cannot have type never",
    );
}

/// Verifies a child class cannot widen an inherited typed constant contract.
#[test]
fn test_error_typed_class_constant_override_must_be_covariant() {
    expect_error(
        "<?php class Base { public const int VALUE = 1; } class Child extends Base { public const int|string VALUE = 2; }",
        "Type of Child::VALUE must be compatible with Base::VALUE of type int",
    );
}

/// Verifies a class constant must preserve the type required by an implemented interface.
#[test]
fn test_error_typed_interface_constant_implementation_must_be_compatible() {
    expect_error(
        "<?php interface Contract { public const int VALUE = 1; } class Impl implements Contract { public const string VALUE = 'wrong'; }",
        "Type of Impl::VALUE must be compatible with Contract::VALUE of type int",
    );
}

/// Verifies that `new static()` on a child with a required constructor parameter
/// reports a missing argument error.
#[test]
fn test_error_new_static_validates_child_constructor() {
    expect_error(
        "<?php class Base { public static function make(): Base { return new static(); } } class Child extends Base { public function __construct(string $name) {} } echo Child::make();",
        "Constructor 'Child::__construct' expects 1 arguments, got 0",
    );
}

/// Verifies the builtin `DatePeriod` constructor enforces its 3-to-4 argument arity.
#[test]
fn test_error_date_period_too_few_args() {
    expect_error(
        "<?php $p = new DatePeriod(new DateTime(\"2024-01-01\"));",
        "Constructor 'DatePeriod::__construct' expects 3 to 4 arguments, got 1",
    );
}

/// Verifies the builtin `DateTime` constructor rejects more than its 0-to-2 arguments.
#[test]
fn test_error_datetime_too_many_args() {
    expect_error(
        "<?php $d = new DateTime(\"now\", null, 3);",
        "Constructor 'DateTime::__construct' expects 0 to 2 arguments, got 3",
    );
}

/// Verifies the builtin `DateTimeImmutable` constructor rejects more than its 0-to-2 arguments.
#[test]
fn test_error_datetime_immutable_too_many_args() {
    expect_error(
        "<?php $d = new DateTimeImmutable(\"now\", null, 3);",
        "Constructor 'DateTimeImmutable::__construct' expects 0 to 2 arguments, got 3",
    );
}

/// Verifies `setMicrosecond()`, which php-src declares `static` outright (not tentatively), still
/// rejects an override that narrows the return to the concrete class, as PHP's fatal does.
#[test]
fn test_error_datetime_immutable_set_microsecond_override_must_return_static() {
    expect_error(
        "<?php class Bad extends DateTimeImmutable { public function setMicrosecond(int $microsecond): DateTimeImmutable { return parent::setMicrosecond($microsecond); } } echo 1;",
        "Cannot override method Bad::setMicrosecond with incompatible return type",
    );
}

/// Verifies the builtin `DateInterval` constructor requires its single duration-string argument.
#[test]
fn test_error_date_interval_too_few_args() {
    expect_error(
        "<?php $i = new DateInterval();",
        "Constructor 'DateInterval::__construct' expects 1 arguments, got 0",
    );
}

// --- #[\Override] enforcement (PHP 8.3) ---

/// Verifies that `#[Override]` on a method with no matching parent method reports
/// "no matching parent method".
#[test]
fn test_error_override_attribute_with_no_parent_method() {
    expect_error(
        "<?php class Base {} class Child extends Base { #[\\Override] public function nope(): void {} }",
        "no matching parent method",
    );
}

/// Verifies that `#[Override]` on a root class (with no parent) reports
/// "no matching parent method".
#[test]
fn test_error_override_attribute_on_root_class() {
    expect_error(
        "<?php class Solo { #[\\Override] public function alone(): void {} }",
        "no matching parent method",
    );
}

/// Verifies that `#[Override]` on a misspelled method name reports "no matching parent
/// method" rather than silently allowing the typo.
#[test]
fn test_error_override_attribute_on_misspelled_method() {
    expect_error(
        "<?php class Base { public function fetchAll(): void {} } class Child extends Base { #[\\Override] public function fetchAl(): void {} }",
        "no matching parent method",
    );
}

/// Verifies that the unqualified `#[Override]` form (without a leading backslash) is
/// recognized as the PHP 8.3 built-in and enforces parent-method matching.
#[test]
fn test_error_override_attribute_unqualified_form_is_recognized() {
    expect_error(
        "<?php class Base {} class Child extends Base { #[Override] public function nope(): void {} }",
        "no matching parent method",
    );
}

/// Verifies that `#[Override]` imported under an alias (e.g., `use Override as
/// MustOverride`) is still recognized as the built-in and enforces parent-method matching.
#[test]
fn test_error_override_attribute_import_alias_is_recognized() {
    expect_error(
        "<?php use Override as MustOverride; class Base {} class Child extends Base { #[MustOverride] public function nope(): void {} }",
        "no matching parent method",
    );
}

/// Verifies that a namespaced user attribute that looks like `Foo\Override` is NOT
/// treated as the PHP 8.3 built-in `#[Override]` and therefore does not enforce
/// parent-method matching.
#[test]
fn test_override_attribute_qualified_lookalike_is_not_builtin() {
    check_source("<?php class Solo { #[Foo\\Override] public function alone(): void {} }")
        .expect("qualified user attribute should not enforce #[\\Override]");
}

/// Verifies that a namespaced `#[Override]` attribute is NOT treated as the PHP 8.3
/// built-in when the `Override` class does not resolve to the built-in, so no
/// parent-method enforcement occurs.
#[test]
fn test_override_attribute_namespaced_unqualified_lookalike_is_not_builtin() {
    check_source("<?php namespace N; class Solo { #[Override] public function alone(): void {} }")
        .expect("namespaced user attribute should not enforce #[\\Override]");
}

/// Verifies that `#[Override]` on a static method with no matching parent static method
/// reports "no matching parent method".
#[test]
fn test_error_override_attribute_on_static_with_no_parent() {
    expect_error(
        "<?php class Base {} class Child extends Base { #[\\Override] public static function gone(): void {} }",
        "no matching parent method",
    );
}

/// Verifies that `#[AllowDynamicProperties]` inside a namespace stays a user-defined attribute
/// (not the built-in) and therefore does not suppress PHP 8.5's creation deprecation.
///
/// The write itself is legal on an ordinary class; `tests/codegen/objects/property_access/`
/// pins the emitted `Creation of dynamic property N\Bag::$x is deprecated` notice.
#[test]
fn test_allow_dynamic_properties_namespaced_lookalike_does_not_grant_the_exemption() {
    expect_no_error(
        "<?php namespace N; #[AllowDynamicProperties] class Bag {} $b = new Bag(); $b->x = 1;",
    );
}

// --- class_attribute_names() argument validation ---

/// Verifies that `class_attribute_names()` with an undefined class reports
/// "undefined class 'DoesNotExist'".
#[test]
fn test_error_class_attribute_names_undefined_class() {
    expect_error(
        "<?php $x = class_attribute_names('DoesNotExist');",
        "undefined class 'DoesNotExist'",
    );
}

/// Verifies that `class_attribute_names()` with a dynamic (variable) argument instead
/// of a string literal reports "requires a string literal class name".
#[test]
fn test_error_class_attribute_names_dynamic_argument() {
    expect_error(
        "<?php $name = 'Foo'; class_attribute_names($name);",
        "requires a string literal class name",
    );
}

/// Verifies that `class_attribute_names()` with no argument reports "exactly 1
/// argument".
#[test]
fn test_error_class_attribute_names_no_argument() {
    expect_error("<?php class_attribute_names();", "exactly 1 argument");
}

/// Verifies that `class_attribute_names()` with a non-string argument (e.g., integer)
/// reports "must be a string class name".
#[test]
fn test_error_class_attribute_names_non_string_argument() {
    expect_error(
        "<?php class_attribute_names(42);",
        "must be a string class name",
    );
}

// --- class_attribute_args() argument validation ---

/// Verifies that `class_attribute_args()` with an undefined class reports
/// "undefined class 'DoesNotExist'".
#[test]
fn test_error_class_attribute_args_undefined_class() {
    expect_error(
        "<?php $x = class_attribute_args('DoesNotExist', 'Foo');",
        "undefined class 'DoesNotExist'",
    );
}

/// Verifies that `class_attribute_args()` with a dynamic class name argument reports
/// "requires a string literal class name".
#[test]
fn test_error_class_attribute_args_dynamic_class_argument() {
    expect_error(
        "<?php $name = 'Foo'; class_attribute_args($name, 'Bar');",
        "requires a string literal class name",
    );
}

/// Verifies that `class_attribute_args()` with a dynamic attribute name argument reports
/// "requires a string literal attribute name".
#[test]
fn test_error_class_attribute_args_dynamic_attr_argument() {
    expect_error(
        "<?php #[Foo] class C {} $name = 'Foo'; class_attribute_args('C', $name);",
        "requires a string literal attribute name",
    );
}

/// Verifies that `class_attribute_args()` called with only one argument (instead of
/// two) reports "exactly 2 arguments".
#[test]
fn test_error_class_attribute_args_wrong_arity() {
    expect_error("<?php class_attribute_args('Foo');", "exactly 2 arguments");
}

/// Verifies that `class_attribute_args()` with a non-string first argument reports
/// "first argument must be a string class name".
#[test]
fn test_error_class_attribute_args_non_string_class() {
    expect_error(
        "<?php class_attribute_args(1, 'Foo');",
        "first argument must be a string class name",
    );
}

/// Verifies that `class_attribute_args()` with a non-string second argument reports
/// "second argument must be a string attribute name".
#[test]
fn test_error_class_attribute_args_non_string_attr() {
    expect_error(
        "<?php #[Foo] class C {} class_attribute_args('C', 1);",
        "second argument must be a string attribute name",
    );
}

/// Verifies that `class_attribute_args()` on an attribute with an unmaterialized
/// symbolic constant argument reports "requested attribute uses argument metadata
/// that is not supported yet".
#[test]
fn test_error_class_attribute_const_args_are_not_silently_dropped() {
    expect_error(
        "<?php #[Attribute(Attribute::TARGET_CLASS)] class MyAttr {} class_attribute_args('MyAttr', 'Attribute');",
        "requested attribute uses argument metadata that is not supported yet",
    );
}

/// Verifies that `class_attribute_args()` on an attribute with expression arguments
/// (e.g., `1 + 2`) reports "requested attribute uses argument metadata that is not
/// supported yet".
#[test]
fn test_error_class_attribute_expression_args_are_not_silently_dropped() {
    expect_error(
        "<?php #[Foo(1 + 2)] class C {} class_attribute_args('C', 'Foo');",
        "requested attribute uses argument metadata that is not supported yet",
    );
}

/// Verifies that `class_get_attributes()` on a class with a still-unsupported
/// (non-foldable arithmetic) attribute argument reports "class has attribute
/// argument metadata that is not supported yet" rather than silently dropping it.
/// Float arguments are now supported, so this guards a genuinely unsupported shape.
#[test]
fn test_error_class_get_attributes_unsupported_arg_not_silently_dropped() {
    expect_error(
        "<?php #[Foo(1 + 2)] class C {} class_get_attributes('C');",
        "class has attribute argument metadata that is not supported yet",
    );
}

// --- class_get_attributes() argument validation ---

/// Verifies that `class_get_attributes()` with an undefined class reports
/// "undefined class 'DoesNotExist'".
#[test]
fn test_error_class_get_attributes_undefined_class() {
    expect_error(
        "<?php $x = class_get_attributes('DoesNotExist');",
        "undefined class 'DoesNotExist'",
    );
}

/// Verifies that `class_get_attributes()` with a dynamic (variable) argument reports
/// "requires a string literal class name".
#[test]
fn test_error_class_get_attributes_dynamic_argument() {
    expect_error(
        "<?php $name = 'Foo'; class_get_attributes($name);",
        "requires a string literal class name",
    );
}

/// Verifies that `class_get_attributes()` with no argument reports "exactly 1
/// argument".
#[test]
fn test_error_class_get_attributes_no_argument() {
    expect_error("<?php class_get_attributes();", "exactly 1 argument");
}

/// Verifies that `class_get_attributes()` with a non-string argument reports
/// "must be a string class name".
#[test]
fn test_error_class_get_attributes_non_string_argument() {
    expect_error(
        "<?php class_get_attributes(42);",
        "must be a string class name",
    );
}

/// Verifies that declaring a class named `ReflectionAttribute` reports
/// "Cannot redeclare built-in reflection type: ReflectionAttribute".
#[test]
fn test_error_reflection_attribute_redeclaration() {
    expect_error(
        "<?php class ReflectionAttribute {}",
        "Cannot redeclare built-in reflection type: ReflectionAttribute",
    );
}

/// Verifies that declaring an interface named `ReflectionAttribute` reports
/// "Cannot redeclare built-in reflection type: ReflectionAttribute".
#[test]
fn test_error_reflection_attribute_interface_redeclaration() {
    expect_error(
        "<?php interface ReflectionAttribute {}",
        "Cannot redeclare built-in reflection type: ReflectionAttribute",
    );
}

/// Verifies that declaring a trait named `ReflectionAttribute` reports
/// "Cannot redeclare built-in reflection type: ReflectionAttribute".
#[test]
fn test_error_reflection_attribute_trait_redeclaration() {
    expect_error(
        "<?php trait ReflectionAttribute {}",
        "Cannot redeclare built-in reflection type: ReflectionAttribute",
    );
}

/// Verifies that `new ReflectionAttribute()` reports "Cannot access private
/// constructor: ReflectionAttribute::__construct".
#[test]
fn test_error_reflection_attribute_constructor_is_private() {
    expect_error(
        "<?php $r = new ReflectionAttribute();",
        "Cannot access private constructor: ReflectionAttribute::__construct",
    );
}

/// Verifies instantiating a descendant of a class with a private constructor is rejected, and
/// that the diagnostic names the class that DECLARES the constructor, as php does.
#[test]
fn test_error_private_constructor_blocks_descendant_from_global_scope() {
    expect_error(
        "<?php class Owner { private function __construct() {} } class Child extends Owner {} $o = new Child();",
        "Cannot access private constructor: Owner::__construct",
    );
}

/// Verifies `new static()` from a scope that does not declare the private constructor is rejected.
#[test]
fn test_error_private_constructor_blocks_new_static_from_other_scope() {
    expect_error(
        "<?php class Owner { private function __construct() {} } class Child extends Owner { public static function make(): static { return new static(); } } Child::make();",
        "Cannot access private constructor: Owner::__construct",
    );
}

/// Verifies the declaring class is reported from further up the chain, not the lexical scope.
#[test]
fn test_error_private_grandparent_constructor_names_its_declaring_class() {
    expect_error(
        "<?php class Root { private function __construct() {} } class Middle extends Root { public static function make(): static { return new static(); } } class Leaf extends Middle {} Leaf::make();",
        "Cannot access private constructor: Root::__construct",
    );
}

/// Issue #870: the ARITY diagnostic names the declaring class too, not the one being
/// instantiated. php-src reports `Too few arguments to function Owner::__construct()` for a
/// descendant of a class with a private constructor; elephc said `Child::__construct`, so the
/// message pointed at a class whose source has no constructor to look at.
#[test]
fn test_error_inherited_private_constructor_arity_names_the_declaring_class() {
    expect_error(
        "<?php class Owner { private function __construct(int $n) {} public static function makeChild(): Owner { return new Child(); } } class Child extends Owner {} Owner::makeChild();",
        "Constructor 'Owner::__construct' expects 1 arguments, got 0",
    );
}

/// The same for TOO MANY arguments, which reaches the arity check by the other side.
#[test]
fn test_error_inherited_private_constructor_excess_arity_names_the_declaring_class() {
    expect_error(
        "<?php class Owner { private function __construct(int $n) {} public static function makeChild(): Owner { return new Child(1, 2, 3); } } class Child extends Owner {} Owner::makeChild();",
        "Constructor 'Owner::__construct' expects 1 arguments, got 3",
    );
}

/// A NAMED argument that matches no parameter is normalized against the same signature, so its
/// diagnostic must name the declaring class as well.
#[test]
fn test_error_inherited_private_constructor_unknown_named_arg_names_the_declaring_class() {
    expect_error(
        "<?php class Owner { private function __construct(int $n) {} public static function makeChild(): Owner { return new Child(nope: 1); } } class Child extends Owner {} Owner::makeChild();",
        "Constructor 'Owner::__construct'",
    );
}

/// The declaring class is reported from further up the chain here too, matching the visibility
/// half's `test_error_private_grandparent_constructor_names_its_declaring_class`.
#[test]
fn test_error_inherited_private_grandparent_constructor_arity_names_its_declaring_class() {
    expect_error(
        "<?php class Root { private function __construct(int $n) {} public static function makeLeaf(): Root { return new Leaf(); } } class Middle extends Root {} class Leaf extends Middle {} Root::makeLeaf();",
        "Constructor 'Root::__construct' expects 1 arguments, got 0",
    );
}

/// A class that declares its OWN constructor is unchanged: the declaring class and the
/// instantiated class are the same, so the message reads exactly as before.
#[test]
fn test_error_own_constructor_arity_still_names_the_instantiated_class() {
    expect_error(
        "<?php class Owner { public function __construct(int $n) {} } $o = new Owner();",
        "Constructor 'Owner::__construct' expects 1 arguments, got 0",
    );
}

/// An inherited PUBLIC constructor names the class that DECLARES it, which is what php-src
/// reports (`Too few arguments to function Base::__construct()`).
///
/// This one CHANGES with the fix, and deliberately so: a public constructor IS in the
/// descendant's own method map, so the owner walk stops at the descendant — but the declaring
/// class it then reports is still the ancestor. Measured before the fix, this same program said
/// `Constructor 'Sub::__construct'`, naming a class whose source has no constructor. The
/// message moves for every INHERITED constructor, not only a private one.
#[test]
fn test_error_inherited_public_constructor_arity_names_the_declaring_class() {
    expect_error(
        "<?php class Base { public function __construct(int $n) {} } class Sub extends Base {} $o = new Sub();",
        "Constructor 'Base::__construct' expects 1 arguments, got 0",
    );
}

/// Verifies that `new ReflectionParameter()` rejects unknown parameter names.
#[test]
fn test_error_reflection_parameter_constructor_unknown_name() {
    expect_error(
        "<?php class C { public function f($a) {} } $r = new ReflectionParameter([C::class, 'f'], 'b');",
        "parameter specified by name could not be found",
    );
}

/// Verifies that `new ReflectionParameter()` rejects unknown function targets.
#[test]
fn test_error_reflection_parameter_constructor_unknown_function() {
    expect_error(
        "<?php $r = new ReflectionParameter('missing_reflect_function', 'a');",
        "Function missing_reflect_function() does not exist",
    );
}

/// Verifies that `new ReflectionParameter()` rejects dynamic function names
/// because runtime function reflection lookup metadata is not available.
#[test]
fn test_error_reflection_parameter_constructor_dynamic_function_name() {
    expect_error(
        "<?php function reflected_function($a) {} $f = 'reflected_function'; $r = new ReflectionParameter($f, 'a');",
        "requires a string literal function name",
    );
}

/// Verifies that `new ReflectionParameter()` still rejects dynamic method names
/// because runtime reflection lookup metadata is not available.
#[test]
fn test_error_reflection_parameter_constructor_dynamic_method_name() {
    expect_error(
        "<?php class C { public function f($a) {} } $m = 'f'; $r = new ReflectionParameter([C::class, $m], 'a');",
        "requires a string literal method name",
    );
}

/// Verifies that `new ReflectionFunction()` rejects unknown function targets.
#[test]
fn test_error_reflection_function_constructor_unknown_function() {
    expect_error(
        "<?php $r = new ReflectionFunction('missing_reflection_function');",
        "Function missing_reflection_function() does not exist",
    );
}

/// Verifies that `new ReflectionFunction()` rejects dynamic function names
/// because runtime function reflection lookup metadata is not available.
#[test]
fn test_error_reflection_function_constructor_dynamic_function_name() {
    expect_error(
        "<?php function reflected_function($a) {} $f = 'reflected_function'; $r = new ReflectionFunction($f);",
        "requires a string literal function name",
    );
}

/// Verifies `ReflectionFunction` rejects attributes whose arguments cannot yet
/// be materialized into `ReflectionAttribute` metadata.
#[test]
fn test_error_reflection_function_get_attributes_unsupported_arg_metadata() {
    expect_error(
        "<?php $name = 'x'; #[FuncAttr($name)] function reflected_function_attr() {} $r = new ReflectionFunction('reflected_function_attr');",
        "function has attribute argument metadata that is not supported yet",
    );
}

/// Verifies that accessing `ReflectionAttribute::__name` property reports
/// "Cannot access private property: ReflectionAttribute::__name".
#[test]
fn test_error_reflection_attribute_internal_properties_are_private() {
    expect_error(
        "<?php #[A] class C {} $attrs = class_get_attributes('C'); echo $attrs[0]->__name;",
        "Cannot access private property: ReflectionAttribute::__name",
    );
}

/// Verifies that declaring a class named `ReflectionClass` reports
/// "Cannot redeclare built-in reflection type: ReflectionClass".
#[test]
fn test_error_reflection_class_redeclaration() {
    expect_error(
        "<?php class ReflectionClass {}",
        "Cannot redeclare built-in reflection type: ReflectionClass",
    );
}

/// Verifies that `new ReflectionClass('Missing')` reports "undefined class 'Missing'".
#[test]
fn test_error_reflection_class_undefined_class() {
    expect_error(
        "<?php $r = new ReflectionClass('Missing');",
        "ReflectionClass::__construct(): undefined class 'Missing'",
    );
}

/// Verifies that `new ReflectionClass($name)` with a dynamic variable reports
/// "requires a string literal class name".
#[test]
fn test_error_reflection_class_dynamic_argument() {
    expect_error(
        "<?php $name = 'C'; class C {} $r = new ReflectionClass($name);",
        "requires a string literal class name",
    );
}

/// Verifies that `new ReflectionMethod('C', 'missing')` on an undefined method reports
/// "undefined method 'C::missing'".
#[test]
fn test_error_reflection_method_undefined_method() {
    expect_error(
        "<?php class C {} $r = new ReflectionMethod('C', 'missing');",
        "undefined method 'C::missing'",
    );
}

/// Verifies that `new ReflectionProperty('C', 'missing')` on an undefined property
/// reports "undefined property 'C::$missing'".
#[test]
fn test_error_reflection_property_undefined_property() {
    expect_error(
        "<?php class C {} $r = new ReflectionProperty('C', 'missing');",
        "undefined property 'C::$missing'",
    );
}

/// Verifies that `new ReflectionMethod` on a method with unsupported attribute
/// argument metadata reports "method has attribute argument metadata that is not
/// supported yet".
#[test]
fn test_error_reflection_method_unsupported_attribute_args() {
    expect_error(
        "<?php class C { #[A(1 + 2)] public function f() {} } $r = new ReflectionMethod('C', 'f');",
        "method has attribute argument metadata that is not supported yet",
    );
}

/// Verifies that an anonymous class missing its body is rejected with a clear diagnostic.
#[test]
fn test_error_anonymous_class_missing_body() {
    expect_error(
        "<?php $o = new class;",
        "Expected '{' to open anonymous class body",
    );
}

/// Verifies that nullsafe dynamic method calls still reject named arguments.
#[test]
fn test_error_nullsafe_dynamic_method_call_named_arguments() {
    expect_error(
        "<?php $obj?->$m(value: 1);",
        "Named arguments are not supported in dynamic calls",
    );
}

/// Verifies user code cannot invoke the compiler-private PDORow constructor.
#[test]
fn test_error_pdo_row_constructor_is_private() {
    expect_pdo_error(
        "<?php $row = new PDORow();",
        "Cannot access private constructor: PDORow::__construct",
    );
}

/// Verifies user code cannot call PDOException's private driver-metadata factory.
#[test]
fn test_error_pdo_exception_internal_factory_is_private() {
    expect_pdo_error(
        "<?php $error = PDOException::__elephcFromErrorInfo('x', ['HY000', 1]);",
        "Cannot access private method: PDOException::__elephcFromErrorInfo",
    );
}

/// Issue #871: a descendant that declares its OWN private constructor HIDES the ancestor's, so
/// the owner walk stops at the descendant and `new static()` from the ancestor's scope names
/// the DESCENDANT's constructor.
///
/// php-src raises `Error: Call to private HideChild::__construct() from scope HideOwner` for the
/// same program, so the class named here is the class PHP names. The test guards the direction
/// the walk must NOT take: resolving to the ancestor would have accepted the call, because the
/// ancestor's constructor is public and the call site is its own scope.
#[test]
fn test_error_a_descendants_own_private_constructor_hides_the_ancestors() {
    expect_error(
        "<?php class HideOwner { public function __construct() {} public static function make(): static { return new static(); } } class HideChild extends HideOwner { private function __construct() {} } HideChild::make();",
        "Cannot access private constructor: HideChild::__construct",
    );
}

/// The same hiding rule through the NAMED form rather than late static binding.
#[test]
fn test_error_a_descendants_own_private_constructor_hides_the_ancestors_for_a_named_new() {
    expect_error(
        "<?php class NamedHideOwner { public function __construct() {} } class NamedHideChild extends NamedHideOwner { private function __construct() {} } $o = new NamedHideChild();",
        "Cannot access private constructor: NamedHideChild::__construct",
    );
}


/// Verifies the malformed declarator lists PHP rejects are still rejected, each by its own cause.
///
/// Accepting `public int $w = 40, $h = 22;` (issue #684) must not turn the comma into a place
/// where anything goes. The four shapes here are the ones a looping parser gets wrong most
/// easily: a duplicate name inside one list, a trailing comma, a constant declarator with no
/// value, and a HOOK block on a list — which php-src rejects outright, because a hook block
/// belongs to one property (`unexpected token "{", expecting "," or ";"`).
#[test]
fn test_malformed_declarator_lists_are_rejected() {
    expect_error(
        "<?php class A { public int $a = 1, $a = 2; }",
        "Cannot redeclare property $a",
    );
    expect_error(
        "<?php class B { const X = 1, X = 2; }",
        "Cannot redeclare class constant X",
    );
    expect_error(
        "<?php class C { public int $a = 1, ; }",
        "Expected a property name after ',' in the declaration list",
    );
    expect_error(
        "<?php class E { const X = 1, Y; }",
        "Expected '=' after class constant name",
    );
    expect_error(
        "<?php class G { public int $a = 1, $b { get => 1; } }",
        "A property hook block needs a declaration of its own",
    );
}

/// Verifies a redeclared constant names the kind of type that declares it (issue #1141).
///
/// The class-like body parser passed a hardcoded `"class"` for every owner, so a trait or an enum
/// reported `Cannot redeclare class constant`; only the interface parser named its own kind. The
/// duplicate is reported whether it repeats a whole `const` or sits inside one declarator list.
#[test]
fn test_constant_redeclaration_names_the_declaring_kind() {
    expect_error(
        "<?php class K { const A = 1; const A = 2; }",
        "Cannot redeclare class constant A",
    );
    expect_error(
        "<?php trait T { const A = 1, A = 2; }",
        "Cannot redeclare trait constant A",
    );
    expect_error(
        "<?php enum E { case X; const A = 1; const A = 2; }",
        "Cannot redeclare enum constant A",
    );
    expect_error(
        "<?php interface I { const A = 1; const A = 2; }",
        "Cannot redeclare interface constant A",
    );
}

/// Verifies PHP-final Exception accessors cannot be overridden by user subclasses.
#[test]
fn test_error_cannot_override_final_exception_method() {
    expect_error(
        "<?php class CustomException extends Exception { public function getMessage(): string { return 'custom'; } }",
        "Cannot override final method Exception::getMessage",
    );
}

/// Verifies Exception::getCode cannot be overridden by user classes.
#[test]
fn test_error_cannot_override_final_exception_get_code() {
    expect_error(
        "<?php class CustomCodeException extends Exception { public function getCode(): int { return 0; } }",
        "Cannot override final method Exception::getCode",
    );
}

/// Verifies a source class that merely shares the name of the compiler-injected `PDOException`
/// gets no exemption: its real spans keep it off the prelude's SQLSTATE `getCode()` hatch, so
/// overriding `getCode()` still reports `Exception` as the declaring class of the final method.
#[test]
fn test_error_user_pdo_exception_cannot_override_final_get_code() {
    expect_error(
        "<?php class PDOException extends Exception { public function getCode(): string|int { return 'HY000'; } }",
        "Cannot override final method Exception::getCode",
    );
    expect_error(
        "<?php namespace App; class PDOException extends \\Exception { public function getCode(): string|int { return 'HY000'; } }",
        "Cannot override final method Exception::getCode",
    );
}

/// Verifies `Error` shares the final Throwable accessors with `Exception`, and the diagnostic
/// names `Error` as the declaring class, also for a subclass reached through `TypeError`.
#[test]
fn test_error_cannot_override_final_error_methods() {
    expect_error(
        "<?php class AppError extends Error { public function getMessage(): string { return 'custom'; } }",
        "Cannot override final method Error::getMessage",
    );
    expect_error(
        "<?php class AppTypeError extends TypeError { public function getPrevious(): ?Throwable { return null; } }",
        "Cannot override final method Error::getPrevious",
    );
}

/// A trait method imported with `as final` cannot be overridden by a subclass, as in PHP 8.3+
/// (`Cannot override final method C::foo()`); an `as final` alias leaves the original
/// overridable (#2352).
#[test]
fn test_error_trait_alias_as_final_method_cannot_be_overridden() {
    expect_error(
        r#"<?php
trait T { public function foo() { return 1; } }
class C { use T { foo as final; } }
class D extends C { public function foo() { return 2; } }
"#,
        "Cannot override final method C::foo",
    );
    expect_error(
        r#"<?php
trait T { public function foo() { return 1; } }
class C { use T { foo as final bar; } }
class D extends C { public function bar() { return 2; } }
"#,
        "Cannot override final method C::bar",
    );
    expect_no_error(
        r#"<?php
trait T { public function foo() { return 1; } }
class C { use T { foo as final bar; } }
class D extends C { public function foo() { return 2; } }
"#,
    );
}

/// An abstract trait method made `as final` stays an abstract final method of an abstract class,
/// as in PHP 8.3+, so only a subclass implementing it fails. A body on the importing class, enum
/// or trait overrides the final import and fails with the trait's name, while a concrete trait
/// method made final stays replaceable there (#2352).
#[test]
fn test_error_trait_alias_as_final_abstract_method() {
    let abstract_trait = "trait T { abstract public function foo(); }\n";
    expect_no_error(&format!(
        "<?php\n{abstract_trait}abstract class C {{ use T {{ foo as final; }} }}\n"
    ));
    expect_no_error(&format!(
        "<?php\n{abstract_trait}abstract class C {{ use T {{ foo as final bar; }} public function foo() {{ return 1; }} }}\n"
    ));
    expect_error(
        &format!(
            "<?php\n{abstract_trait}abstract class C {{ use T {{ foo as final; }} }}\nclass D extends C {{ public function foo() {{ return 1; }} }}\n"
        ),
        "Cannot override final method C::foo",
    );
    for (adaptation, body, message) in [
        ("foo as final;", "foo", "Cannot override final method T::foo"),
        ("foo as final bar;", "bar", "Cannot override final method T::bar"),
    ] {
        expect_error(
            &format!(
                "<?php\n{abstract_trait}class C {{ use T {{ {adaptation} }} public function {body}() {{ return 1; }} }}\n"
            ),
            message,
        );
    }
    expect_error(
        &format!(
            "<?php\n{abstract_trait}enum E {{ use T {{ foo as final; }} public function foo() {{ return 1; }} }}\n"
        ),
        "Cannot override final method T::foo",
    );
    expect_error(
        &format!(
            "<?php\n{abstract_trait}trait O {{ use T {{ foo as final; }} public function foo() {{ return 1; }} }}\nclass C {{ use O; }}\n"
        ),
        "Cannot override final method T::foo",
    );
    expect_no_error(
        "<?php\ntrait T { public function foo() { return 't'; } }\nclass C { use T { foo as final; } public function foo() { return 'c'; } }\necho (new C)->foo();\n",
    );
}

/// An abstract trait method made `as final` is overridden by a concrete method from another trait
/// or by a body of the other kind (static or instance), and the fatal names the trait in the
/// `use` that imported it, which differs from the declaring trait once it is re-exported (#2352).
#[test]
fn test_error_trait_alias_as_final_abstract_method_override_sources() {
    for (source, message) in [
        (
            "trait A { abstract public function foo(); }\ntrait B { public function foo() { return 1; } }\nclass C { use A, B { A::foo as final; } }",
            "Cannot override final method A::foo",
        ),
        (
            "trait A { abstract public function foo(); }\ntrait B { public static function foo() { return 1; } }\nclass C { use A, B { A::foo as final; } }",
            "Cannot override final method A::foo",
        ),
        (
            "trait T { abstract public function foo(); }\ntrait A { use T { foo as final; } }\ntrait B { public function foo() { return 1; } }\nclass C { use A, B; }",
            "Cannot override final method A::foo",
        ),
        (
            "trait T { abstract public function foo(); }\ntrait O { use T { foo as final; } }\nclass C { use O; public function foo() { return 1; } }",
            "Cannot override final method O::foo",
        ),
        (
            "trait T { abstract public function foo(); }\ntrait M { use T { foo as final; } }\ntrait O { use M; public function foo() { return 1; } }\nclass C { use O; }",
            "Cannot override final method M::foo",
        ),
        (
            "trait T { abstract public function foo(); }\ntrait O { use T { foo as final bar; } }\nclass C { use O; public function bar() { return 1; } }",
            "Cannot override final method O::bar",
        ),
        (
            "trait T { abstract public function foo(); }\nabstract class C { use T { foo as final; } public static function foo() { return 1; } }",
            "Cannot override final method T::foo",
        ),
        (
            "trait T { abstract public static function foo(); }\nabstract class C { use T { foo as final; } public function foo() { return 1; } }",
            "Cannot override final method T::foo",
        ),
    ] {
        expect_error(&format!("<?php\n{source}\n"), message);
    }
}

/// Two imports of one name follow Zend's binding order: a later abstract `as final` import
/// overrides any earlier one, an earlier one is overridden by a later concrete method (an `as
/// final` alias included), and a parent method of any visibility or kind overrides it too.
/// A later plain abstract redeclaration of an earlier final one stays valid (#2352).
#[test]
fn test_error_trait_alias_as_final_abstract_method_binding_order() {
    let a = "trait A { abstract public function foo(); }\n";
    for (source, message) in [
        (
            "trait B { abstract public function foo(); }\nabstract class C { use B, A { A::foo as final; } }",
            "Cannot override final method A::foo",
        ),
        (
            "trait B { abstract public function foo(); }\nabstract class C { use A, B { B::foo as final; } }",
            "Cannot override final method B::foo",
        ),
        (
            "trait B { abstract public function foo(); }\nabstract class C { use A, B { A::foo as final; B::foo as final; } }",
            "Cannot override final method B::foo",
        ),
        (
            "trait B { public function bar() { return 1; } }\nabstract class C { use A, B { A::foo as final bar; } }",
            "Cannot override final method A::bar",
        ),
        (
            "trait B { public function bar() { return 1; } }\nabstract class C { use A { foo as final bar; } use B; }",
            "Cannot override final method A::bar",
        ),
        (
            "trait B { public static function bar() { return 1; } }\nabstract class C { use A, B { A::foo as final bar; } }",
            "Cannot override final method A::bar",
        ),
        (
            "class P { public function foo() { return 1; } }\nclass C extends P { use A { foo as final; } }",
            "Cannot override final method A::foo",
        ),
        (
            "class P { private function foo() { return 1; } }\nabstract class C extends P { use A { foo as final; } }",
            "Cannot override final method A::foo",
        ),
        (
            "class P { public static function foo() { return 1; } }\nabstract class C extends P { use A { foo as final; } }",
            "Cannot override final method A::foo",
        ),
        (
            "class G { public function foo() { return 1; } }\nclass P extends G {}\nabstract class C extends P { use A { foo as final; } }",
            "Cannot override final method A::foo",
        ),
        (
            "class P { public function bar() { return 1; } }\nabstract class C extends P { use A { foo as final bar; } }",
            "Cannot override final method A::bar",
        ),
    ] {
        expect_error(&format!("<?php\n{a}{source}\n"), message);
    }
    expect_no_error(&format!(
        "<?php\n{a}trait B {{ abstract public function foo(); }}\nabstract class C {{ use A, B {{ A::foo as final; }} }}\n"
    ));
}

/// A class body method that overrides both an abstract `as final` import and a final, non
/// private ancestor method is reported against the ancestor, as PHP binds the parent first;
/// without such an ancestor it is reported against the import. Each program reports exactly one
/// error, so a remaining method of the name adds no kind or final diagnostic (#2352).
#[test]
fn test_error_trait_alias_as_final_abstract_method_with_final_ancestor() {
    let a = "trait A { abstract public function foo(); }\n";
    for (source, message) in [
        (
            format!("class P {{ final public function foo() {{}} }}\n{a}abstract class C extends P {{ use A {{ foo as final; }} public function foo() {{}} }}"),
            "Cannot override final method P::foo",
        ),
        (
            format!("class P {{ final public static function foo() {{}} }}\n{a}abstract class C extends P {{ use A {{ foo as final; }} public static function foo() {{}} }}"),
            "Cannot override final method P::foo",
        ),
        (
            format!("class G {{ final public function foo() {{}} }}\nclass P extends G {{}}\n{a}abstract class C extends P {{ use A {{ foo as final; }} public function foo() {{}} }}"),
            "Cannot override final method G::foo",
        ),
        (
            format!("{a}abstract class C extends P {{ use A {{ foo as final; }} public function foo() {{}} }}\nclass P {{ final public function foo() {{}} }}"),
            "Cannot override final method P::foo",
        ),
        (
            format!("class P {{ final public function bar() {{}} }}\n{a}abstract class C extends P {{ use A {{ foo as final bar; }} public function bar() {{}} }}"),
            "Cannot override final method P::bar",
        ),
        (
            format!("class P {{ public function foo() {{}} }}\n{a}abstract class C extends P {{ use A {{ foo as final; }} public function foo() {{}} }}"),
            "Cannot override final method A::foo",
        ),
        (
            format!("class P {{ final private function foo() {{}} }}\n{a}abstract class C extends P {{ use A {{ foo as final; }} public function foo() {{}} }}"),
            "Cannot override final method A::foo",
        ),
        (
            format!("class P {{ final public function foo() {{}} }}\n{a}trait B {{ abstract public static function foo(); }}\nabstract class C extends P {{ use A, B {{ A::foo as final; }} }}"),
            "Cannot override final method A::foo",
        ),
        (
            format!("class P {{ public function foo() {{}} }}\n{a}trait B {{ abstract public static function foo(); }}\nabstract class C extends P {{ use A, B {{ A::foo as final; }} }}"),
            "Cannot override final method A::foo",
        ),
    ] {
        let error = check_source_full(&format!("<?php\n{source}\n"))
            .err()
            .expect("program should fail");
        let messages: Vec<String> = error.flatten().into_iter().map(|error| error.message).collect();
        assert_eq!(messages.len(), 1, "{source}: {messages:?}");
        assert!(messages[0].contains(message), "{source}: {messages:?}");
    }
}

/// Abstract `as final` imports bind after the class's own and concrete imported methods and after
/// the parent is inherited, as in php-src's `zend_do_link_class`: with any other method of the
/// name the earliest final import in `use` order is reported, with only the imports the first
/// one that a later final import overrides; a concrete import or class body is checked as an
/// override of a visible ancestor first, also before a plain abstract import of the other kind
/// fails, and every binding failure waits for the class's other inheritance checks (#2352).
#[test]
fn test_error_trait_alias_as_final_abstract_method_binding_model() {
    let ab = "trait A { abstract public function foo(); }\ntrait B { abstract public function foo(); }\n";
    let a = "trait A { abstract public function foo(); }\n";
    let both = "A::foo as final; B::foo as final;";
    let single_error = |source: &str| -> String {
        let error = check_source_full(&format!("<?php\n{source}\n"))
            .err()
            .expect("program should fail");
        let messages: Vec<String> = error.flatten().into_iter().map(|error| error.message).collect();
        assert_eq!(messages.len(), 1, "{source}: {messages:?}");
        messages.into_iter().next().unwrap()
    };
    for (source, message) in [
        (format!("{ab}class P {{ public function foo() {{}} }}\nabstract class C extends P {{ use A, B {{ {both} }} }}"), "Cannot override final method A::foo"),
        (format!("{ab}abstract class C {{ use A, B {{ {both} }} public function foo() {{}} }}"), "Cannot override final method A::foo"),
        (format!("{ab}class P {{ public function foo() {{}} }}\nabstract class C extends P {{ use B, A {{ {both} }} }}"), "Cannot override final method B::foo"),
        (format!("{ab}abstract class C {{ use A, B {{ {both} }} }}"), "Cannot override final method B::foo"),
        (format!("{ab}enum E {{ use A, B {{ {both} }} public function foo() {{}} }}"), "Cannot override final method A::foo"),
        (format!("{ab}enum E {{ use A, B {{ {both} }} }}"), "Cannot override final method B::foo"),
        (format!("{ab}trait O {{ use A, B {{ {both} }} public function foo() {{}} }}\nclass Z {{ use O; }}"), "Cannot override final method A::foo"),
        (format!("{ab}trait O {{ use A, B {{ {both} }} }}\nabstract class Z {{ use O; }}"), "Cannot override final method B::foo"),
        ("trait A { abstract public function getMessage(); }\ntrait B { abstract public function getMessage(); }\nclass C extends Exception { use A, B { A::getMessage as final; B::getMessage as final; } }".to_string(), "Cannot override final method A::getMessage"),
        (format!("class P {{ final public function foo() {{}} }}\n{a}trait B {{ public function foo() {{ return 1; }} }}\nclass C extends P {{ use A, B {{ A::foo as final; }} }}"), "Cannot override final method P::foo"),
        (format!("class P {{ public function foo() {{}} }}\n{a}trait B {{ public static function foo() {{ return 1; }} }}\nclass C extends P {{ use A, B {{ A::foo as final; }} }}"), "Cannot change method kind"),
        (format!("class P {{ public function foo() {{}} }}\n{a}trait B {{ protected function foo() {{ return 1; }} }}\nclass C extends P {{ use A, B {{ A::foo as final; }} }}"), "Cannot reduce visibility"),
        (format!("class P {{ public function foo() {{}} }}\n{a}trait B {{ public function foo() {{ return 1; }} }}\nclass C extends P {{ use A, B {{ A::foo as final; }} }}"), "Cannot override final method A::foo"),
        (format!("class P {{ private function foo() {{}} }}\n{a}trait B {{ public static function foo() {{ return 1; }} }}\nclass C extends P {{ use A, B {{ A::foo as final; }} }}"), "Cannot override final method A::foo"),
        (format!("class P {{ public function bar() {{}} }}\n{a}trait B {{ public static function bar() {{ return 1; }} }}\nclass C extends P {{ use A, B {{ A::foo as final bar; }} }}"), "Cannot change method kind"),
    ] {
        let reported = single_error(&source);
        assert!(reported.contains(message), "{source}: {reported}");
    }
    // A plain abstract import of the other kind fails while binding too, but only after the class
    // body passes its checks against a visible ancestor.
    let kinds = "trait A { abstract public static function foo(); }\ntrait B { abstract public function foo(); }\n";
    for (source, message) in [
        (format!("{kinds}class P {{ final public function foo() {{}} }}\nabstract class C extends P {{ use A, B {{ B::foo as final; }} abstract public function foo(); }}"), "Cannot override final method P::foo"),
        (format!("{kinds}class P {{ public function foo() {{}} }}\nclass C extends P {{ use A, B {{ B::foo as final; }} protected function foo() {{ return 1; }} }}"), "Cannot reduce visibility"),
        (format!("{kinds}class P {{ public function foo() {{}} }}\nabstract class C extends P {{ use A, B {{ B::foo as final; }} abstract public function foo(); }}"), "Cannot make concrete method abstract"),
        (format!("{kinds}class P {{ public function foo() {{}} }}\nclass C extends P {{ use A, B {{ B::foo as final; }} public function foo() {{}} }}"), "Cannot change method kind when overriding C::foo"),
    ] {
        let reported = single_error(&source);
        assert!(reported.contains(message), "{source}: {reported}");
    }
    // The binding failure waits for the whole parent inheritance of the class: other methods
    // and final constants first, then the first failure in trait binding order.
    for (source, message) in [
        (format!("{a}class P {{ public function foo() {{}} public function bar() {{}} }}\nclass C extends P {{ use A {{ foo as final; }} public function foo() {{}} protected function bar() {{}} }}"), "Cannot reduce visibility when overriding method: C::bar"),
        (format!("{kinds}class P {{ public function foo() {{}} public function bar() {{}} }}\nclass C extends P {{ use A, B {{ B::foo as final; }} protected function bar() {{}} }}"), "Cannot reduce visibility when overriding method: C::bar"),
        (format!("{a}trait B {{ abstract public function bar(); }}\nclass P {{ public function foo() {{}} public function bar() {{}} }}\nclass C extends P {{ use A, B {{ A::foo as final; B::bar as final; }} public function bar() {{}} public function foo() {{}} }}"), "Cannot override final method A::foo"),
        (format!("{kinds}class P {{ public function foo() {{}} final public const X = 1; }}\nclass C extends P {{ use A, B {{ B::foo as final; }} public function foo() {{}} public const X = 2; }}"), "final constant"),
        // Interface final constants are checked after the abstract trait binding.
        (format!("interface I {{ final public const X = 1; }}\n{a}class C implements I {{ use A {{ foo as final; }} public function foo() {{}} public const X = 2; }}"), "Cannot override final method A::foo"),
        // PHP stops binding at the failure, so a requirement bound later is never checked.
        (format!("class P {{ public function foo() {{}} public static function bar() {{}} }}\n{a}trait T {{ abstract public function bar(); }}\nclass C extends P {{ use A, T {{ A::foo as final; }} public function foo() {{}} }}"), "Cannot override final method A::foo"),
        (format!("{kinds}enum E {{ use A, B {{ B::foo as final; }} public function foo() {{}} case X; }}"), "Cannot change method kind when overriding E::foo"),
        // The parent's own final constant is checked before an inherited interface one.
        (format!("interface I {{ final public const Y = 1; }}\nclass P implements I {{ final public const X = 1; }}\n{a}class C extends P {{ use A {{ foo as final; }} public function foo() {{}} public const Y = 2; public const X = 2; }}"), "X cannot override final constant"),
        // A compatible abstract redeclaration of an earlier requirement stays on the final error.
        (format!("{ab}abstract class C {{ use A, B {{ B::foo as final; }} abstract public function foo(); }}"), "Cannot override final method B::foo"),
        // Interface final constants follow the parent's interface and constant order.
        ("interface I { final public const Y = 1; }\ninterface J { final public const Z = 1; }\nclass P implements I, J {}\nclass C extends P { public const Z = 2; public const Y = 2; }".to_string(), "Y cannot override final"),
        // A final constant of an interface the parent implements is inherited with the parent.
        (format!("interface I {{ final public const X = 1; }}\nclass P implements I {{}}\n{a}class C extends P {{ use A {{ foo as final; }} public function foo() {{}} public const X = 2; }}"), "final interface constant"),
        (format!("class P {{ public function foo(): int {{ return 1; }} }}\ntrait A {{ abstract public function foo(): string; }}\ntrait B {{ abstract public function foo(): string; }}\nabstract class C extends P {{ use B, A {{ B::foo as final; }} }}"), "Cannot override final method B::foo"),
        (format!("{a}trait T {{ abstract public function bar(): string; }}\nclass C {{ use A, T {{ A::foo as final; }} public function foo() {{}} public function bar(): int {{ return 1; }} }}"), "Cannot override final method A::foo"),
        ("trait A { abstract public function foo(); public $x; }\ntrait B { abstract public function foo(); }\nenum E { use A, B { A::foo as final; B::foo as final; } public function foo() {} case X; }".to_string(), "Cannot override final method A::foo"),
    ] {
        let reported = single_error(&source);
        assert!(reported.contains(message), "{source}: {reported}");
    }
    // A parent that failed to build keeps its own error: the child's used traits do not replace it.
    let parent_failed = check_source_full(
        "<?php\ntrait N { abstract public function bar(): string; }\ntrait T { use N; public function bar(): int { return 1; } }\ntrait A { abstract public function foo(); }\nfinal class GP {}\nclass P extends GP {}\nclass C extends P { use A, T { A::foo as final; } public function foo() {} }\n",
    )
    .err()
    .expect("program should fail");
    let messages: Vec<String> = parent_failed.flatten().into_iter().map(|error| error.message).collect();
    assert!(messages[0].contains("cannot extend final class"), "{messages:?}");
    assert!(messages.iter().all(|message| !message.contains("bar")), "{messages:?}");
    // A parent that is unknown, not a class, or part of a cycle through the class is the class's
    // own error, which the used trait's incompatibility still replaces.
    let trait_pair = "trait N { abstract public function bar(): string; }\ntrait T { use N; public function bar(): int { return 1; } }\ntrait A { abstract public function foo(); }\n";
    for parent in [
        "",
        "interface Missing {}\n",
        "trait Missing {}\n",
        "class Missing extends C {}\n",
    ] {
        let error = check_source_full(&format!(
            "<?php\n{trait_pair}{parent}class C extends Missing {{ use A, T {{ A::foo as final; }} public function foo() {{}} }}\n"
        ))
        .err()
        .expect("program should fail");
        let messages: Vec<String> = error.flatten().into_iter().map(|error| error.message).collect();
        assert!(messages[0].contains("bar"), "{parent}: {messages:?}");
    }
    // A private final constant is a compile error of the class, before any trait is linked.
    let error = check_source_full(&format!(
        "<?php\n{trait_pair}class C {{ private final const X = 1; use A, T {{ A::foo as final; }} public function foo() {{}} }}\n"
    ))
    .err()
    .expect("program should fail");
    let messages: Vec<String> = error.flatten().into_iter().map(|error| error.message).collect();
    assert!(messages[0].contains("Private constant C::X cannot be final"), "{messages:?}");
    for source in [
        // Abstract imports bound before the failure are checked first, against the parent or
        // the method that satisfies them.
        format!("class P {{ public function foo(): int {{ return 1; }} }}\ntrait A {{ abstract public function foo(): string; }}\ntrait B {{ abstract public function foo(): string; }}\nabstract class C extends P {{ use A, B {{ B::foo as final; }} }}"),
        format!("class P {{ public function foo(): int {{ return 1; }} }}\ntrait A {{ abstract public function foo(): string; }}\ntrait B {{ abstract public function foo(): string; }}\nclass C extends P {{ use A, B {{ B::foo as final; }} public function foo(): int {{ return 1; }} }}"),
        format!("class P {{ public function foo() {{}} public function bar(): int {{ return 1; }} }}\n{a}trait T {{ abstract public function bar(): string; }}\nclass C extends P {{ use T, A {{ A::foo as final; }} public function foo() {{}} public function bar(): int {{ return 1; }} }}"),
        // An abstract redeclaration is checked against an earlier requirement too.
        "trait A { abstract public function foo(): string; }\ntrait B { abstract public function foo(): string; }\nabstract class C { use A, B { B::foo as final; } abstract public function foo(): int; }".to_string(),
        "abstract class P { abstract public function bar(): int; }\ntrait T { abstract public function bar(): string; }\ntrait A { abstract public function foo(); }\nabstract class C extends P { use T, A { A::foo as final; } abstract public function bar(): int; public function foo() {} }".to_string(),
        // A used trait's own satisfied requirement is checked while compiling that trait.
        "trait N { abstract public function bar(): string; }\ntrait T { use N; public function bar(): int { return 1; } }\ntrait A { abstract public function foo(); }\nclass C { use A, T { A::foo as final; } public function foo() {} }".to_string(),
        // A used trait's own satisfied requirement is still the error when the class failed an
        // inheritance check first, and is reported once for two classes.
        "trait N { abstract public function bar(): string; }\ntrait T { use N; public function bar(): int { return 1; } }\ntrait A { abstract public function foo(); }\nclass P { final public const X = 1; }\nclass C extends P { use A, T { A::foo as final; } public function foo() {} public const X = 2; }".to_string(),
        "trait N { abstract public function bar(): string; }\ntrait T { use N; public function bar(): int { return 1; } }\ntrait A { abstract public function foo(); }\nclass C { use A, T { A::foo as final; } public function foo() {} }\nclass E { use A, T { A::foo as final; } public function foo() {} }".to_string(),
        // A requirement bound before the failure is checked first.
        format!("class P {{ public function foo() {{}} public function bar(): int {{ return 1; }} }}\n{a}trait T {{ abstract public function bar(): string; }}\nclass C extends P {{ use T, A {{ A::foo as final; }} public function foo() {{}} }}"),
        format!("{kinds}class P {{ public function foo(): int {{ return 1; }} }}\nclass C extends P {{ use A, B {{ B::foo as final; }} public function foo(): string {{ return 'x'; }} }}"),
        format!("{ab}class P {{ public function foo(): int {{ return 1; }} }}\nabstract class C extends P {{ use A, B {{ {both} }} public function foo(): string {{ return 'x'; }} }}"),
        format!("class P {{ public function foo(): int {{ return 1; }} }}\n{a}trait B {{ public function foo(): string {{ return 'x'; }} }}\nclass C extends P {{ use A, B {{ A::foo as final; }} }}"),
    ] {
        let reported = single_error(&source);
        assert!(!reported.contains("Cannot override final method"), "{source}: {reported}");
    }
}

/// Builtin ancestors count like user ones for an abstract `as final` import: a kept import is
/// overridden by `Exception::getMessage`, and a class body replacing it meets the final builtin
/// method first. A class body that overrides a visible, non-final ancestor illegally reports
/// that override (kind, visibility, abstract, signature) before the import, and the other
/// methods of the name add nothing (#2352).
#[test]
fn test_error_trait_alias_as_final_abstract_method_override_precedence() {
    let single_error = |source: &str| -> String {
        let error = check_source_full(&format!("<?php\n{source}\n"))
            .err()
            .expect("program should fail");
        let messages: Vec<String> = error.flatten().into_iter().map(|error| error.message).collect();
        assert_eq!(messages.len(), 1, "{source}: {messages:?}");
        messages.into_iter().next().unwrap()
    };
    for (source, message) in [
        (
            "trait A { abstract public function getMessage(); }\nclass C extends Exception { use A { getMessage as final; } }",
            "Cannot override final method A::getMessage",
        ),
        (
            "trait A { abstract public function getPrevious(): ?Throwable; }\nabstract class C extends TypeError { use A { getPrevious as final; } }",
            "Cannot override final method A::getPrevious",
        ),
        (
            "trait A { abstract public function getMessage(); }\nclass C extends Exception { use A { getMessage as final; } public function getMessage() { return 'x'; } }",
            "Cannot override final method Exception::getMessage",
        ),
        (
            "class P { public static function foo() {} }\ntrait A { abstract public function foo(); }\nabstract class C extends P { use A { foo as final; } public function foo() {} }",
            "Cannot change method kind",
        ),
        (
            "class P { public function foo() {} }\ntrait A { abstract public function foo(); }\nabstract class C extends P { use A { foo as final; } protected function foo() {} }",
            "Cannot reduce visibility",
        ),
        (
            "class P { public function foo() {} }\ntrait A { abstract public function foo(); }\nabstract class C extends P { use A { foo as final; } abstract public function foo(); }",
            "Cannot make concrete method abstract",
        ),
        (
            "class P { final public function foo() {} }\ntrait A { abstract public function foo(); }\ntrait B { abstract public static function foo(); }\nabstract class C extends P { use A, B { A::foo as final; } abstract public function foo(); }",
            "Cannot override final method P::foo",
        ),
        (
            "class P { public function foo() {} }\ntrait A { abstract public function foo(); }\nabstract class C extends P { use A { foo as final; } public function foo() {} }",
            "Cannot override final method A::foo",
        ),
    ] {
        let reported = single_error(source);
        assert!(reported.contains(message), "{source}: {reported}");
    }
    let reported = single_error(
        "class P { public function foo(): int { return 1; } }\ntrait A { abstract public function foo(); }\nabstract class C extends P { use A { foo as final; } public function foo(): string { return 'x'; } }",
    );
    assert!(!reported.contains("Cannot override final method"), "{reported}");
}

/// The same abstract trait method reached through two traits is one method, final only when its
/// first copy is; `insteadof` keeps an abstract `as final` import as a requirement; two
/// concrete methods collide before any final requirement is compared; `foo as final foo` makes
/// the method itself final; and a parent override is reported once (#2352). An `as final`
/// made inside the used trait survives `insteadof` the same way.
#[test]
fn test_error_trait_alias_as_final_abstract_method_edge_cases() {
    let t = "trait T { abstract public function foo(); }\n";
    expect_no_error(&format!(
        "<?php\n{t}trait A {{ use T; }}\ntrait B {{ use T {{ foo as final; }} }}\nabstract class C {{ use A, B; }}\nclass D extends C {{ public function foo() {{ return 1; }} }}\n"
    ));
    expect_error(
        &format!(
            "<?php\n{t}trait A {{ use T; }}\ntrait B {{ use T {{ foo as final; }} }}\nabstract class C {{ use B, A; }}\nclass D extends C {{ public function foo() {{ return 1; }} }}\n"
        ),
        "Cannot override final method C::foo",
    );
    let a = "trait A { abstract public function foo(); }\n";
    expect_error(
        &format!(
            "<?php\n{a}trait B {{ public function foo() {{ return 1; }} }}\nclass C {{ use A, B {{ B::foo insteadof A; A::foo as final; }} }}\n"
        ),
        "Cannot override final method A::foo",
    );
    expect_error(
        &format!(
            "<?php\n{a}trait B {{ abstract public function foo(); }}\nabstract class C {{ use A, B {{ A::foo as final; B::foo insteadof A; }} }}\nclass D extends C {{ public function foo() {{ return 1; }} }}\n"
        ),
        "Cannot override final method C::foo",
    );
    expect_error(
        &format!(
            "<?php\n{a}trait B {{ abstract public function foo(); }}\nabstract class C {{ use B, A {{ A::foo as final; B::foo insteadof A; }} }}\n"
        ),
        "Cannot override final method A::foo",
    );
    expect_error(
        &format!(
            "<?php\n{a}trait B {{ public function foo() {{ return 1; }} }}\ntrait C {{ public function foo() {{ return 2; }} }}\nclass D {{ use A, B, C {{ A::foo as final; }} }}\n"
        ),
        "Trait method C::foo has not been applied as D::foo, because of collision with B::foo",
    );
    expect_error(
        &format!(
            "<?php\n{t}abstract class C {{ use T {{ foo as final foo; }} }}\nclass D extends C {{ public function foo() {{ return 1; }} }}\n"
        ),
        "Cannot override final method C::foo",
    );
    let inner_final = "trait T { abstract public function foo(); }\ntrait A { use T { foo as final; } }\n";
    expect_error(
        &format!(
            "<?php\n{inner_final}trait B {{ public function foo() {{ return 1; }} }}\nclass C {{ use A, B {{ B::foo insteadof A; }} }}\n"
        ),
        "Cannot override final method A::foo",
    );
    expect_error(
        &format!(
            "<?php\n{inner_final}trait B {{ abstract public function foo(); }}\nabstract class C {{ use A, B {{ B::foo insteadof A; }} }}\nclass D extends C {{ public function foo() {{ return 1; }} }}\n"
        ),
        "Cannot override final method C::foo",
    );
    let static_parent = format!(
        "<?php\n{a}class P {{ public static function foo() {{ return 1; }} }}\nabstract class C extends P {{ use A {{ foo as final; }} }}\n"
    );
    let message = check_source(&static_parent).expect_err("parent override should fail");
    assert!(message.contains("Cannot override final method A::foo"), "{message}");
    assert!(!message.contains("Cannot change method kind"), "{message}");
}
