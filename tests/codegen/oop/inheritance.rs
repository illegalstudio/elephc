//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object-oriented PHP inheritance, including class protected members are accessible inside class methods, class protected static method is callable inside class, and inheritance dynamic dispatch uses child override.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies an overriding interface method may add an optional parameter while
/// preserving calls valid for the parent declaration.
#[test]
fn test_interface_override_may_add_optional_parameters() {
    let out = compile_and_run(
        r#"<?php
interface Lookup {
    public function find(string $key);
}

interface ConfigurableLookup extends Lookup {
    public function find(string $key, int $mode = 0);
}

class MemoryLookup implements ConfigurableLookup {
    public function find(string $key, int $mode = 0) {
        return $key . $mode;
    }
}

$lookup = new MemoryLookup();
echo $lookup->find("item");
"#,
    );
    assert_eq!(out, "item0");
}

/// Verifies a virtual call through an interface preserves the surplus positional
/// arguments consumed by the concrete implementation's argument introspection.
#[test]
fn test_interface_dispatch_propagates_argument_introspection_tail() {
    let out = compile_and_run(
        r#"<?php
interface Invocation {
    public function countPassed();
}

class CountingInvocation implements Invocation {
    public function countPassed() {
        return func_num_args();
    }
}

function invoke(Invocation $target) {
    return $target->countPassed(10, 20);
}

echo invoke(new CountingInvocation());
"#,
    );
    assert_eq!(out, "2");
}

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

