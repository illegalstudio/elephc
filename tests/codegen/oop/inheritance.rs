//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object-oriented PHP inheritance, including class protected members are accessible inside class methods, class protected static method is callable inside class, and inheritance dynamic dispatch uses child override.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

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

#[test]
fn test_property_redeclaration_adds_readonly() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public int $value = 0;
}

class Child extends Base {
    public readonly int $value;

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

#[test]
fn test_property_redeclaration_widens_visibility_and_adds_readonly() {
    let out = compile_and_run(
        r#"<?php
class Base {
    protected int $value = 0;

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
