use crate::support::*;

#[test]
fn test_instanceof_classes_and_unknown_target() {
    let out = compile_and_run(
        r#"<?php
class A {}
class B {}
$a = new A();
echo ($a instanceof A) ? "T" : "F";
echo ($a instanceof B) ? "T" : "F";
echo (42 instanceof A) ? "T" : "F";
echo ($a instanceof Missing) ? "T" : "F";
"#,
    );
    assert_eq!(out, "TFFF");
}

#[test]
fn test_instanceof_inheritance_and_interfaces() {
    let out = compile_and_run(
        r#"<?php
interface Named {
    public function name();
}

interface Entity extends Named {
    public function id();
}

class Base {}

class User extends Base implements Entity {
    public function name() { return "user"; }
    public function id() { return 1; }
}

$user = new User();
$base = new Base();
echo ($user instanceof User) ? "T" : "F";
echo ($user instanceof Base) ? "T" : "F";
echo ($user instanceof Entity) ? "T" : "F";
echo ($user instanceof Named) ? "T" : "F";
echo ($base instanceof User) ? "T" : "F";
"#,
    );
    assert_eq!(out, "TTTTF");
}

#[test]
fn test_instanceof_self_parent_and_late_static() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public function check(Base $x) {
        echo ($x instanceof self) ? "S" : "s";
        echo ($x instanceof static) ? "T" : "t";
    }
}

class Child extends Base {
    public function checkParent(Base $x) {
        echo ($x instanceof parent) ? "P" : "p";
    }
}

$base = new Base();
$child = new Child();
$base->check($child);
$child->check($base);
$child->checkParent($child);
"#,
    );
    assert_eq!(out, "STStP");
}

#[test]
fn test_instanceof_lhs_evaluates_once() {
    let out = compile_and_run(
        r#"<?php
class Item {}

class Factory {
    public $count = 0;

    public function make() {
        $this->count = $this->count + 1;
        return new Item();
    }
}

$factory = new Factory();
echo ($factory->make() instanceof Item) ? "T" : "F";
echo $factory->count;
"#,
    );
    assert_eq!(out, "T1");
}

#[test]
fn test_instanceof_handles_mixed_and_nullable_object_values() {
    let out = compile_and_run(
        r#"<?php
interface Named {}
class User implements Named {}

function id(mixed $value): mixed {
    return $value;
}

function maybe(bool $flag): ?User {
    if ($flag) {
        return new User();
    }
    return null;
}

$mixedObject = id(new User());
$mixedScalar = id(7);
echo ($mixedObject instanceof User) ? "T" : "F";
echo ($mixedObject instanceof Named) ? "T" : "F";
echo ($mixedScalar instanceof User) ? "T" : "F";
echo (maybe(true) instanceof User) ? "T" : "F";
echo (maybe(false) instanceof User) ? "T" : "F";
"#,
    );
    assert_eq!(out, "TTFTF");
}

#[test]
fn test_trait_basic_method_import() {
    let out = compile_and_run(
        r#"<?php
trait Greeter {
    public function greet() { return "hello"; }
}
class Person {
    use Greeter;
}
$p = new Person();
echo $p->greet();
"#,
    );
    assert_eq!(out, "hello");
}

#[test]
fn test_trait_class_method_override_wins() {
    let out = compile_and_run(
        r#"<?php
trait Greeter {
    public function greet() { return "trait"; }
}
class Person {
    use Greeter;
    public function greet() { return "class"; }
}
$p = new Person();
echo $p->greet();
"#,
    );
    assert_eq!(out, "class");
}

#[test]
fn test_trait_insteadof_and_alias() {
    let out = compile_and_run(
        r#"<?php
trait A {
    public function label() { return "A"; }
}
trait B {
    public function label() { return "B"; }
}
class Box {
    use A, B {
        A::label insteadof B;
        B::label as bLabel;
    }
}
$b = new Box();
echo $b->label();
echo ":";
echo $b->bLabel();
"#,
    );
    assert_eq!(out, "A:B");
}

#[test]
fn test_trait_property_default_and_method_access() {
    let out = compile_and_run(
        r#"<?php
trait Counter {
    public $value = 7;
    public function read() { return $this->value; }
}
class Box {
    use Counter;
}
$b = new Box();
echo $b->read();
"#,
    );
    assert_eq!(out, "7");
}

#[test]
fn test_trait_can_use_another_trait() {
    let out = compile_and_run(
        r#"<?php
trait BaseGreeter {
    public function greet() { return "A"; }
}
trait FancyGreeter {
    use BaseGreeter;
    public function greetTwice() { return $this->greet() . "B"; }
}
class Person {
    use FancyGreeter;
}
$p = new Person();
echo $p->greetTwice();
"#,
    );
    assert_eq!(out, "AB");
}

#[test]
fn test_trait_static_method_import() {
    let out = compile_and_run(
        r#"<?php
trait Numbers {
    public static function one() { return 1; }
}
class Box {
    use Numbers;
}
echo Box::one();
"#,
    );
    assert_eq!(out, "1");
}

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