/// Verifies explicit ancestor syntax may invoke an inherited instance constructor
/// while preserving the current child object as `$this`.
#[test]
fn test_explicit_ancestor_constructor_binds_current_instance() {
    let out = compile_and_run(
        r#"<?php
class ExplicitAncestorBase {
    protected string $value = "";

    public function __construct(string $value) {
        $this->value = $value;
    }
}

final class ExplicitAncestorChild extends ExplicitAncestorBase {
    public function __construct(string $value) {
        ExplicitAncestorBase::__construct($value);
    }

    public function value(): string {
        return $this->value;
    }
}

echo (new ExplicitAncestorChild("ok"))->value();
"#,
    );
    assert_eq!(out, "ok");
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

/// Verifies a parent scope can access protected instance, static, and method members declared by
/// a child because PHP grants protected access across both directions of one inheritance family.
#[test]
fn test_parent_scope_accesses_child_protected_members() {
    let out = compile_and_run(
        r#"<?php
class ParentBox {
    public function read(ChildBox $box): string {
        return $box->value . ":" . ChildBox::$staticValue . ":" . $box->label();
    }
}

class ChildBox extends ParentBox {
    protected string $value = "instance";
    protected static string $staticValue = "static";

    protected function label(): string {
        return "method";
    }
}

echo (new ParentBox())->read(new ChildBox());
"#,
    );
    assert_eq!(out, "instance:static:method");
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

/// Verifies a nullable base-class receiver dispatches a method declared only by its runtime child.
#[test]
fn test_nullable_base_receiver_dispatches_subclass_only_method() {
    let out = compile_and_run(
        r#"<?php
class NullableDispatchBase {}

class NullableDispatchChild extends NullableDispatchBase {
    public function childOnly(): string {
        return "child";
    }
}

function nullableDispatchFactory(bool $present): ?NullableDispatchBase {
    return $present ? new NullableDispatchChild() : null;
}

$value = nullableDispatchFactory(true);
echo $value->childOnly();
"#,
    );
    assert_eq!(out, "child");
}


/// A parent's PRIVATE members are reachable from the parent's own scope through a receiver typed
/// as a subclass: PHP resolves `private` by the lexical scope, not by the receiver's class.
///
/// Class flattening does not inherit private METHODS into a subclass's table (private properties
/// it does inherit), so such a call found nothing and the checker refused it with
/// `Undefined method: ChildCls::secret` where php prints `Sa`.
#[test]
fn test_private_member_resolves_in_the_declaring_scope_through_a_subclass_receiver() {
    let out = compile_and_run(
        r#"<?php
class ParentCls {
    private bool $flag = true;

    private function secret(string $s): string
    {
        return 'S' . $s;
    }

    public function viaSelf(): string
    {
        $other = new ChildCls();
        return $other->secret('a') . ':' . ($other->flag ? 'T' : 'F');
    }
}

class ChildCls extends ParentCls {}

echo (new ParentCls())->viaSelf();
"#,
    );
    assert_eq!(out, "Sa:T");
}

/// Verifies an ancestor-typed call reaches an override that declares MORE parameters with the
/// override's defaults, instead of reading the extra parameters from uninitialized registers.
///
/// Symfony's generated container overrides `Container::load(string $file)` as
/// `load($file, $lazyLoad = true)`, and `Container::make()` calls it through the base type: the
/// site passed one argument and the override crashed or saw garbage. A class that does not widen
/// the method keeps the ordinary virtual call.
#[test]
fn test_ancestor_typed_call_pads_a_wider_override() {
    let out = compile_and_run(
        r#"<?php
class Base {
    protected function load(string $file) { return "base:$file"; }
    public static function make(self $c, string $id) { return $c->load($id); }
}
class Child extends Base {
    protected function load($file, $lazyLoad = true, int $n = 7, ?array $opts = null) {
        return "child:$file:" . var_export($lazyLoad, true) . ":$n:" . var_export($opts, true);
    }
}
class GrandChild extends Child {}
class Plain extends Base {}
echo Base::make(new Child(), 'a'), "|", Base::make(new GrandChild(), 'b'), "|", Base::make(new Plain(), 'c');
"#,
    );
    assert_eq!(out, "child:a:true:7:NULL|child:b:true:7:NULL|base:c");
}

/// A child may redeclare a PRIVATE parent property with the other staticness: a private member
/// is not inherited, so `B::$enc` (instance) and `A::$enc` (static) are two properties, and the
/// same holds the other way round for `$inst`. `symfony/mime`'s `ParameterizedHeader` is this
/// shape. Expected output is php 8.5's; the build used to fail with "Cannot redeclare static
/// property as instance property".
#[test]
fn test_private_parent_property_may_be_redeclared_with_other_staticness() {
    let out = compile_and_run(
        r#"<?php
class A {
    private static $enc = "s";
    private $inst = "i";
    static function g() { return self::$enc; }
    function h() { return $this->inst; }
}
class B extends A {
    private $enc = "b";
    private static $inst = "t";
    function f() { return $this->enc . self::$inst; }
}
$b = new B;
echo A::g(), $b->h(), $b->f(), "\n";
"#,
    );
    assert_eq!(out, "sibt\n");
}

/// A child that shadows a private parent property with a CLASS-CONSTANT default is validated
/// like every other property default: the constant is typed once class constants exist. The
/// shadowing path used the plain validator and read `K::NONE` as a string ("default expects Int,
/// got Str"), which dropped `symfony/validator`'s `ClassMetadata` from the build. Expected output
/// is php 8.5's.
#[test]
fn test_private_parent_property_shadow_accepts_class_constant_default() {
    let out = compile_and_run(
        r#"<?php
class G { private int $s = K::NONE; function g() { return $this->s; } }
class C extends G { private int $s = K::CASCADE; function f() { return $this->s; } }
class K { public const NONE = 1; public const CASCADE = 2; }
$c = new C;
echo $c->f(), $c->g(), "\n";
"#,
    );
    assert_eq!(out, "21\n");
}

/// Verifies that converting an object to string calls the RUNTIME class's `__toString`: the
/// declared parameter type named a parent whose own `__toString` was called directly, so a
/// subclass override was silently skipped (`[plain]` for a `Fancy`).
#[test]
fn test_string_conversion_calls_the_subclass_tostring_override() {
    let out = compile_and_run(
        r#"<?php
class Plain {
    public function __toString(): string { return "plain"; }
}
class Fancy extends Plain {
    public function __toString(): string { return "fancy"; }
}
function show(Plain $p): string { return "[" . $p . "]"; }
function cast(Plain $p): string { return (string) $p; }
echo show(new Plain()), show(new Fancy()), cast(new Fancy());
"#,
    );
    assert_eq!(out, "[plain][fancy]fancy");
}

/// Verifies that an abstract class whose `__toString` comes only from `\Stringable` converts
/// through its subclass: the direct call named a method body the abstract class never has, and
/// the program failed to link.
#[test]
fn test_string_conversion_through_an_abstract_stringable_class() {
    let out = compile_and_run(
        r#"<?php
abstract class Shape implements \Stringable {
    abstract public function area(): int;
}
class Square extends Shape {
    public function __construct(private int $side) {}
    public function area(): int { return $this->side * $this->side; }
    public function __toString(): string { return "square " . $this->side; }
}
function describe(Shape $s): string { return (string) $s . "/" . $s->area(); }
echo describe(new Square(3));
"#,
    );
    assert_eq!(out, "square 3/9");
}

/// Verifies an undeclared `__toString()` return type is php's implicit `string`: it satisfies
/// `Stringable`, a user interface declaring `: string`, and a parent's declared override, and a
/// returned int is coerced like any `: string` function.
///
/// The checker refused all three shapes ("without declaring a compatible return type"), which
/// is how Laravel's `HtmlString`, `MessageBag`, `ViewErrorBag` and `Stringable` are written, and
/// each refusal left the class to the interpreter.
///
/// Reference value captured from php 8.5.10.
#[test]
fn test_undeclared_tostring_return_type_is_implicitly_string() {
    let out = compile_and_run(
        r#"<?php
interface Htmlable { public function toHtml(); }
interface Renderable { public function __toString(): string; }
class HtmlString implements Htmlable, Stringable {
    public function __construct(protected $html = '') {}
    public function toHtml() { return $this->html; }
    public function __toString() { return $this->toHtml(); }
}
class Bag implements Renderable {
    public function __toString() { return 'bag'; }
}
class Base { public function __toString(): string { return 'base'; } }
class Child extends Base { public function __toString() { return 'child'; } }
class Number { public function __toString() { return 5; } }
echo new HtmlString('<b>x</b>'), "|", new Bag(), "|", new Child(), "|", new Number(), "\n";
echo (new HtmlString('s')) instanceof Stringable ? "stringable" : "plain", "|";
echo (new ReflectionMethod('Number', '__toString'))->getReturnType(), "\n";
"#,
    );
    assert_eq!(out, "<b>x</b>|bag|child|5\nstringable|string\n");
}

/// Untyped members of an override family return in one storage, so a call typed by the base
/// reads what the override actually returned. A base that only throws was typed `void` while the
/// override returned a string (Laravel's `Facade::getFacadeAccessor()`), and a base returning a
/// string with an override returning an int read garbage.
#[test]
fn test_untyped_override_family_returns_share_one_storage() {
    let out = compile_and_run(
        r#"<?php
abstract class Base {
    protected static function accessor() { throw new RuntimeException('no accessor'); }
    protected function name() { throw new RuntimeException('no name'); }
    protected function v() { return "base"; }
    protected static function s() { return "base"; }
    public static function viaStatic() { return 'static:' . static::accessor() . ':' . var_export(static::s(), true); }
    public function viaThis() { return 'this:' . $this->name() . ':' . var_export($this->v(), true); }
}
class Impl extends Base {
    protected static function accessor() { return 'log'; }
    protected function name() { return 'impl'; }
    protected function v() { return 5; }
    protected static function s() { return 6; }
}
class Plain extends Base {
    protected static function accessor() { return 'plain'; }
    protected function name() { return 'p'; }
}
echo Impl::viaStatic(), " ", (new Impl)->viaThis(), "\n";
echo Plain::viaStatic(), " ", (new Plain)->viaThis(), "\n";
"#,
    );
    assert_eq!(out, "static:log:6 this:impl:5\nstatic:plain:'base' this:p:'base'\n");
}

/// A by-name allocation (`new $name`, `new` inside eval) of a class that hides a private
/// ancestor property runs the ancestor's private-default thunk. That thunk must write the
/// ancestor's own slots: it lacked a lexical class, so the private names counted as invisible,
/// the defaults went to a dynamic-property hash the object never had, and the worker segfaulted
/// (Symfony's `App\Kernel` under `--web`).
#[test]
fn test_by_name_allocation_initializes_hidden_private_ancestor_defaults() {
    let out = compile_and_run(
        r#"<?php
abstract class K {
    private ?string $warmupDir = null;
    private int $requestStackSize = 0;
    private bool $resetServices = false;
    public function info(): string {
        return var_export($this->warmupDir, true) . ' ' . $this->requestStackSize . ' ' . var_export($this->resetServices, true);
    }
}
class AppK extends K {
    private $requestStackSize = 'mine';
    public function mine(): string { return (string) $this->requestStackSize; }
}
$k = new AppK();
echo $k->info(), ' ', $k->mine(), "\n";
$name = 'AppK';
$e = eval('return new ' . $name . '();');
echo $e->info(), ' ', $e->mine(), "\n";
$d = new $name();
echo $d->info(), ' ', $d->mine(), "\n";
"#,
    );
    assert_eq!(out, "NULL 0 false mine\nNULL 0 false mine\nNULL 0 false mine\n");
}

/// A builtin parent's return type is TENTATIVE: overriding `DateTime::createFromFormat()`
/// (`DateTime|false`) with `?static`, as Carbon does, is a deprecation in php, not an error. The
/// checker refused it and Carbon went to the interpreter. The inherited builtin static method is
/// reached through `parent::` and through the static callable table, so its body must be lowered
/// although no `DateTime::createFromFormat()` call names it.
#[test]
fn test_override_of_a_builtin_static_method_with_a_tentative_return_type() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class MyDate extends DateTime {
    #[\ReturnTypeWillChange]
    public static function createFromFormat($format, $time, $timezone = null): ?static
    {
        $parent = parent::createFromFormat($format, $time, $timezone);
        if ($parent === false) {
            return null;
        }
        return new static('@' . $parent->getTimestamp());
    }
    public function format($format): string { return 'my:' . parent::format($format); }
}
$d = MyDate::createFromFormat('Y-m-d H:i:s', '2024-02-03 04:05:06', new DateTimeZone('UTC'));
echo get_class($d), " ", $d->format('Y-m-d'), "|";
var_dump(MyDate::createFromFormat('Y-m-d', 'nope'));
$fn = [MyDate::class, 'createFromFormat'];
echo get_class($fn('Y', '2020')), "|";
function via_parent(DateTime $x): string { return $x->format('Y'); }
echo via_parent($d);
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "MyDate my:2024-02-03|NULL\nMyDate|my:2024");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
