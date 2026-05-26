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
    // instanceof with `self` is illegal when not inside a class method body.
    expect_error(
        "<?php class A {} $a = new A(); echo $a instanceof self;",
        "Cannot use self in instanceof outside of a class context",
    );
}

#[test]
fn test_error_undefined_class() {
    // new on an undefined class name reports an undefined class error.
    expect_error("<?php $x = new Missing();", "Undefined class: Missing");
}

#[test]
fn test_error_undefined_property() {
    // accessing an absent property on an object reports undefined property with the class name.
    expect_error(
        "<?php class Box {} $b = new Box(); echo $b->missing;",
        "Undefined property: Box::missing",
    );
}

#[test]
fn test_error_undefined_method() {
    // calling an absent method on an object reports undefined method with the class name.
    expect_error(
        "<?php class Box {} $b = new Box(); $b->missing();",
        "Undefined method: Box::missing",
    );
}

#[test]
fn test_error_object_subscript_requires_array_access() {
    // object subscript access only works on arrays or objects implementing ArrayAccess.
    expect_error(
        "<?php class Box {} $b = new Box(); echo $b[\"k\"];",
        "Cannot index non-array",
    );
}

#[test]
fn test_error_nullsafe_property_rejects_scalar_receiver() {
    // `?->` on a scalar (e.g. `?int`) is rejected; nullsafe requires object or null.
    expect_error(
        "<?php ?int $value = null; echo $value?->missing;",
        "Nullsafe property access requires an object or null",
    );
}

#[test]
fn test_error_nullsafe_method_rejects_scalar_receiver() {
    // `?->` method call on a scalar is rejected; nullsafe requires object or null.
    expect_error(
        "<?php ?int $value = null; $value?->missing();",
        "Nullsafe method call requires an object or null",
    );
}

#[test]
fn test_error_nullsafe_first_class_callable_is_rejected() {
    // nullsafe cannot be combined with the `...` closure-creation syntax.
    expect_error(
        "<?php class Box { public function run() {} } $b = new Box(); $fn = $b?->run(...);",
        "Cannot combine nullsafe operator with Closure creation",
    );
}

#[test]
fn test_error_nullsafe_assignment_target_is_rejected() {
    // nullsafe cannot appear on the left-hand side of an assignment.
    expect_error(
        "<?php class Profile {} class User { public ?Profile $profile; } $user = new User(); $user?->profile = new Profile();",
        "Invalid assignment target",
    );
}

#[test]
fn test_error_private_access() {
    // private property cannot be read from outside the class.
    expect_error(
        "<?php class Secret { private $value = 7; } $s = new Secret(); echo $s->value;",
        "Cannot access private property: Secret::value",
    );
}

#[test]
fn test_error_readonly_assign() {
    // readonly property may only be assigned during construction.
    expect_error(
        "<?php class User { public readonly $id; public function __construct($id) { $this->id = $id; } } $u = new User(1); $u->id = 2;",
        "Cannot assign to readonly property outside constructor: User::id",
    );
}

#[test]
fn test_error_typed_property_rejects_invalid_default() {
    // typed property with a mismatched default value is rejected at declaration time.
    expect_error(
        "<?php class Box { public int $value = \"bad\"; }",
        "Property Box::$value default expects Int, got Str",
    );
}

#[test]
fn test_error_typed_property_rejects_invalid_assignment() {
    // assigning a string to an int-typed property is rejected at the assignment site.
    expect_error(
        "<?php class Box { public int $value; } $b = new Box(); $b->value = \"bad\";",
        "Property Box::$value expects Int, got Str",
    );
}

#[test]
fn test_error_typed_property_rejects_constructor_assignment_from_untyped_param() {
    // an untyped constructor parameter passed to a typed property must still satisfy the property type.
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
    // promoted property syntax is only valid inside a constructor.
    expect_error(
        "<?php class Box { public function set(public int $value) {} }",
        "Cannot declare promoted property outside a constructor",
    );
}

#[test]
fn test_error_constructor_promotion_redeclares_property() {
    // declaring a promoted parameter whose name duplicates an explicit property is rejected.
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
    // variadic parameters cannot be promoted.
    expect_error(
        "<?php class Box { public function __construct(public ...$values) {} }",
        "Cannot declare variadic promoted property",
    );
}

#[test]
fn test_error_constructor_promotion_by_reference_requires_variable_arg() {
    // by-reference promoted parameter must receive a variable argument at the call site.
    expect_error(
        "<?php class Box { public function __construct(public int &$value) {} } $box = new Box(1);",
        "Constructor 'Box::__construct' parameter $value must be passed a variable",
    );
}

