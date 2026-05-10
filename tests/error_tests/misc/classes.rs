//! Purpose:
//! Integration or regression tests for diagnostic coverage of misc classes, including instanceof self outside class scope, undefined class, and undefined property.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

#[test]
fn test_error_instanceof_self_outside_class_scope() {
    expect_error(
        "<?php class A {} $a = new A(); echo $a instanceof self;",
        "Cannot use self in instanceof outside of a class context",
    );
}

#[test]
fn test_error_undefined_class() {
    expect_error("<?php $x = new Missing();", "Undefined class: Missing");
}

#[test]
fn test_error_undefined_property() {
    expect_error(
        "<?php class Box {} $b = new Box(); echo $b->missing;",
        "Undefined property: Box::missing",
    );
}

#[test]
fn test_error_undefined_method() {
    expect_error(
        "<?php class Box {} $b = new Box(); $b->missing();",
        "Undefined method: Box::missing",
    );
}

#[test]
fn test_error_nullsafe_property_rejects_scalar_receiver() {
    expect_error(
        "<?php ?int $value = null; echo $value?->missing;",
        "Nullsafe property access requires an object or null",
    );
}

#[test]
fn test_error_nullsafe_method_rejects_scalar_receiver() {
    expect_error(
        "<?php ?int $value = null; $value?->missing();",
        "Nullsafe method call requires an object or null",
    );
}

#[test]
fn test_error_nullsafe_first_class_callable_is_rejected() {
    expect_error(
        "<?php class Box { public function run() {} } $b = new Box(); $fn = $b?->run(...);",
        "Cannot combine nullsafe operator with Closure creation",
    );
}

#[test]
fn test_error_nullsafe_assignment_target_is_rejected() {
    expect_error(
        "<?php class Profile {} class User { public ?Profile $profile; } $user = new User(); $user?->profile = new Profile();",
        "Invalid assignment target",
    );
}

#[test]
fn test_error_private_access() {
    expect_error(
        "<?php class Secret { private $value = 7; } $s = new Secret(); echo $s->value;",
        "Cannot access private property: Secret::value",
    );
}

#[test]
fn test_error_readonly_assign() {
    expect_error(
        "<?php class User { public readonly $id; public function __construct($id) { $this->id = $id; } } $u = new User(1); $u->id = 2;",
        "Cannot assign to readonly property outside constructor: User::id",
    );
}

#[test]
fn test_error_typed_property_rejects_invalid_default() {
    expect_error(
        "<?php class Box { public int $value = \"bad\"; }",
        "Property Box::$value default expects Int, got Str",
    );
}

#[test]
fn test_error_typed_property_rejects_invalid_assignment() {
    expect_error(
        "<?php class Box { public int $value; } $b = new Box(); $b->value = \"bad\";",
        "Property Box::$value expects Int, got Str",
    );
}

#[test]
fn test_error_typed_property_rejects_constructor_assignment_from_untyped_param() {
    expect_error(
        r#"<?php
class Box {
    public int $value;
    public function __construct($value) {
        $this->value = $value;
    }
}
$box = new Box("bad");
"#,
        "Property Box::$value expects Int, got Str",
    );
}

#[test]
fn test_error_constructor_promotion_outside_constructor() {
    expect_error(
        "<?php class Box { public function set(public int $value) {} }",
        "Cannot declare promoted property outside a constructor",
    );
}

#[test]
fn test_error_constructor_promotion_redeclares_property() {
    expect_error(
        "<?php class Box { public int $value; public function __construct(public int $value) {} }",
        "Cannot redeclare promoted property $value",
    );
    expect_error(
        "<?php class Box { public function __construct(public int $value) {} public int $value; }",
        "Cannot redeclare property $value",
    );
}

#[test]
fn test_error_constructor_promotion_rejects_variadic() {
    expect_error(
        "<?php class Box { public function __construct(public ...$values) {} }",
        "Cannot declare variadic promoted property",
    );
}