#[test]
fn test_trait_protected_alias_is_callable_inside_class() {
    let out = compile_and_run(
        r#"<?php
trait Greeter {
    public function greet() {
        return "hello";
    }
}

class Demo {
    use Greeter {
        Greeter::greet as protected innerGreet;
    }

    public function reveal() {
        return $this->innerGreet();
    }
}

$demo = new Demo();
echo $demo->reveal();
"#,
    );
    assert_eq!(out, "hello");
}

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

#[test]
fn test_inherited_constructor_specializes_base_string_property_type() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public $name;

    public function __construct($name) {
        $this->name = $name;
    }

    public function greet() {
        return $this->name;
    }
}

class Child extends Base {}

$child = new Child("Ada");
echo $child->greet();
"#,
    );
    assert_eq!(out, "Ada");
}

#[test]
fn test_array_literal_allows_sibling_objects_with_common_parent() {
    let out = compile_and_run(
        r#"<?php
class Animal {
    public $name;

    public function __construct($name) {
        $this->name = $name;
    }

    public function label() {
        return $this->name;
    }
}

class Dog extends Animal {}
class Cat extends Animal {}

$animals = [new Dog("Rex"), new Cat("Mia")];
foreach ($animals as $animal) {
    echo $animal->label() . " ";
}
"#,
    );
    assert_eq!(out, "Rex Mia ");
}

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

#[test]
fn test_example_interfaces_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../examples/interfaces/main.php"));
    assert_eq!(out, "WIDGET\n");
}

#[test]
fn test_match_without_default_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
$value = 3;
echo match($value) {
    1 => "one",
    2 => "two",
};
"#,
    );
    assert!(err.contains("unhandled match case"), "{err}");
}