#[test]
fn test_error_constructor_promotion_readonly_by_reference() {
    // readonly promoted property cannot be by-reference.
    expect_error(
        "<?php class Box { public function __construct(public readonly int &$value) {} }",
        "Readonly promoted property cannot be by-reference",
    );
}

#[test]
fn test_error_readonly_class_constructor_promotion_by_reference() {
    // inside a readonly class, promoted properties cannot be by-reference.
    expect_error(
        "<?php readonly class Box { public function __construct(public int &$value) {} }",
        "Readonly promoted property cannot be by-reference",
    );
}

#[test]
fn test_error_constructor_promotion_rejects_abstract_constructor() {
    // promoted properties cannot appear in an abstract constructor declaration.
    expect_error(
        "<?php abstract class Box { abstract public function __construct(public int $value); }",
        "Cannot declare promoted property in an abstract constructor",
    );
}

#[test]
fn test_error_typed_property_rejects_void_type() {
    // void is not a valid property type.
    expect_error(
        "<?php class Box { public void $value; }",
        "Property Box::$value cannot use type void",
    );
}

#[test]
fn test_error_typed_property_rejects_callable_type() {
    // callable is not a valid property type.
    expect_error(
        "<?php class Box { public callable $callback; }",
        "Property Box::$callback cannot use type callable",
    );
}

#[test]
fn test_error_static_property_rejects_readonly() {
    // static properties cannot be marked readonly.
    expect_error(
        "<?php class Box { public static readonly int $count = 1; }",
        "Static properties cannot be readonly",
    );
}

#[test]
fn test_error_readonly_class_static_property_with_readonly_modifier() {
    // even inside a readonly class, static properties cannot be readonly.
    expect_error(
        "<?php readonly class Box { public static readonly int $count = 1; }",
        "Static properties cannot be readonly",
    );
}

#[test]
fn test_error_static_property_undefined() {
    // accessing an absent static property reports undefined with the class name.
    expect_error(
        "<?php class Box {} echo Box::$count;",
        "Undefined static property: Box::count",
    );
}

#[test]
fn test_error_static_property_redeclaration_cannot_add_type_to_untyped_parent() {
    // a child static property cannot acquire a type when the parent has no type.
    expect_error(
        "<?php class Base { public static $count = 1; } class Child extends Base { public static int $count = 2; }",
        "Type of Child::$count must not be defined (as in class Base)",
    );
}

#[test]
fn test_error_static_property_redeclaration_cannot_reduce_visibility() {
    // child static property cannot reduce visibility compared to the parent.
    expect_error(
        "<?php class Base { public static int $count = 1; } class Child extends Base { protected static int $count = 2; }",
        "Cannot reduce visibility when overriding static property: Child::count",
    );
}

#[test]
fn test_error_private_static_property_outside_class() {
    // private static property cannot be accessed from outside the declaring class.
    expect_error(
        "<?php class Box { private static int $count = 1; } echo Box::$count;",
        "Cannot access private static property: Box::count",
    );
}

#[test]
fn test_error_wrong_constructor_args() {
    // missing arguments to a constructor produce the expected argument-count error.
    expect_error(
        "<?php class Point { public function __construct($x) {} } $p = new Point();",
        "Constructor 'Point::__construct' expects 1 arguments, got 0",
    );
}

#[test]
fn test_error_parent_outside_class_scope() {
    // `parent::` is illegal outside a class method body.
    expect_error(
        "<?php parent::boot();",
        "Cannot use parent:: outside class method scope",
    );
}

#[test]
fn test_error_self_outside_class_scope() {
    // `self::` is illegal outside a class method body.
    expect_error(
        "<?php self::boot();",
        "Cannot use self:: outside class method scope",
    );
}

#[test]
fn test_error_static_outside_class_scope() {
    // `static::` is illegal outside a class method body.
    expect_error(
        "<?php static::boot();",
        "Cannot use static:: outside class method scope",
    );
}

#[test]
fn test_error_self_instance_method_from_static_method() {
    // `self::` in a static method cannot call an instance method.
    expect_error(
        "<?php class Box { public static function run() { return self::value(); } public function value() { return 1; } } echo Box::run();",
        "Cannot call self instance method from a static method",
    );
}

#[test]
fn test_error_circular_inheritance() {
    // a class that extends itself is rejected as circular inheritance.
    expect_error(
        "<?php class A extends B {} class B extends A {}",
        "Circular inheritance detected",
    );
}