#[test]
fn test_error_constructor_promotion_rejects_readonly_by_reference() {
    expect_error(
        "<?php class Box { public function __construct(public readonly int &$value) {} }",
        "Readonly promoted by-reference properties are not supported",
    );
}

#[test]
fn test_error_constructor_promotion_rejects_by_reference_default() {
    expect_error(
        "<?php class Box { public function __construct(public int &$value = 1) {} }",
        "Promoted by-reference properties cannot use default values yet",
    );
}

#[test]
fn test_error_constructor_promotion_by_reference_requires_variable_arg() {
    expect_error(
        "<?php class Box { public function __construct(public int &$value) {} } $box = new Box(1);",
        "Constructor 'Box::__construct' parameter $value must be passed a variable",
    );
}

#[test]
fn test_error_constructor_promotion_rejects_abstract_constructor() {
    expect_error(
        "<?php abstract class Box { abstract public function __construct(public int $value); }",
        "Cannot declare promoted property in an abstract constructor",
    );
}

#[test]
fn test_error_typed_property_rejects_void_type() {
    expect_error(
        "<?php class Box { public void $value; }",
        "Property Box::$value cannot use type void",
    );
}

#[test]
fn test_error_typed_property_rejects_callable_type() {
    expect_error(
        "<?php class Box { public callable $callback; }",
        "Property Box::$callback cannot use type callable",
    );
}

#[test]
fn test_error_static_property_rejects_readonly() {
    expect_error(
        "<?php class Box { public static readonly int $count = 1; }",
        "Readonly static properties are not supported",
    );
}

#[test]
fn test_error_static_property_undefined() {
    expect_error(
        "<?php class Box {} echo Box::$count;",
        "Undefined static property: Box::count",
    );
}

#[test]
fn test_error_static_property_redeclaration_cannot_add_type_to_untyped_parent() {
    expect_error(
        "<?php class Base { public static $count = 1; } class Child extends Base { public static int $count = 2; }",
        "Type of Child::$count must not be defined (as in class Base)",
    );
}

#[test]
fn test_error_static_property_redeclaration_cannot_reduce_visibility() {
    expect_error(
        "<?php class Base { public static int $count = 1; } class Child extends Base { protected static int $count = 2; }",
        "Cannot reduce visibility when overriding static property: Child::count",
    );
}

#[test]
fn test_error_private_static_property_outside_class() {
    expect_error(
        "<?php class Box { private static int $count = 1; } echo Box::$count;",
        "Cannot access private static property: Box::count",
    );
}

#[test]
fn test_error_wrong_constructor_args() {
    expect_error(
        "<?php class Point { public function __construct($x) {} } $p = new Point();",
        "Constructor 'Point::__construct' expects 1 arguments, got 0",
    );
}

#[test]
fn test_error_parent_outside_class_scope() {
    expect_error(
        "<?php parent::boot();",
        "Cannot use parent:: outside class method scope",
    );
}

#[test]
fn test_error_self_outside_class_scope() {
    expect_error(
        "<?php self::boot();",
        "Cannot use self:: outside class method scope",
    );
}

#[test]
fn test_error_static_outside_class_scope() {
    expect_error(
        "<?php static::boot();",
        "Cannot use static:: outside class method scope",
    );
}

#[test]
fn test_error_self_instance_method_from_static_method() {
    expect_error(
        "<?php class Box { public static function run() { return self::value(); } public function value() { return 1; } } echo Box::run();",
        "Cannot call self instance method from a static method",
    );
}

#[test]
fn test_error_circular_inheritance() {
    expect_error(
        "<?php class A extends B {} class B extends A {}",
        "Circular inheritance detected",
    );
}

#[test]
fn test_error_cannot_reduce_visibility_when_overriding_method() {
    expect_error(
        "<?php class Base { public function ping() { return 1; } } class Child extends Base { protected function ping() { return 2; } }",
        "Cannot reduce visibility when overriding method: Child::ping",
    );
}

#[test]
fn test_error_subclass_cannot_access_parent_private_property() {
    expect_error(
        "<?php class Base { private $value = 1; } class Child extends Base { public function read() { return $this->value; } } $c = new Child(); echo $c->read();",
        "Cannot access private property: Child::value",
    );
}

