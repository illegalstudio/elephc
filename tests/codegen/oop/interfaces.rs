//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object-oriented PHP interfaces, including interface contract can be satisfied by concrete class, abstract base can defer method to concrete child, and class can implement multiple interfaces.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Uses checked-in example PHP fixtures through include_str! in addition to inline native-output assertions.

use super::*;

/// Verifies an UNTYPED parameter on a bodyless declaration stays `mixed`. The compiler starts an
/// untyped parameter at `int` and lets a concrete body refine it from its own call sites, but an
/// interface method has no body to refine from, so the starting point froze and became the
/// contract: a program that logged an int first then rejected the string call with
/// "parameter $level expects Int, got Str". PSR-3 declares `log($level, …)` exactly this way and
/// Symfony's `ErrorListener` calls it with a string level. PHP outputs "int:a|string:b|".
#[test]
fn test_untyped_interface_parameter_stays_mixed() {
    let out = compile_and_run(
        r#"<?php
interface Log {
    public function log($level, string $message): void;
}

abstract class BaseLog implements Log {
    abstract public function describe($level): string;
}

class Plain extends BaseLog {
    public function log($level, string $message): void {
        echo get_debug_type($level), ':', $message, '|';
    }

    public function describe($level): string { return get_debug_type($level); }
}

function emit(Log $logger, string $level): void {
    $logger->log($level, 'b');
}

$l = new Plain();
$l->log(3, 'a');
emit($l, 'critical');
echo $l->describe(1), ',', $l->describe('x');
"#,
    );
    assert_eq!(out, "int:a|string:b|int,string");
}

/// Verifies virtual/interface calls forward the hidden argc operand used by an overriding method.
#[test]
fn test_interface_dispatch_forwards_hidden_func_get_args_count() {
    let out = compile_and_run(
        r#"<?php
interface ArityLoader { public function load(mixed $resource, ?string $type = null): mixed; }
abstract class ArityLoaderBase implements ArityLoader {
    public function proxy(): int { return $this->load('base', null); }
}
final class ArityLoaderImpl extends ArityLoaderBase {
    public function load(mixed $resource, ?string $type = null): mixed {
        return func_num_args();
    }
}
function invokeArityLoader(ArityLoader $loader): int { return $loader->load('iface', null); }
$loader = new ArityLoaderImpl();
echo $loader->proxy(), '|', invokeArityLoader($loader);
"#,
    );
    assert_eq!(out, "2|2");
}

