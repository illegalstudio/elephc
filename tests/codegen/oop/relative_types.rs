//! Purpose:
//! End-to-end codegen tests for the relative class types `self`, `static`, and `parent` used
//! in method parameter, method return, and property type positions.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `self` resolves lexically while method return `static` binds to the call-site receiver.
//! - Trait methods resolve `self`/`static` to the using class, exercised by `test_static_in_trait`.

use super::*;

/// Verifies that a `self` return type lets a method return `$this` and be chained.
#[test]
fn test_self_return_type_chains() {
    let out = compile_and_run(
        "<?php
        class C {
            public function me(): self { return $this; }
            public function v(): string { return \"ok\"; }
        }
        echo (new C())->me()->v();
        ",
    );
    assert_eq!(out, "ok");
}

/// Regression: a `self`-typed VARIADIC parameter (`self ...$items`) must have its `self`
/// rewritten to the enclosing class like every other member type annotation. Previously the
/// variadic-param type was skipped, so `self` survived and was rejected with
/// "Cannot use 'self' as a type outside of a class".
#[test]
fn test_self_typed_variadic_param() {
    let out = compile_and_run(
        "<?php
        final class Bag {
            public function __construct(public string $x) {}
            public static function concat(self ...$items): self {
                $buf = '';
                foreach ($items as $i) { $buf .= $i->x; }
                return new self($buf);
            }
        }
        echo Bag::concat(new Bag('a'), new Bag('b'), new Bag('c'))->x;
        ",
    );
    assert_eq!(out, "abc");
}

/// Regression: a `self`-typed VARIADIC parameter on an ENUM method must be rewritten to the
/// enum name like regular parameters and return types. The enum schema path uses its own
/// relative-type substitution, which previously skipped the variadic-param type.
#[test]
fn test_enum_self_typed_variadic_param() {
    let out = compile_and_run(
        "<?php
        enum Suit: string {
            case Hearts = 'H';
            case Spades = 'S';
            case Clubs = 'C';
            public static function join(self ...$suits): string {
                $buf = '';
                foreach ($suits as $s) { $buf .= $s->value; }
                return $buf;
            }
        }
        echo Suit::join(Suit::Hearts, Suit::Spades, Suit::Clubs);
        ",
    );
    assert_eq!(out, "HSC");
}

/// Verifies that a `static` return type returns a late-bound instance via `new static()`.
#[test]
fn test_static_return_type() {
    let out = compile_and_run(
        "<?php
        class C {
            public static function make(): static { return new static(); }
            public function v(): string { return \"made\"; }
        }
        echo C::make()->v();
        ",
    );
    assert_eq!(out, "made");
}

/// Verifies an inherited static factory returning `static` exposes subclass-only methods.
#[test]
fn test_inherited_static_factory_return_binds_to_called_class() {
    let out = compile_and_run(
        r#"<?php
class Factory {
    public static function make(): static { return new static(); }
}
final class ProductFactory extends Factory {
    public function label(): string { return "product"; }
}
echo ProductFactory::make()->label();
echo ":";
echo (new ReflectionMethod(Factory::class, "make"))->getReturnType()->getName();
"#,
    );
    assert_eq!(out, "product:static");
}

/// Verifies an inherited non-`with*` method returning `static` exposes subclass-only methods.
#[test]
fn test_inherited_static_return_type_binds_to_subclass_receiver() {
    let out = compile_and_run(
        r#"<?php
class Builder {
    public function andWhere(string $condition): static { return $this; }
}
final class QueryBuilder extends Builder {
    public function getSQL(): string { return "SELECT"; }
}
echo (new QueryBuilder())->andWhere('active = 1')->getSQL();
"#,
    );
    assert_eq!(out, "SELECT");
}

/// Verifies nullable late-static returns retain null while binding the object branch.
#[test]
fn test_nullable_static_return_binds_object_branch_to_receiver() {
    let out = compile_and_run(
        r#"<?php
class MaybeBuilder {
    public function maybe(bool $present): ?static {
        return $present ? $this : null;
    }
}
final class ConcreteBuilder extends MaybeBuilder {
    public function build(): string { return "built"; }
}
$builder = new ConcreteBuilder();
echo $builder->maybe(true)?->build();
echo $builder->maybe(false)?->build() ?? "none";
"#,
    );
    assert_eq!(out, "builtnone");
}

/// Verifies a compound late-static return keeps its explicit member in typing, ABI boxing,
/// and Reflection metadata.
#[test]
fn test_late_static_union_preserves_explicit_member() {
    let out = compile_and_run(
        r#"<?php
class Choice {
    public function choose(bool $same): static|Choice {
        return $same ? $this : new Choice();
    }
    public function label(): string { return "choice"; }
}
final class SpecialChoice extends Choice {}
$value = (new SpecialChoice())->choose(false);
echo $value->label() . ":";
$type = (new ReflectionMethod(Choice::class, "choose"))->getReturnType();
if ($type instanceof ReflectionUnionType) {
    echo count($type->getTypes());
    foreach ($type->getTypes() as $member) {
        echo ":" . $member->getName();
    }
}
"#,
    );
    assert_eq!(out, "choice:2:Choice:static");
}

/// Verifies a child override may covariantly narrow `static|false` to `static`.
#[test]
fn test_late_static_union_override_can_narrow_to_static() {
    let out = compile_and_run(
        r#"<?php
class MaybeCloneable {
    public function duplicate(): static|false { return false; }
}
final class AlwaysCloneable extends MaybeCloneable {
    public function duplicate(): static { return $this; }
    public function label(): string { return "clone"; }
}
echo (new AlwaysCloneable())->duplicate()->label();
"#,
    );
    assert_eq!(out, "clone");
}

/// Verifies that a `parent` return type resolves to the parent class and exposes its methods.
#[test]
fn test_parent_return_type() {
    let out = compile_and_run(
        "<?php
        class P { public function who(): string { return \"P\"; } }
        class C extends P {
            public function up(): parent { return $this; }
        }
        echo (new C())->up()->who();
        ",
    );
    assert_eq!(out, "P");
}

/// Verifies that a `self` parameter type accepts another instance of the same class.
#[test]
fn test_self_parameter_type() {
    let out = compile_and_run(
        "<?php
        class C {
            public int $n = 0;
            public function plus(self $other): int { return $this->n + $other->n; }
        }
        $a = new C(); $a->n = 2;
        $b = new C(); $b->n = 3;
        echo $a->plus($b);
        ",
    );
    assert_eq!(out, "5");
}

/// Verifies that a nullable `?self` property stores a same-class instance and null.
#[test]
fn test_self_nullable_property() {
    let out = compile_and_run(
        "<?php
        class Node {
            public ?self $next = null;
            public int $v = 0;
        }
        $a = new Node(); $a->v = 1;
        $b = new Node(); $b->v = 2;
        $a->next = $b;
        echo $a->next->v;
        echo $a->next->next === null ? \"end\" : \"?\";
        ",
    );
    assert_eq!(out, "2end");
}

/// Verifies that a `?self` return type returns either a same-class instance or null.
#[test]
fn test_self_nullable_return() {
    let out = compile_and_run(
        "<?php
        class C {
            public function maybe(bool $b): ?self { return $b ? $this : null; }
            public function v(): string { return \"M\"; }
        }
        $c = new C();
        echo $c->maybe(true)->v();
        echo $c->maybe(false) === null ? \"N\" : \"?\";
        ",
    );
    assert_eq!(out, "MN");
}

/// Verifies that `static` inside a trait method resolves to the using class, not the trait,
/// so the returned instance exposes the using class's own methods.
#[test]
fn test_static_in_trait() {
    let out = compile_and_run(
        "<?php
        trait Fluent {
            public function chain(): static { return $this; }
        }
        class Builder {
            use Fluent;
            public function build(): string { return \"built\"; }
        }
        echo (new Builder())->chain()->build();
        ",
    );
    assert_eq!(out, "built");
}

/// Compiles and runs the checked-in `examples/relative-class-types/main.php` fixture, which
/// exercises `self`, a late-bound inherited `static` return, and a nullable `?self` property.
#[test]
fn test_example_relative_class_types_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/relative-class-types/main.php"));
    assert_eq!(out, "599\n3\n6\ntail\nSELECT * WHERE active = 1\n");
}
