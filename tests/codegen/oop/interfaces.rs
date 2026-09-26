//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object-oriented PHP interfaces, including interface contract can be satisfied by concrete class, abstract base can defer method to concrete child, and class can implement multiple interfaces.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Uses checked-in example PHP fixtures through include_str! in addition to inline native-output assertions.

use super::*;

/// An explicit mixed implementation keeps the boxed ABI of an untyped interface parameter.
#[test]
fn test_untyped_interface_parameter_accepts_explicit_mixed() {
    let out = compile_and_run(
        r#"<?php
interface Sink { public function put($value, $fallback = null); }
class Box implements Sink {
    public function put(mixed $value, mixed $fallback = null) {
        echo gettype($value), gettype($fallback);
    }
}
function send(Sink $sink) { $sink->put(42); }
send(new Box());
"#,
    );
    assert_eq!(out, "integerNULL");
}

/// Interface dispatch boxes a typed object before entering a mixed implementation parameter.
#[test]
fn test_interface_object_parameter_widens_to_mixed() {
    let out = compile_and_run(
        r#"<?php
class User {}
interface Named { public function take(User $user, string $label); }
class Box implements Named {
    public function take(mixed $user, mixed $label) {
        echo gettype($user), gettype($label);
    }
}
function send(Named $sink, User $user) { $sink->take($user, "name"); }
$box = new Box();
$user = new User();
$box->take($user, "name");
send($box, $user);
"#,
    );
    assert_eq!(out, "objectstringobjectstring");
}

/// A returned mixed value remains alive after the adapter releases its argument cell.
#[test]
fn test_interface_widened_argument_can_be_returned() {
    let out = compile_and_run(
        r#"<?php
class User {}
interface Identity { public function keep(User $user): mixed; }
class Box implements Identity {
    public function keep(mixed $user): mixed { return $user; }
}
function send(Identity $box, User $user): mixed { return $box->keep($user); }
echo gettype(send(new Box(), new User()));
"#,
    );
    assert_eq!(out, "object");
}

/// A widened reference keeps caller storage through nested by-reference calls.
#[test]
fn test_interface_widened_reference_writes_through_nested_aliases() {
    let out = compile_and_run(
        r#"<?php
class User {}
interface Named { public function take(User &$user); }
class Box implements Named {
    public function take(mixed &$user) { $user = "changed"; }
}
class Exact implements Named {
    public function take(User &$user) { $user = new User(); }
}
function relay(Named $box, User &$user) { $box->take($user); }
$user = new User();
$alias =& $user;
relay(new Box(), $user);
$other = new User();
relay(new Exact(), $other);
echo $user, ":", $alias, ":", gettype($other);
"#,
    );
    assert_eq!(out, "changed:changed:object");
}

/// The declared object type is checked again when a widened reference is passed later.
#[test]
fn test_interface_widened_reference_rechecks_entry_type() {
    let out = compile_and_run(
        r#"<?php
class User {}
interface Named { public function take(User &$user); }
class Box implements Named {
    public function take(mixed &$user) { $user = "changed"; }
}
function relay(Named $box, User &$user) { $box->take($user); }
$user = new User();
relay(new Box(), $user);
try { relay(new Box(), $user); } catch (TypeError $error) { echo "caught"; }
"#,
    );
    assert_eq!(out, "caught");
}

/// Direct interface dispatch also enforces the declared object type after a reference retypes it.
#[test]
fn test_interface_widened_reference_direct_dispatch_rechecks_entry_type() {
    let out = compile_and_run(
        r#"<?php
class User {}
interface Named { public function take(User &$user); }
class Box implements Named {
    public function take(mixed &$user) { $user = "changed"; }
}
$box = new Box();
function dispatch(Named $box) {
    $user = new User();
    $box->take($user);
    try { $box->take($user); } catch (TypeError $error) { echo "caught"; }
}
dispatch($box);
"#,
    );
    assert_eq!(out, "caught");
}

/// A concrete method checks its declared object type after a reference changes type.
#[test]
fn test_concrete_object_reference_rechecks_after_mixed_write() {
    let out = compile_and_run(
        r#"<?php
class User { public string $name = "user"; }
class Admin extends User { public string $name = "admin"; }
function change(User &$user) { $user = "changed"; }
class Exact {
    public function take(User &$user) { echo $user->name; }
}
$exact = new Exact();
$valid = new Admin();
$exact->take($valid);
$user = new User();
change($user);
try { $exact->take($user); } catch (TypeError $error) { echo ":caught"; }
"#,
    );
    assert_eq!(out, "admin:caught");
}