#[test]
fn test_error_cannot_reduce_visibility_when_overriding_method() {
    // child method cannot reduce visibility when overriding a parent method.
    expect_error(
        "<?php class Base { public function ping() { return 1; } } class Child extends Base { protected function ping() { return 2; } }",
        "Cannot reduce visibility when overriding method: Child::ping",
    );
}

#[test]
fn test_error_subclass_cannot_access_parent_private_property() {
    // private properties are invisible to child classes; accessing them via `$this` is an error.
    expect_error(
        "<?php class Base { private $value = 1; } class Child extends Base { public function read() { return $this->value; } } $c = new Child(); echo $c->read();",
        "Cannot access private property: Child::value",
    );
}

#[test]
fn test_error_missing_interface_method() {
    // a class that implements an interface must provide all interface methods.
    expect_error(
        "<?php interface Named { public function name(); } class User implements Named {}",
        "Class User must implement interface method Named::name",
    );
}

#[test]
fn test_error_wrong_signature_vs_interface() {
    // implementing an interface method with a different parameter count is an error.
    expect_error(
        "<?php interface Named { public function name($x); } class User implements Named { public function name() { return \"x\"; } }",
        "Cannot change parameter count when implementing interface method: User::name",
    );
}

#[test]
fn test_error_user_class_cannot_implement_throwable_directly() {
    // user classes may not directly implement Throwable; they must extend Exception or Error.
    expect_error(
        r#"<?php
class MyThrowable implements Throwable {
    public function getMessage(): string { return "x"; }
    public function getCode(): int { return 0; }
    public function getFile(): string { return ""; }
    public function getLine(): int { return 0; }
    public function getTrace(): array { return []; }
    public function getTraceAsString(): string { return ""; }
    public function getPrevious(): ?Throwable { return null; }
    public function __toString(): string { return "x"; }
}
"#,
        "Class MyThrowable cannot implement interface Throwable, extend Exception or Error instead",
    );
}

#[test]
fn test_error_user_class_cannot_implement_throwable_child_interface_directly() {
    // a user class may not directly implement any interface that extends Throwable.
    expect_error(
        r#"<?php
interface MyThrowableInterface extends Throwable {}

class MyThrowable implements MyThrowableInterface {
    public function getMessage(): string { return "x"; }
    public function getCode(): int { return 0; }
    public function getFile(): string { return ""; }
    public function getLine(): int { return 0; }
    public function getTrace(): array { return []; }
    public function getTraceAsString(): string { return ""; }
    public function getPrevious(): ?Throwable { return null; }
    public function __toString(): string { return "x"; }
}
"#,
        "Class MyThrowable cannot implement interface Throwable, extend Exception or Error instead",
    );
}

#[test]
fn test_error_instantiate_abstract_class() {
    // abstract classes cannot be instantiated directly.
    expect_error(
        "<?php abstract class Base { abstract public function run(); } $x = new Base();",
        "Cannot instantiate abstract class: Base",
    );
}

#[test]
fn test_error_abstract_method_with_body() {
    // abstract methods must not have a body.
    expect_error(
        "<?php abstract class Base { abstract public function run() { return 1; } }",
        "Abstract method cannot have a body: Base::run",
    );
}

#[test]
fn test_error_final_class_cannot_be_extended() {
    // a final class cannot be extended.
    expect_error(
        "<?php final class Base {} class Child extends Base {}",
        "Class Child cannot extend final class Base",
    );
}

#[test]
fn test_error_final_method_cannot_be_overridden() {
    // a final instance method cannot be overridden in a child class.
    expect_error(
        "<?php class Base { final public function run() { return 1; } } class Child extends Base { public function run() { return 2; } }",
        "Cannot override final method Base::run",
    );
}

#[test]
fn test_error_final_static_method_cannot_be_overridden() {
    // a final static method cannot be overridden in a child class.
    expect_error(
        "<?php class Base { final public static function run() { return 1; } } class Child extends Base { public static function run() { return 2; } }",
        "Cannot override final method Base::run",
    );
}

#[test]
fn test_error_final_property_cannot_be_overridden() {
    // a final property cannot be overridden in a child class.
    expect_error(
        "<?php class Base { final public $value; } class Child extends Base { public $value; }",
        "Cannot override final property Base::$value",
    );
}

#[test]
fn test_error_final_abstract_class() {
    // a class cannot be both final and abstract.
    expect_error(
        "<?php final abstract class Base {}",
        "Cannot use the final modifier on an abstract class",
    );
}

#[test]
fn test_error_abstract_final_class() {
    // a class cannot be both abstract and final (reverse order also rejected).
    expect_error(
        "<?php abstract final class Base {}",
        "Cannot use the final modifier on an abstract class",
    );
}