/// Verifies a generic `object` receiver dispatches a method over closed-world class candidates.
#[test]
fn test_generic_object_receiver_dispatches_known_method_candidate() {
    let out = compile_and_run(
        r#"<?php
final class GenericObjectStack {
    private array $values = [];
    public function push(int $value): void { $this->values[] = $value; }
    public function count(): int { return count($this->values); }
}
function eraseStackType(GenericObjectStack $stack): object { return $stack; }
$stack = eraseStackType(new GenericObjectStack());
$stack->push(4);
echo $stack->count();
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies an impossible narrowed-method branch compiles and remains unexecuted.
#[test]
fn test_impossible_instanceof_narrowed_method_branch_compiles() {
    let out = compile_and_run(
        r#"<?php
interface OptionalWarmer { public function warmUp(string $directory): array; }
class NonWarmableKernel {}
function optionallyWarm(NonWarmableKernel $kernel): array {
    return $kernel instanceof OptionalWarmer ? $kernel->warmUp('/tmp') : [];
}
echo 'cold';
"#,
    );
    assert_eq!(out, "cold");
}

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

/// Direct interface dispatch checks the IMPLEMENTATION's parameter type, not the interface's: the
/// second `take()` reaches `Box::take(mixed &$user)` with the string the first one wrote, which
/// php accepts without a TypeError (host PHP 8.5 prints nothing). It used to expect "caught".
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
    assert_eq!(out, "");
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

/// Verifies object syntax dispatches a static interface method to the concrete receiver class.
#[test]
fn test_static_interface_method_dispatch_through_object_receiver() {
    let out = compile_and_run(
        r#"<?php
interface RuntimeStaticLabel {
    public static function label(string $suffix = "!"): string;
}
final class RuntimeStaticA implements RuntimeStaticLabel {
    public static function label(string $suffix = "!"): string { return "A" . $suffix; }
}
final class RuntimeStaticB implements RuntimeStaticLabel {
    public static function label(string $suffix = "!"): string { return "B" . $suffix; }
}
function runtimeStaticLabel(RuntimeStaticLabel $value): string {
    return $value->label("?");
}
echo runtimeStaticLabel(new RuntimeStaticA()), "|", runtimeStaticLabel(new RuntimeStaticB());
"#,
    );
    assert_eq!(out, "A?|B?");
}

/// Verifies static-syntax calls through an object invoke a static-only method.
///
/// PHP accepts `$object::method()` as well as `Class::method()`. The lowering
/// must not send this shape through the instance-only runtime callable-array
/// lookup when the receiver's class and its static method are known.
#[test]
fn test_static_method_dispatch_through_object_static_syntax() {
    let out = compile_and_run(
        r#"<?php
final class ObjectStaticSyntax {
    public static function label(string $suffix): string { return "static" . $suffix; }
}
function objectStaticSyntax(ObjectStaticSyntax $value): string {
    return $value::label("!");
}
echo objectStaticSyntax(new ObjectStaticSyntax());
"#,
    );
    assert_eq!(out, "static!");
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
    assert_eq!(out, "WIDGET\nproduct\nWIDGET\nwidget\ngadget\nunnamed\nA-42\n1\n\n");
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

/// Verifies covariant object returns do not depend on class metadata construction order.
#[test]
fn test_class_covariant_sibling_return_before_result_metadata() {
    let out = compile_and_run(
        r#"<?php
interface ResultContract {}
class ConcreteResult implements ResultContract {}
class BaseResult {}
class NarrowResult extends BaseResult {}
class ParentFactory {
    public function create(): BaseResult { return new BaseResult(); }
    public function createContract(): ResultContract { return new ConcreteResult(); }
}
class ChildFactory extends ParentFactory {
    public function create(): NarrowResult { return new NarrowResult(); }
    public function createContract(): ConcreteResult { return new ConcreteResult(); }
}
$factory = new ChildFactory();
echo ($factory->create() instanceof NarrowResult ? 'class' : 'bad');
echo '|';
echo ($factory->createContract() instanceof ConcreteResult ? 'interface' : 'bad');
"#,
    );
    assert_eq!(out, "class|interface");
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

/// Narrowed interface dispatch excludes unrelated same-name methods with incompatible ABIs.
#[test]
fn test_narrowed_interface_method_filters_unrelated_candidates() {
    let out = compile_and_run(
        r#"<?php
interface Runner {}
class StringRunner implements Runner {
    public function run(string $value): string { return $value; }
}
class Unrelated {
    public function run(?int $value): string { return 'wrong'; }
}
function invoke(Runner $runner): string {
    if ($runner instanceof StringRunner) {
        return $runner->run('ok');
    }
    return 'miss';
}
echo invoke(new StringRunner());
"#,
    );
    assert_eq!(out, "ok");
}


/// A class implements PHP's `Serializable`, which elephc never declared.
///
/// `TENTATIVE_RETURN_TYPE_METHODS` already named the interface, but it was absent from
/// `BUILTIN_INTERFACE_NAMES`, so a class implementing it sent the interpreter to `spl_autoload` —
/// and Composer cannot autoload a PHP built-in. `Twig\Profiler\Profile` is
/// `final class Profile implements \IteratorAggregate, \Serializable`, so the whole declaration
/// was refused with `class Twig\Profiler\Profile could not be declared`, killing the `--web`
/// worker while Symfony rendered its error page.
///
/// Deprecated since PHP 8.1 and still declared by the engine, which is what matters: userland code
/// implements it. Both its methods carry tentative return types, so the implementer below declares
/// a narrower `string` for `serialize()` and no type at all for `unserialize()`'s parameter.
///
/// Oracle: `php -n` prints the asserted line.
#[test]
fn test_class_implements_the_serializable_builtin_interface() {
    let out = compile_and_run(
        r#"<?php
final class Profile implements \IteratorAggregate, \Serializable {
    private array $items = ['a', 'b'];
    public function getIterator(): \Traversable { return new \ArrayIterator($this->items); }
    public function serialize(): string { return \serialize($this->__serialize()); }
    public function unserialize($data): void { $this->__unserialize(\unserialize($data)); }
    public function __serialize(): array { return [$this->items]; }
    public function __unserialize(array $data): void { [$this->items] = $data; }
}
$p = new Profile();
echo ($p instanceof Serializable ? 'yes' : 'no'), '|';
foreach ($p as $v) { echo $v; }
echo '|', $p->serialize();
"#,
    );
    assert_eq!(out, "yes|ab|a:1:{i:0;a:2:{i:0;s:1:\"a\";i:1;s:1:\"b\";}}");
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

/// `??` whose left operand can never be null keeps that operand's type: `find()` declares
/// `Implementation|false`, so `find() ?? new Other()` is still `Implementation|false` and may be
/// returned from a function declaring it, and so are the same calls through an instance and a
/// static method. The unreachable `Other` default made the result `Implementation|false|Other`,
/// which the return check refused. Regression for #1462.
#[test]
fn test_null_coalesce_drops_an_unreachable_object_default() {
    let out = compile_and_run(
        r#"<?php
interface Contract { public function name(): string; }
final class Implementation implements Contract { public function name(): string { return "impl"; } }
final class Other implements Contract { public function name(): string { return "other"; } }
final class Repo {
    public function find(bool $ok): Implementation|false { return $ok ? new Implementation() : false; }
    public static function first(bool $ok): Implementation|false { return $ok ? new Implementation() : false; }
}

function find(bool $ok): Implementation|false { return $ok ? new Implementation() : false; }
function lookup(bool $ok): Implementation|false { return find($ok) ?? new Other(); }
function viaMethod(Repo $repo, bool $ok): Implementation|false { return $repo->find($ok) ?? new Other(); }
function viaStatic(bool $ok): Implementation|false { return Repo::first($ok) ?? new Other(); }
function label(bool $ok): string { $found = find($ok) ?? new Other(); return $found === false ? "none" : $found->name(); }

var_dump(lookup(false));
echo lookup(true)->name(), "\n";
var_dump(viaMethod(new Repo(), false));
echo viaMethod(new Repo(), true)->name(), " ", viaStatic(true)->name(), "\n";
var_dump(viaStatic(false));
echo label(false), " ", label(true), "\n";
echo (new Implementation() ?? new Other())->name(), "\n";
"#,
    );
    assert_eq!(
        out,
        "bool(false)\nimpl\nbool(false)\nimpl impl\nbool(false)\nnone impl\nimpl\n"
    );
}

/// `??` over an arm that mixes an object with a scalar keeps a union, so narrowing away the
/// scalars leaves the object type: `?(Contract|int) ?? new Implementation()` is `Contract|int`,
/// and after `is_int()` it passes as a `Contract`; with a string default the join is
/// `Contract|int|string`. Both joins were `mixed`, which the `Contract` parameter refused.
/// Regression for #1463.
#[test]
fn test_null_coalesce_keeps_object_and_scalar_arms_as_a_union() {
    let out = compile_and_run(
        r#"<?php
interface Contract { public function name(): string; }
final class Implementation implements Contract { public function name(): string { return "impl"; } }
final class Other implements Contract { public function name(): string { return "other"; } }
function contractOnly(Contract $c): string { return $c->name(); }
function run(Contract|int|null $v): string {
    $r = $v ?? new Implementation();
    if (is_int($r)) { return "int " . $r; }
    return contractOnly($r);
}
function label(Contract|int|null $v): string {
    $r = $v ?? "none";
    if (is_string($r)) { return $r; }
    if (is_int($r)) { return "int " . $r; }
    return contractOnly($r);
}
echo run(null), " ", run(4), " ", run(new Other()), "\n";
echo label(null), " ", label(5), " ", label(new Other()), "\n";
"#,
    );
    assert_eq!(out, "impl int 4 other\nnone int 5 other\n");
}

/// Verifies an interface call reaches an implementation that inserts defaulted parameters while
/// every frame carries the generated `func_get_args()` collector.
///
/// Doctrine's `EntityManager::find($className, $id, LockMode|int|null $lockMode = null,
/// ?int $lockVersion = null)` implements `ObjectManager::find(string $className, mixed $id)`.
/// The interface-slot adapter has to box the differing prefix (`string` into an untyped
/// parameter), materialize each inserted default (a boxed null and a tagged-scalar null), and
/// forward the caller's own collector; the planner refused the shape and symfony/demo did not
/// build. A decorator forwarding through the interface and a direct call with every argument
/// are checked too. Expected output from php 8.5; the heap stays clean.
#[test]
fn test_interface_call_reaches_an_implementation_with_inserted_defaults() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class LockMode { public string $n = "pessimistic"; }
interface ObjectManager {
    public function find(string $className, mixed $id): object|null;
}
class EntityManager implements ObjectManager {
    public function find($className, mixed $id, LockMode|int|null $lockMode = null, int|null $lockVersion = null): object|null {
        $o = new stdClass();
        $o->desc = $className . ":" . $id . ":" . var_export($lockMode, true) . ":" . var_export($lockVersion, true) . ":" . count(func_get_args());
        return $o;
    }
}
class Decorator implements ObjectManager {
    public function __construct(private ObjectManager $inner) {}
    public function find(string $className, mixed $id): object|null { return $this->inner->find($className, $id); }
}
function lookup(ObjectManager $om, string $class, int $id): string {
    return $om->find($class, $id)->desc;
}
echo lookup(new EntityManager(), "App\\Post", 7), "|";
echo lookup(new Decorator(new EntityManager()), "App\\Tag", 3), "|";
$direct = new EntityManager();
echo $direct->find("App\\User", 1, 2, 5)->desc, "|";
eval('echo "eval";');
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "App\\Post:7:NULL:NULL:2|App\\Tag:3:NULL:NULL:2|App\\User:1:2:5:4|eval"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An interface declaring `__get($key)` / `__isset($key)` (Laravel's `Enumerable`) and a trait
/// implementing them. The implementation's first parameter is typed `string`, as php's magic
/// contract says; the interface entry kept `mixed`, and with the frame collector every method
/// carries once the program contains an `eval()`, the method-table adapter had to UNBOX the name
/// -- which it cannot -- so the build failed on `Collection::__get`. Expected output from php 8.5.
#[test]
fn test_interface_magic_get_reaches_a_trait_implementation_through_the_contract() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
interface Enum { public function __get($key); public function __isset($key); }
trait Proxies {
    public function __get($key) { return $key === 'sum' ? array_sum($this->items) : 'no ' . $key; }
    public function __isset($key) { return $key === 'sum'; }
}
class Coll implements Enum {
    use Proxies;
    public function __construct(public array $items) {}
}
function via(Enum $e): string { return $e->__get("sum") . "," . $e->__get("other") . "," . ($e->__isset("sum") ? "y" : "n"); }
echo via(new Coll([1, 2, 3]));
if ($argc > 5) { eval('echo 1;'); }
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "6,no other,y");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