/// Static calls retain the declared object check after physical ABI boxing.
#[test]
fn test_concrete_static_object_reference_rechecks_after_mixed_write() {
    let out = compile_and_run(
        r#"<?php
class User {}
function change(User &$user) { $user = "changed"; }
class Exact {
    public static function take(User &$user) { echo "entered"; }
}
$user = new User();
change($user);
try { Exact::take($user); } catch (TypeError $error) { echo "caught"; }
"#,
    );
    assert_eq!(out, "caught");
}

/// A typed function's entry check stays catchable when its body cannot throw.
#[test]
fn test_object_reference_function_entry_type_error_remains_catchable() {
    let out = compile_and_run(
        r#"<?php
class User {}
function change(User &$user) { $user = "changed"; }
function take(User &$user) { echo "entered"; }
$user = new User();
change($user);
try { take($user); } catch (TypeError $error) { echo "caught"; }
"#,
    );
    assert_eq!(out, "caught");
}

/// Constructor entry checks remain visible to catch analysis for a pure body.
#[test]
fn test_object_reference_constructor_entry_type_error_remains_catchable() {
    let out = compile_and_run(
        r#"<?php
class User {}
function change(User &$user) { $user = "changed"; }
class Exact {
    public function __construct(User &$user) { echo "entered"; }
}
$user = new User();
change($user);
try { new Exact($user); } catch (TypeError $error) { echo "caught"; }
"#,
    );
    assert_eq!(out, "caught");
}

/// The generic object hint checks its boxed reference payload on a later call.
#[test]
fn test_concrete_object_hint_reference_rechecks_after_type_change() {
    let out = compile_and_run(
        r#"<?php
class User {}
function change(object &$value) { $value = "changed"; }
class Exact {
    public function take(object &$value) { echo "entered"; }
}
$value = new User();
change($value);
$exact = new Exact();
try { $exact->take($value); } catch (TypeError $error) { echo "caught"; }
"#,
    );
    assert_eq!(out, "caught");
}

/// A typed implementation shares the canonical reference cell used by interface dispatch.
#[test]
fn test_interface_exact_object_reference_uses_shared_cell() {
    let out = compile_and_run(
        r#"<?php
class User { public string $name = "before"; }
interface Named { public function take(User &$user); }
class Exact implements Named {
    public function take(User &$user) { $user->name = "after"; }
}
function relay(Named $box, User &$user) { $box->take($user); }
$user = new User();
relay(new Exact(), $user);
echo $user->name;
"#,
    );
    assert_eq!(out, "after");
}

/// A static implementation can widen an object reference and update an existing alias.
#[test]
fn test_static_interface_widened_reference_updates_alias() {
    let out = compile_and_run(
        r#"<?php
class User {}
interface Named { public static function take(User &$user); }
class Box implements Named {
    public static function take(mixed &$user) { $user = "changed"; }
}
function relay(User &$user) { Box::take($user); }
$user = new User();
$alias =& $user;
relay($user);
echo $alias;
"#,
    );
    assert_eq!(out, "changed");
}

/// A typed reference can retype its caller after validating the incoming object.
#[test]
fn test_typed_reference_parameter_can_retype_caller() {
    let out = compile_and_run(
        r#"<?php
class User {}
function change(User &$user) { $user = "changed"; }
$user = new User();
change($user);
echo $user;
"#,
    );
    assert_eq!(out, "changed");
}

/// Keeps untyped interface defaults callable after concrete method parameters widen.
#[test]
fn test_untyped_interface_method_defaults_use_stable_boxed_abi() {
    let out = compile_and_run(
        r#"<?php
interface BindingContract {
    public function bind($abstract, $concrete = null, $shared = false);
}

class BindingContainer implements BindingContract {
    public function bind($abstract, $concrete = null, $shared = false) {
        echo $abstract;
    }
}

function invoke(BindingContract $container) {
    $result = $container->bind(42);
    echo gettype($result);
}

$container = new BindingContainer();
$container->bind("x", "value", true);
invoke($container);
"#,
    );
    assert_eq!(out, "x42NULL");
}