#[test]
fn test_error_final_abstract_method() {
    // an abstract method cannot also be final.
    expect_error(
        "<?php abstract class Base { final abstract public function run(); }",
        "Cannot use the final modifier on an abstract method: Base::run",
    );
}

#[test]
fn test_error_interface_method_cannot_be_final() {
    // methods declared in an interface cannot be final.
    expect_error(
        "<?php interface Named { final public function name(); }",
        "Interface method Named::name must not be final",
    );
}

#[test]
fn test_error_final_property_cannot_be_private() {
    // a property cannot be marked both final and private.
    expect_error(
        "<?php class Box { final private $value; }",
        "Property cannot be both final and private",
    );
}

#[test]
fn test_error_interface_inheritance_cycle() {
    // two interfaces cannot extend each other (circular interface inheritance).
    expect_error(
        "<?php interface A extends B {} interface B extends A {}",
        "Circular interface inheritance detected",
    );
}

#[test]
fn test_error_class_cannot_extend_interface() {
    // a class uses `extends` for classes and `implements` for interfaces; using `extends` for an interface is an error.
    expect_error(
        "<?php interface Named { public function name(); } class User extends Named {}",
        "Class User cannot extend interface Named; use implements instead",
    );
}

// --- Date/time error tests ---

#[test]
fn test_error_readonly_class_property_is_implicitly_readonly() {
    // inside a readonly class, instance properties are implicitly readonly.
    expect_error(
        "<?php readonly class User { public $id; public function __construct($id) { $this->id = $id; } } $u = new User(1); $u->id = 2;",
        "Cannot assign to readonly property outside constructor: User::id",
    );
}

#[test]
fn test_error_readonly_class_cannot_extend_non_readonly_parent() {
    // a readonly class cannot extend a non-readonly parent class.
    expect_error(
        "<?php class Base {} readonly class Child extends Base {}",
        "readonly class cannot extend non-readonly parent",
    );
}

#[test]
fn test_error_property_redeclaration_changes_type() {
    // child property cannot change the declared type from the parent.
    expect_error(
        "<?php class Base { public int $x = 0; } class Child extends Base { public string $x = \"hello\"; }",
        "Type of Child::$x must be int, not string (as in class Base)",
    );
}

#[test]
fn test_error_property_redeclaration_reduces_visibility() {
    // child property cannot reduce visibility compared to the parent.
    expect_error(
        "<?php class Base { public int $value = 1; } class Child extends Base { protected int $value = 2; }",
        "Cannot reduce visibility when overriding property: Child::$value",
    );
}

#[test]
fn test_error_property_redeclaration_removes_readonly() {
    // child property cannot remove the readonly modifier from the parent.
    expect_error(
        "<?php class Base { public readonly int $value; public function __construct() { $this->value = 1; } } class Child extends Base { public int $value = 5; }",
        "Cannot remove readonly modifier when redeclaring property: Child::$value",
    );
}

#[test]
fn test_error_property_redeclaration_drops_parent_type_declaration() {
    // child property cannot drop a type declaration that the parent declared.
    expect_error(
        "<?php class Base { public int $x = 0; } class Child extends Base { public $x = 5; }",
        "Type of Child::$x must be int (as in class Base)",
    );
}

#[test]
fn test_error_property_redeclaration_adds_type_to_untyped_parent() {
    // child property cannot add a type when the parent has no type on the property.
    expect_error(
        "<?php class Base { public $x = 0; } class Child extends Base { public int $x = 5; }",
        "Type of Child::$x must not be defined (as in class Base)",
    );
}

#[test]
fn test_error_property_redeclaration_shadows_private_parent_property() {
    // private parent properties cannot be shadowed by a child; the feature is not yet supported.
    expect_error(
        "<?php class Base { private int $secret = 1; } class Child extends Base { public int $secret = 2; }",
        "shadowing private parent properties is not yet supported",
    );
}

#[test]
fn test_error_property_redeclaration_changes_by_ref_qualifier() {
    // child property cannot change the by-reference qualifier from the parent.
    expect_error(
        "<?php class Base { public function __construct(public int &$ref) {} } class Child extends Base { public int $ref = 0; }",
        "Cannot change by-reference qualifier when redeclaring property: Child::$ref",
    );
}

#[test]
fn test_error_abstract_property_in_non_abstract_class() {
    // abstract properties are only allowed in abstract classes.
    expect_error(
        "<?php class Box { abstract public int $value { get; } }",
        "Abstract properties can only be declared in abstract classes",
    );
}