#[test]
fn test_readonly_class_constructor_initialization() {
    let out = compile_and_run(
        r#"<?php
readonly class User {
    public $id;

    public function __construct($id) {
        $this->id = $id;
    }
}

$user = new User(42);
echo $user->id;
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_final_class_instantiates_and_dispatches_methods() {
    let out = compile_and_run(
        r#"<?php
final class Receipt {
    public $code = 41;

    public function next() {
        return $this->code + 1;
    }
}

$receipt = new Receipt();
echo $receipt->next();
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_final_method_dispatches_normally_without_override() {
    let out = compile_and_run(
        r#"<?php
class Base {
    final public function label() {
        return "base";
    }
}

class Child extends Base {
    public function suffix() {
        return "child";
    }
}

$child = new Child();
echo $child->label();
echo ":";
echo $child->suffix();
"#,
    );
    assert_eq!(out, "base:child");
}

#[test]
fn test_final_property_reads_normally_without_override() {
    let out = compile_and_run(
        r#"<?php
class Base {
    final public $value = 40;

    public function value() {
        return $this->value + 2;
    }
}

class Child extends Base {
    public function label() {
        return "answer:";
    }
}

$child = new Child();
echo $child->label();
echo $child->value();
"#,
    );
    assert_eq!(out, "answer:42");
}

#[test]
fn test_typed_properties_defaults_constructor_assignment_and_nullable() {
    let out = compile_and_run(
        r#"<?php
class User {
    public int $id;
    public string $name = "Ada";
    public ?string $email = null;

    public function __construct($id) {
        $this->id = $id;
    }

    public function label() {
        return $this->name . ":" . $this->id;
    }
}

$user = new User(42);
echo $user->label();
echo ":";
echo is_null($user->email);
$user->email = "ada@example.test";
echo ":";
echo $user->email;
"#,
    );
    assert_eq!(out, "Ada:42:1:ada@example.test");
}

#[test]
fn test_example_final_classes_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../examples/final-classes/main.php"));
    assert_eq!(out, "invoice:42\n");
}

#[test]
fn test_example_typed_properties_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../examples/typed-properties/main.php"));
    assert_eq!(out, "Ada:42\nmissing email\n");
}

#[test]
fn test_first_class_callable_named_function_indirect_call() {
    let out = compile_and_run(
        r#"<?php
function triple($n) {
    return $n * 3;
}

$fn = triple(...);
echo $fn(7);
"#,
    );
    assert_eq!(out, "21");
}

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

#[test]
fn test_first_class_callable_builtin_used_in_array_map() {
    let out = compile_and_run(
        r#"<?php
$len = strlen(...);
echo $len("tool");
"#,
    );
    assert_eq!(out, "4");
}

#[test]
fn test_first_class_callable_preserves_by_ref_params() {
    let out = compile_and_run(
        r#"<?php
function bump(&$n) {
    $n = $n + 1;
}

$fn = bump(...);
$value = 7;
$fn($value);
echo $value;
"#,
    );
    assert_eq!(out, "8");
}

#[test]
fn test_first_class_callable_alias_preserves_by_ref_params() {
    let out = compile_and_run(
        r#"<?php
function bump(&$n) {
    $n = $n + 1;
}

$f = bump(...);
$g = $f;
$value = 7;
$g($value);
echo $value;
"#,
    );
    assert_eq!(out, "8");
}

#[test]
fn test_closure_alias_preserves_by_ref_params() {
    let out = compile_and_run(
        r#"<?php
$f = function (&$x) {
    $x = $x + 1;
};

$g = $f;
$value = 7;
$g($value);
echo $value;
"#,
    );
    assert_eq!(out, "8");
}

#[test]
fn test_first_class_callable_variable_used_in_array_map() {
    let out = compile_and_run(
        r#"<?php
function double($n) {
    return $n * 2;
}

$fn = double(...);
$values = array_map($fn, [1, 2, 3]);
echo $values[0];
echo ":";
echo $values[2];
"#,
    );
    assert_eq!(out, "2:6");
}

#[test]
fn test_first_class_callable_untyped_function_accepts_string_args() {
    let out = compile_and_run(
        r#"<?php
function greet($name) {
    return "Hello " . $name;
}

$f = greet(...);
echo $f("World");
"#,
    );
    assert_eq!(out, "Hello World");
}

#[test]
fn test_first_class_callable_direct_call_user_func() {
    let out = compile_and_run(
        r#"<?php
echo call_user_func(strlen(...), "hello");
"#,
    );
    assert_eq!(out, "5");
}

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

#[test]
fn test_call_user_func_first_class_callable_preserves_by_ref_params() {
    let out = compile_and_run(
        r#"<?php
function bump(&$n) {
    $n = $n + 1;
}

$f = bump(...);
$value = 5;
call_user_func($f, $value);
echo $value;
"#,
    );
    assert_eq!(out, "6");
}

#[test]
fn test_call_user_func_closure_alias_preserves_by_ref_params() {
    let out = compile_and_run(
        r#"<?php
$f = function (&$x) {
    $x = $x + 1;
};
$g = $f;
$value = 5;
call_user_func($g, $value);
echo $value;
"#,
    );
    assert_eq!(out, "6");
}

#[test]
fn test_instance_method_preserves_multiple_byref_array_params() {
    let out = compile_and_run(
        r#"<?php
class Foo {
    public function bar(array &$a, array &$b): void {
        $a[0] = 1;
        $b[0] = 2;
    }
}

$x = [0];
$y = [0];
$foo = new Foo();
$foo->bar($x, $y);
echo $x[0];
echo $y[0];
"#,
    );
    assert_eq!(out, "12");
}

#[test]
fn test_first_class_callable_variadic_function_call() {
    let out = compile_and_run(
        r#"<?php
function count_args(...$xs) {
    echo count($xs);
}

$f = count_args(...);
$f(1, 2, 3);
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_closure_variadic_call() {
    let out = compile_and_run(
        r#"<?php
$f = function (...$xs) {
    echo count($xs);
};

$f(1, 2, 3);
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_first_class_callable_variadic_with_regular_param() {
    let out = compile_and_run(
        r#"<?php
function head_and_count($a, ...$rest) {
    echo $a;
    echo ":";
    echo count($rest);
}

$f = head_and_count(...);
$f(7, 8, 9);
"#,
    );
    assert_eq!(out, "7:2");
}

#[test]
fn test_first_class_callable_builtin_count_accepts_string_arrays() {
    let out = compile_and_run(
        r#"<?php
$f = count(...);
$xs = ["a", "b"];
echo $f($xs);
"#,
    );
    assert_eq!(out, "2");
}

#[test]
fn test_first_class_callable_builtin_count_accepts_assoc_arrays() {
    let out = compile_and_run(
        r#"<?php
$f = count(...);
$xs = ["a" => 1, "b" => 2];
echo $f($xs);
"#,
    );
    assert_eq!(out, "2");
}

#[test]
fn test_example_v017_trio_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../examples/v017-trio/main.php"));
    assert_eq!(out, "health:[ok]:missing");
}

#[test]
fn test_union_typed_local_gettype_and_reassignment() {
    let out = compile_and_run(
        r#"<?php
function demo() {
    int|string $value = 1;
    echo gettype($value);
    echo ":";
    $value = "two";
    echo gettype($value);
    echo ":";
    echo $value;
}

demo();
"#,
    );
    assert_eq!(out, "integer:string:two");
}

#[test]
fn test_nullable_typed_local_null_coalesce() {
    let out = compile_and_run(
        r#"<?php
function demo() {
    ?int $value = null;
    echo $value ?? 41;
    $value = 1;
    echo $value ?? 41;
}

demo();
"#,
    );
    assert_eq!(out, "411");
}

#[test]
fn test_union_typed_local_truthiness_dispatch() {
    let out = compile_and_run(
        r#"<?php
function demo() {
    int|string $value = "0";
    if ($value) {
        echo 1;
    } else {
        echo 0;
    }
    $value = 7;
    if ($value) {
        echo 1;
    } else {
        echo 0;
    }
}

demo();
"#,
    );
    assert_eq!(out, "01");
}

#[test]
fn test_union_typed_local_empty_dispatch() {
    let out = compile_and_run(
        r#"<?php
function demo() {
    int|string $value = "0";
    echo empty($value) ? 1 : 0;
    $value = "7";
    echo empty($value) ? 1 : 0;
}

demo();
"#,
    );
    assert_eq!(out, "10");
}