/// Verifies a concrete class can satisfy an interface contract by implementing all required methods.
/// Fixture: interface `Named` with method `name()`, concrete `User` implementing `Named`.
/// Asserts the method call on the concrete instance returns the expected string.
#[test]
fn test_interface_contract_can_be_satisfied_by_concrete_class() {
    let out = compile_and_run(
        r#"<?php
interface Named {
    public function name();
}

class User implements Named {
    public function name() {
        return "Ada";
    }
}

$user = new User();
echo $user->name();
"#,
    );
    assert_eq!(out, "Ada");
}

/// Verifies an abstract class can defer interface method implementation to a concrete child class.
/// Fixture: abstract `BaseGreeter` with abstract method `label()` and concrete `PersonGreeter`.
/// Asserts calling `greet()` on the concrete child triggers `label()` via `$this->label()`.
#[test]
fn test_abstract_base_can_defer_method_to_concrete_child() {
    let out = compile_and_run(
        r#"<?php
abstract class BaseGreeter {
    abstract public function label();

    public function greet() {
        return "hi " . $this->label();
    }
}

class PersonGreeter extends BaseGreeter {
    public function label() {
        return "world";
    }
}

$g = new PersonGreeter();
echo $g->greet();
"#,
    );
    assert_eq!(out, "hi world");
}

/// Verifies a class can implement multiple interfaces simultaneously.
/// Fixture: `Named` and `Tagged` interfaces, `Item` implementing both.
/// Asserts chained method calls resolve to the correct interface method on the same instance.
#[test]
fn test_class_can_implement_multiple_interfaces() {
    let out = compile_and_run(
        r#"<?php
interface Named {
    public function name();
}

interface Tagged {
    public function tag();
}

class Item implements Named, Tagged {
    public function name() {
        return "box";
    }

    public function tag() {
        return "BX";
    }
}

$item = new Item();
echo $item->name() . ":" . $item->tag();
"#,
    );
    assert_eq!(out, "box:BX");
}

/// Verifies a class can satisfy a static interface method contract.
///
/// Fixture: interface `StaticMaker` declares `public static make(...)`;
/// `StaticWidget` implements it. The test also checks ReflectionClass and
/// ReflectionMethod expose the method as static.
#[test]
fn test_static_interface_method_contract_is_supported() {
    let out = compile_and_run(
        r#"<?php
interface StaticMaker {
    public static function make(string $name): string;
}

class StaticWidget implements StaticMaker {
    public static function make(string $name): string {
        return "W:" . $name;
    }
}

echo StaticWidget::make("box");
echo ":";
$interface = new ReflectionClass(StaticMaker::class);
echo $interface->hasMethod("make") ? "H" : "h";
echo ":";
$listed = $interface->getMethods()[0];
echo $listed->getName();
echo ":";
echo $listed->isStatic() ? "S" : "s";
echo ":";
echo $listed->getNumberOfParameters();
echo ":";
$method = new ReflectionMethod(StaticMaker::class, "make");
echo $method->isStatic() ? "S" : "s";
echo ":";
echo $method->getName();
echo ":";
echo (new ReflectionClass(StaticWidget::class))->implementsInterface(StaticMaker::class) ? "Y" : "N";
"#,
    );
    assert_eq!(out, "W:box:H:make:S:1:S:make:Y");
}

/// Verifies an abstract class may defer a static interface method to a concrete child.
///
/// Fixture: `AbstractStaticLabel` implements `StaticLabel` but leaves the
/// static contract abstract; `ConcreteStaticLabel` provides it and is callable.
#[test]
fn test_abstract_class_can_defer_static_interface_method_to_child() {
    let out = compile_and_run(
        r#"<?php
interface StaticLabel {
    public static function label(): string;
}

abstract class AbstractStaticLabel implements StaticLabel {
}

class ConcreteStaticLabel extends AbstractStaticLabel {
    public static function label(): string {
        return "ready";
    }
}

echo ConcreteStaticLabel::label();
"#,
    );
    assert_eq!(out, "ready");
}

/// Verifies transitive interface extension is enforced: a class must satisfy the full chain.
/// Fixture: `Labeled extends Named`, `Product implements Labeled`. Uses `strtoupper($this->name())`.
/// Asserts the method call correctly resolves through the transitive interface hierarchy.
#[test]
fn test_transitive_interface_extends_is_enforced() {
    let out = compile_and_run(
        r#"<?php
interface Named {
    public function name();
}

interface Labeled extends Named {
    public function label();
}

class Product implements Labeled {
    public function name() {
        return "widget";
    }

    public function label() {
        return strtoupper($this->name());
    }
}

$product = new Product();
echo $product->label();
"#,
    );
    assert_eq!(out, "WIDGET");
}