#[test]
fn test_error_unhooked_abstract_property_is_rejected() {
    // a property marked abstract without hooks is rejected; only hooked properties may be abstract.
    expect_error(
        "<?php abstract class Box { abstract public int $value; }",
        "Only hooked properties may be declared abstract",
    );
}

#[test]
fn test_error_concrete_class_missing_abstract_trait_property() {
    // a concrete class using a trait that declares an abstract hooked property must implement it.
    expect_error(
        "<?php trait HasValue { abstract public int $value { get; } } class Box { use HasValue; }",
        "Concrete class Box must declare abstract property Box::$value",
    );
}

#[test]
fn test_error_interface_property_without_hooks_is_rejected() {
    // interfaces may only declare hooked properties; plain properties are rejected.
    expect_error(
        "<?php interface HasValue { public int $value; }",
        "Interfaces may only include hooked properties",
    );
}

#[test]
fn test_error_interface_set_property_rejects_readonly_implementation() {
    // a readonly property cannot satisfy a set-only contract from an interface.
    expect_error(
        "<?php interface Writable { public int $value { set; } } class Box implements Writable { public readonly int $value; public function __construct(int $value) { $this->value = $value; } }",
        "Readonly property Box::$value cannot satisfy set property contract",
    );
}

#[test]
fn test_error_interface_property_missing_uses_contract_span() {
    // when an implementing class misses a hooked property, the span points to the class declaration line.
    let err = check_source_full(
        r#"<?php
interface HasValue {
    public int $value { get; }
}
class Box implements HasValue {}
"#,
    )
    .expect_err("expected missing interface property error");
    assert!(
        err.message
            .contains("Class Box must implement interface property HasValue::$value")
    );
    assert_eq!(err.span.line, 3);
    assert!(err.span.col > 0);
}

#[test]
fn test_error_interface_property_type_mismatch_uses_implementation_span() {
    // when a hooked property type is incompatible, the span points to the implementing property.
    let err = check_source_full(
        r#"<?php
interface HasValue {
    public string $value { get; }
}
class Box implements HasValue {
    public int $value;
}
"#,
    )
    .expect_err("expected interface property type error");
    assert!(
        err.message.contains(
            "Type of Box::$value must be compatible with get property contract string"
        )
    );
    assert_eq!(err.span.line, 6);
    assert!(err.span.col > 0);
}

#[test]
fn test_error_deferred_interface_property_uses_contract_span() {
    // when an intermediate abstract class defers implementation, the final concrete class still gets the error with the contract span.
    let err = check_source_full(
        r#"<?php
interface HasValue {
    public int $value { get; }
}
abstract class Base implements HasValue {}
class Box extends Base {}
"#,
    )
    .expect_err("expected deferred interface property error");
    assert!(
        err.message
            .contains("Concrete class Box must declare abstract property HasValue::$value")
    );
    assert_eq!(err.span.line, 3);
    assert!(err.span.col > 0);
}

#[test]
fn test_error_abstract_property_with_default() {
    // abstract hooked properties cannot have a default value.
    expect_error(
        "<?php abstract class Box { abstract public int $value = 1 { get; } }",
        "Abstract property $value cannot have a default value",
    );
}

#[test]
fn test_error_abstract_property_with_static() {
    // hooked properties cannot be static.
    expect_error(
        "<?php abstract class Box { abstract public static int $value { get; } }",
        "Cannot declare hooks for static property",
    );
}

#[test]
fn test_error_abstract_property_with_final() {
    // hooked properties cannot also be final.
    expect_error(
        "<?php abstract class Box { abstract final public int $value { get; } }",
        "Cannot use the final modifier on an abstract property",
    );
}

#[test]
fn test_error_abstract_property_with_private() {
    // private hooked properties are not supported.
    expect_error(
        "<?php abstract class Box { abstract private int $value { get; } }",
        "Private abstract properties are not supported",
    );
}

#[test]
fn test_error_concrete_class_missing_abstract_property() {
    // a concrete subclass of an abstract class must implement all abstract hooked properties.
    expect_error(
        "<?php abstract class Shape { abstract public int $sides { get; } } class Triangle extends Shape {}",
        "Concrete class Triangle must declare abstract property Shape::$sides",
    );
}

#[test]
fn test_error_concrete_property_redeclared_as_abstract() {
    // a property with a concrete implementation in the parent cannot become abstract in the child.
    expect_error(
        "<?php class Base { public int $value = 1; } abstract class Child extends Base { abstract public int $value { get; } }",
        "Cannot make concrete property abstract: Child::$value",
    );
}
