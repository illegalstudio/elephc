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