/// Verifies the checked-in example at `examples/interfaces/main.php` compiles and runs end-to-end.
/// Loads the PHP fixture via `include_str!`, asserts stdout matches expected multi-line output.
#[test]
fn test_example_interfaces_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/interfaces/main.php"));
    // `isset(...) . "\n"`: a bool false stringifies to "" (not "0") in PHP, so the
    // post-unset isset line is empty.
    assert_eq!(out, "WIDGET\nproduct\nA-42\n1\n\n");
}

/// Verifies an interface with a read-only property (`get;`) can be satisfied by a concrete property.
/// Fixture: interface `HasId` with `public int $id { get; }`, concrete `User` with int field.
/// Asserts reading the property on the concrete instance returns the expected value.
#[test]
fn test_interface_get_property_contract_is_satisfied_by_concrete_property() {
    let out = compile_and_run(
        r#"<?php
interface HasId {
    public int $id { get; }
}

class User implements HasId {
    public int $id = 42;
}

$user = new User();
echo $user->id;
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies interface property setters allow contravariant type (subclass) in implementing class.
/// Fixture: `Dog extends Animal`, interface `DogSink` with `public Dog $pet { set; }`,
/// implementing `Kennel` declares `public Animal $pet`. Sets a `Dog` instance and checks `instanceof Animal`.
/// Asserts contravariant property types are accepted per PHP semantics.
#[test]
fn test_interface_set_property_contract_allows_contravariant_type() {
    let out = compile_and_run(
        r#"<?php
class Animal {}
class Dog extends Animal {}

interface DogSink {
    public Dog $pet { set; }
}

class Kennel implements DogSink {
    public Animal $pet;
}

$kennel = new Kennel();
$kennel->pet = new Dog();
echo $kennel->pet instanceof Animal;
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies an abstract class can defer interface property implementation to a concrete child.
/// Fixture: interface `HasName` with `string $name { get; set; }`, abstract `NamedBase implements HasName`,
/// concrete `Product extends NamedBase` with a default field initializer.
/// Asserts reading the property on the concrete child resolves via the abstract's interface contract.
#[test]
fn test_abstract_class_can_defer_interface_property_to_child() {
    let out = compile_and_run(
        r#"<?php
interface HasName {
    public string $name { get; set; }
}

abstract class NamedBase implements HasName {
}

class Product extends NamedBase {
    public string $name = "widget";
}

$product = new Product();
echo $product->name;
"#,
    );
    assert_eq!(out, "widget");
}

/// Verifies a PHP 8.3+ static interface method: an interface may declare a `static` method,
/// and an implementing class satisfies it with a static method, dispatched by class.
/// Fixture: interface `Previewable` with `static previews(): array`, final `C` implementing it.
#[test]
fn test_static_interface_method() {
    let out = compile_and_run(
        r#"<?php
interface Previewable {
    public static function previews(): array;
}

final class C implements Previewable {
    public static function previews(): array {
        return ['a', 'b', 'c'];
    }
}

echo implode(',', C::previews());
"#,
    );
    assert_eq!(out, "a,b,c");
}

/// Verifies a concrete child satisfies a static interface method when the interface is
/// implemented by an abstract parent class, and `#[\Override]` on the child's static
/// implementation resolves through the parent's inherited interfaces.
#[test]
fn test_static_interface_method_via_abstract_parent() {
    let out = compile_and_run(
        r#"<?php
interface Previewable {
    public static function previews(): array;
}

abstract class Base implements Previewable {
}

class C extends Base {
    #[\Override]
    public static function previews(): array {
        return ['x', 'y'];
    }
}

echo implode(',', C::previews());
"#,
    );
    assert_eq!(out, "x,y");
}

/// Verifies `#[\Override]` is accepted on a static interface-method implementation
/// (the override target is the interface's static method, matched via `InterfaceInfo.static_methods`).
#[test]
fn test_override_on_static_interface_method() {
    let out = compile_and_run(
        r#"<?php
interface Previewable {
    public static function previews(): array;
}

final class C implements Previewable {
    #[\Override]
    public static function previews(): array {
        return ['a', 'b'];
    }
}

echo implode(',', C::previews());
"#,
    );
    assert_eq!(out, "a,b");
}

/// An implementation may return a NARROWER type than the interface declares — the PSR-7 shape
/// `withX(): static` (resolving to the class) against an interface-typed return. The class
/// under validation is mid-construction when conformance runs, so the covariance is proven
/// from the conformance context itself. Byte-parity vs PHP 8.5.
#[test]
fn test_interface_covariant_self_return() {
    let out = compile_and_run(
        "<?php interface I { public function w(): I; } final class C implements I { public function w(): static { return $this; } } echo (new C())->w() instanceof C ? 'ok' : 'no';",
    );
    assert_eq!(out, "ok");
}

/// A static implementation may return its concrete class against an interface return contract.
#[test]
fn test_static_interface_covariant_self_return() {
    let out = compile_and_run(
        r#"<?php
interface Maker {
    public static function make(): Maker;
}
final class Product implements Maker {
    public static function make(): static { return new static(); }
}
echo Product::make() instanceof Product ? 'ok' : 'no';
"#,
    );
    assert_eq!(out, "ok");
}

/// Parent method returns the parent class; child may override with `static` / self (covariant).
#[test]
fn test_class_covariant_self_return_override() {
    let out = compile_and_run(
        "<?php class Base { public function w(): Base { return $this; } } class Child extends Base { public function w(): static { return $this; } } echo (new Child())->w() instanceof Child ? 'ok' : 'no';",
    );
    assert_eq!(out, "ok");
}

/// Verifies an inherited interface method returning `static` stays typed as the child interface.
#[test]
fn test_interface_late_static_return_stays_receiver() {
    let out = compile_and_run(
        r#"<?php
interface Message {
    public function withHeader(string $value): static;
}
interface Request extends Message {
    public function withMethod(string $method): static;
    public function method(): string;
}
final class Req implements Request {
    public function __construct(private string $method = 'GET') {}
    public function withHeader(string $value): static { return new static($this->method); }
    public function withMethod(string $method): static { return new static($method); }
    public function method(): string { return $this->method; }
}
function chain(Request $request): string {
    return $request->withHeader('x-trace')->withMethod('POST')->method();
}
echo chain(new Req());
"#,
    );
    assert_eq!(out, "POST");
}

/// Verifies an implementation may covariantly narrow `static|false` to `static`.
#[test]
fn test_interface_late_static_union_can_narrow_to_static() {
    let out = compile_and_run(
        r#"<?php
interface MaybeCopyable {
    public function copy(): static|false;
}
final class AlwaysCopyable implements MaybeCopyable {
    public function copy(): static { return $this; }
    public function label(): string { return "copy"; }
}
echo (new AlwaysCopyable())->copy()->label();
"#,
    );
    assert_eq!(out, "copy");
}

/// `??` over object operands keeps an object type: `?Contract ?? new Implementation()` is a
/// `Contract`, and two unrelated implementations are a union both of whose members satisfy a
/// declared `Contract`. It was typed `mixed`, so passing it to a `Contract` parameter or
/// returning it from a `Contract` function was refused. Regression for #822.
#[test]
fn test_null_coalesce_over_objects_keeps_the_object_type() {
    let out = compile_and_run(
        r#"<?php
interface Contract { public function name(): string; }
final class Implementation implements Contract { public function name(): string { return "impl"; } }
final class Other implements Contract { public function name(): string { return "other"; } }

final class Consumer
{
    public function __construct(public Contract $value) {}

    public static function make(?Contract $value = null): self
    {
        return new self($value ?? new Implementation());
    }
}

echo Consumer::make()->value->name(), "\n";
echo Consumer::make(new Other())->value->name(), "\n";

function pick(?Implementation $a, Other $b): Contract { return $a ?? $b; }
echo pick(null, new Other())->name(), " ", pick(new Implementation(), new Other())->name(), "\n";

function maybe(?Contract $c, ?Other $d): ?Contract { return $c ?? $d; }
var_dump(maybe(null, null));
echo maybe(null, new Other())->name(), "\n";
for ($i = 0; $i < 20; $i++) { $k = Consumer::make($i % 2 ? new Other() : null); }
echo $k->value->name(), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "impl\n",
            "other\n",
            "other impl\n",
            "NULL\n",
            "other\n",
            "other\n",
        )
    );
}
