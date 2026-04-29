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