#[test]
fn test_error_property_shadowing_across_inheritance_not_supported() {
    expect_error(
        "<?php class Base { public $value = 1; } class Child extends Base { public $value = 2; }",
        "Property redeclaration across inheritance is not yet supported: Child::value",
    );
}

#[test]
fn test_error_missing_interface_method() {
    expect_error(
        "<?php interface Named { public function name(); } class User implements Named {}",
        "Class User must implement interface method Named::name",
    );
}

#[test]
fn test_error_wrong_signature_vs_interface() {
    expect_error(
        "<?php interface Named { public function name($x); } class User implements Named { public function name() { return \"x\"; } }",
        "Cannot change parameter count when implementing interface method: User::name",
    );
}

#[test]
fn test_error_instantiate_abstract_class() {
    expect_error(
        "<?php abstract class Base { abstract public function run(); } $x = new Base();",
        "Cannot instantiate abstract class: Base",
    );
}

#[test]
fn test_error_abstract_method_with_body() {
    expect_error(
        "<?php abstract class Base { abstract public function run() { return 1; } }",
        "Abstract method cannot have a body: Base::run",
    );
}

#[test]
fn test_error_final_class_cannot_be_extended() {
    expect_error(
        "<?php final class Base {} class Child extends Base {}",
        "Class Child cannot extend final class Base",
    );
}

#[test]
fn test_error_final_method_cannot_be_overridden() {
    expect_error(
        "<?php class Base { final public function run() { return 1; } } class Child extends Base { public function run() { return 2; } }",
        "Cannot override final method Base::run",
    );
}

#[test]
fn test_error_final_static_method_cannot_be_overridden() {
    expect_error(
        "<?php class Base { final public static function run() { return 1; } } class Child extends Base { public static function run() { return 2; } }",
        "Cannot override final method Base::run",
    );
}

#[test]
fn test_error_final_property_cannot_be_overridden() {
    expect_error(
        "<?php class Base { final public $value; } class Child extends Base { public $value; }",
        "Cannot override final property Base::$value",
    );
}

#[test]
fn test_error_final_abstract_class() {
    expect_error(
        "<?php final abstract class Base {}",
        "Cannot use the final modifier on an abstract class",
    );
}

#[test]
fn test_error_abstract_final_class() {
    expect_error(
        "<?php abstract final class Base {}",
        "Cannot use the final modifier on an abstract class",
    );
}

#[test]
fn test_error_final_abstract_method() {
    expect_error(
        "<?php abstract class Base { final abstract public function run(); }",
        "Cannot use the final modifier on an abstract method: Base::run",
    );
}

#[test]
fn test_error_interface_method_cannot_be_final() {
    expect_error(
        "<?php interface Named { final public function name(); }",
        "Interface method Named::name must not be final",
    );
}

#[test]
fn test_error_final_property_cannot_be_private() {
    expect_error(
        "<?php class Box { final private $value; }",
        "Property cannot be both final and private",
    );
}

#[test]
fn test_error_interface_inheritance_cycle() {
    expect_error(
        "<?php interface A extends B {} interface B extends A {}",
        "Circular interface inheritance detected",
    );
}

#[test]
fn test_error_class_cannot_extend_interface() {
    expect_error(
        "<?php interface Named { public function name(); } class User extends Named {}",
        "Class User cannot extend interface Named; use implements instead",
    );
}

// --- Date/time error tests ---

#[test]
fn test_error_readonly_class_property_is_implicitly_readonly() {
    expect_error(
        "<?php readonly class User { public $id; public function __construct($id) { $this->id = $id; } } $u = new User(1); $u->id = 2;",
        "Cannot assign to readonly property outside constructor: User::id",
    );
}

#[test]
fn test_error_readonly_class_cannot_extend_non_readonly_parent() {
    expect_error(
        "<?php class Base {} readonly class Child extends Base {}",
        "readonly class cannot extend non-readonly parent",
    );
}
