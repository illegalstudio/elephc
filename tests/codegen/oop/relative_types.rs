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

/// Verifies `new static()` runs an ancestor's private constructor for a selected descendant.
///
/// A private method is not inherited, so the descendant's own method map carries no
/// `__construct`; resolving only there allocated the object and ran nothing at all.
#[test]
fn test_new_static_runs_inherited_private_constructor() {
    let out = compile_and_run(
        r#"<?php
class Connection {
    private function __construct() { echo "connecting "; }
    public static function make(): static { return new static(); }
    public function ok(): string { return "ok"; }
}
class PooledConnection extends Connection {}
echo PooledConnection::make()->ok();
"#,
    );
    assert_eq!(out, "connecting ok");
}

/// Verifies the whole constructor chain runs when the private constructor calls `parent::`.
#[test]
fn test_new_static_private_constructor_runs_parent_chain() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public function __construct() { echo "base "; }
}
class Middle extends Base {
    private function __construct() { parent::__construct(); echo "middle "; }
    public static function make(): static { return new static(); }
    public function ok(): string { return "ok"; }
}
class Leaf extends Middle {}
echo Leaf::make()->ok();
"#,
    );
    assert_eq!(out, "base middle ok");
}

/// Verifies an inherited private constructor still receives its arguments.
///
/// With arguments the same missing lookup surfaced as a checker arity error claiming the
/// descendant takes none, rather than as a silently skipped call.
#[test]
fn test_new_static_inherited_private_constructor_takes_arguments() {
    let out = compile_and_run(
        r#"<?php
class Tagged {
    private function __construct(private int $tag) {}
    public static function make(int $tag): static { return new static($tag); }
    public function tag(): int { return $this->tag; }
}
class SubTagged extends Tagged {}
echo SubTagged::make(7)->tag();
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies naming the descendant directly from the declaring scope also runs the constructor.
#[test]
fn test_fixed_new_of_descendant_runs_declaring_private_constructor() {
    let out = compile_and_run(
        r#"<?php
class Owner {
    private function __construct() { echo "built "; }
    public static function makeChild(): Owner { return new OwnedChild(); }
}
class OwnedChild extends Owner {}
echo Owner::makeChild() instanceof OwnedChild ? "yes" : "no";
"#,
    );
    assert_eq!(out, "built yes");
}

/// Issue #868: an inherited private constructor with OMITTED DEFAULTS on the named
/// (`fixed_new`) path.
///
/// `ir_lower::expr::object_construction::constructor_signature` read the descendant's own
/// `methods` map, which a private ancestor constructor is deliberately absent from, so the
/// call site saw no constructor, padded no defaults, and `fixed_new` rejected the arity with
/// `constructor call to OwnedDefaults::__construct with 0 args for 1 params`.
#[test]
fn test_fixed_new_of_descendant_pads_inherited_private_constructor_defaults() {
    let out = compile_and_run(
        r#"<?php
class OwnerDefaults {
    private function __construct(private int $n = 4) {}
    public static function makeChild(): OwnerDefaults { return new OwnedDefaults(); }
    public function n(): int { return $this->n; }
}
class OwnedDefaults extends OwnerDefaults {}
echo OwnerDefaults::makeChild()->n();
"#,
    );
    assert_eq!(out, "4");
}

/// Only the OMITTED arguments are padded: an explicitly passed one still wins.
#[test]
fn test_fixed_new_of_descendant_keeps_an_explicit_argument_over_the_default() {
    let out = compile_and_run(
        r#"<?php
class OwnerMixed {
    private function __construct(private int $a = 1, private int $b = 2) {}
    public static function makeChild(): OwnerMixed { return new OwnedMixed(9); }
    public function sum(): int { return $this->a * 10 + $this->b; }
}
class OwnedMixed extends OwnerMixed {}
echo OwnerMixed::makeChild()->sum();
"#,
    );
    assert_eq!(out, "92");
}

/// A CONTROL for the reflection half of the same lookup: an inherited PUBLIC constructor IS
/// copied into the descendant's own `methods` map, so the direct lookup found it too and this
/// fixture passes either way. The regression test for the changed helper is the
/// private-ancestor one below.
#[test]
fn test_reflection_new_instance_pads_inherited_constructor_defaults() {
    let out = compile_and_run(
        r#"<?php
class ReflOwner {
    public function __construct(public int $n = 5) {}
}
class ReflChild extends ReflOwner {}
$reflected = new ReflectionClass('ReflChild');
echo $reflected->newInstance()->n;
"#,
    );
    assert_eq!(out, "5");
}

/// The REGRESSION test for `reflection_new_instance::constructor_signature_for_class_name`: a
/// descendant of a class with a PRIVATE constructor, which is the only shape where the changed
/// lookup differs from the direct one, because it is the only shape whose own `methods` map
/// lacks the entry.
///
/// Measured both ways on this fixture: with the owner walk it prints `6` (the default is padded
/// and the ancestor's constructor runs); reading the descendant's own map it prints `0` — no
/// signature, so no padding and no constructor call at all, leaving the promoted property at
/// its zero value.
///
/// NOTE ON THE SHAPE: php-src rejects this program outright with
/// `Error: Call to private PrivOwner::__construct()`, because `newInstance()` enforces
/// constructor visibility and elephc does not yet. That separate gap is what makes the shape
/// reachable here at all. When it is closed this fixture should become a REJECTION test rather
/// than being deleted — the lookup it pins is still the one doing the work, and it is what
/// decides which class's constructor the visibility check will then be asked about.
#[test]
fn test_reflection_new_instance_resolves_an_inherited_private_constructor() {
    let out = compile_and_run(
        r#"<?php
class PrivReflOwner {
    private function __construct(public int $n = 6) {}
}
class PrivReflChild extends PrivReflOwner {}
$reflected = new ReflectionClass('PrivReflChild');
echo $reflected->newInstance()->n;
"#,
    );
    assert_eq!(out, "6");
}

/// An inherited PUBLIC constructor with defaults is unaffected — the owner walk stops at the
/// instantiated class whenever its own map HAS the entry, which a public inherited constructor
/// does, so this path resolves exactly as before.
#[test]
fn test_fixed_new_of_descendant_with_an_inherited_public_constructor_still_pads() {
    let out = compile_and_run(
        r#"<?php
class PublicOwner {
    public function __construct(public int $n = 3) {}
}
class PublicChild extends PublicOwner {}
echo (new PublicChild())->n, "|", (new PublicChild(8))->n;
"#,
    );
    assert_eq!(out, "3|8");
}

/// A descendant that REPLACES the inherited constructor keeps its own: the owner walk stops at
/// the first class whose map has the entry, which is the descendant itself.
#[test]
fn test_fixed_new_prefers_the_descendants_own_constructor_over_the_ancestors() {
    let out = compile_and_run(
        r#"<?php
class ShadowOwner {
    private function __construct(private int $n = 1) {}
    public function n(): int { return $this->n; }
}
class ShadowChild extends ShadowOwner {
    public function __construct(private int $m = 7) {}
    public function n(): int { return $this->m; }
}
echo (new ShadowChild())->n();
"#,
    );
    assert_eq!(out, "7");
}

/// Issue #869: `ReflectionClass::getConstructor()` on a descendant of a class with a PRIVATE
/// constructor returns a `ReflectionMethod` on the DECLARING class, as PHP does. It used to
/// answer `null`, because the descendant's own method map deliberately carries no entry.
#[test]
fn test_reflection_get_constructor_reports_the_declaring_class_for_an_inherited_private_ctor() {
    let out = compile_and_run(
        r#"<?php
class ReflPrivOwner { private function __construct() {} }
class ReflPrivChild extends ReflPrivOwner {}
$constructor = (new ReflectionClass('ReflPrivChild'))->getConstructor();
var_dump($constructor === null);
echo $constructor?->getName(), "|", $constructor?->getDeclaringClass()->getName(), "|";
var_dump($constructor?->isPrivate());
"#,
    );
    assert_eq!(out, "bool(false)\n__construct|ReflPrivOwner|bool(true)\n");
}

/// `isInstantiable()` follows from the same lookup: the inherited constructor is not public, so
/// PHP answers `false`. With no constructor member found at all it used to answer `true`.
#[test]
fn test_reflection_is_instantiable_is_false_for_an_inherited_private_constructor() {
    let out = compile_and_run(
        r#"<?php
class InstOwner { private function __construct() {} }
class InstChild extends InstOwner {}
class InstPubOwner { public function __construct() {} }
class InstPubChild extends InstPubOwner {}
var_dump(
    (new ReflectionClass('InstChild'))->isInstantiable(),
    (new ReflectionClass('InstOwner'))->isInstantiable(),
    (new ReflectionClass('InstPubChild'))->isInstantiable()
);
"#,
    );
    assert_eq!(out, "bool(false)\nbool(false)\nbool(true)\n");
}

/// The descendant's own method LIST is unchanged: PHP reports `method_exists()` as `false` and
/// `getMethods()` as empty for an inherited private constructor, and only `getConstructor()`
/// sees it. Pinning both halves is what keeps the #869 fix from over-reaching.
#[test]
fn test_an_inherited_private_constructor_stays_off_the_method_list() {
    let out = compile_and_run(
        r#"<?php
class ListOwner { private function __construct() {} }
class ListChild extends ListOwner {}
var_dump(method_exists('ListChild', '__construct'));
echo count((new ReflectionClass('ListChild'))->getMethods());
"#,
    );
    assert_eq!(out, "bool(false)\n0");
}

/// The eval bridge reads the same AOT reflection rows, so it must agree with the compiled side.
#[test]
fn test_eval_reflection_sees_an_inherited_private_constructor() {
    let out = compile_and_run(
        r#"<?php
class EvalOwner { private function __construct() {} }
class EvalChild extends EvalOwner {}
echo eval('return (new ReflectionClass("EvalChild"))->getConstructor()?->getDeclaringClass()->getName();'), "|";
var_dump(eval('return method_exists("EvalChild", "__construct");'));
"#,
    );
    assert_eq!(out, "EvalOwner|bool(false)\n");
}

/// Verifies the singleton shape this defect actually reached: private constructor, static
/// accessor, one subclass, and a call through the base type.
#[test]
fn test_singleton_subclass_runs_private_constructor_and_dispatches() {
    let out = compile_and_run(
        r#"<?php
class Root {
    public function __construct() {}
    public function alpha(): string { return "root-alpha"; }
    public function beta(): string { return "root-beta"; }
}
class Single extends Root {
    private static ?Single $instance = null;
    private function __construct() { parent::__construct(); echo "init "; }
    public static function get(): Single { return self::$instance ??= new self(); }
    public function alpha(): string { return "single-alpha"; }
}
function through_root(Root $value): string { return $value->beta(); }
$single = Single::get();
echo $single->alpha(), "|", through_root($single);
"#,
    );
    assert_eq!(out, "init single-alpha|root-beta");
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
