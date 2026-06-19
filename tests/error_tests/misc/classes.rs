//! Purpose:
//! Integration or regression tests for diagnostic coverage of misc classes, including instanceof self outside class scope, undefined class, and undefined property.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

/// Verifies the error diagnostic for instanceof self outside class scope.
#[test]
fn test_error_instanceof_self_outside_class_scope() {
    // instanceof with `self` is illegal when not inside a class method body.
    expect_error(
        "<?php class A {} $a = new A(); echo $a instanceof self;",
        "Cannot use self in instanceof outside of a class context",
    );
}

/// Verifies the error diagnostic for undefined class.
#[test]
fn test_error_undefined_class() {
    // new on an undefined class name reports an undefined class error.
    expect_error("<?php $x = new Missing();", "Undefined class: Missing");
}

/// Verifies the error diagnostic for undefined property.
#[test]
fn test_error_undefined_property() {
    // accessing an absent property on an object reports undefined property with the class name.
    expect_error(
        "<?php class Box {} $b = new Box(); echo $b->missing;",
        "Undefined property: Box::missing",
    );
}

/// Verifies the error diagnostic for undefined method.
#[test]
fn test_error_undefined_method() {
    // calling an absent method on an object reports undefined method with the class name.
    expect_error(
        "<?php class Box {} $b = new Box(); $b->missing();",
        "Undefined method: Box::missing",
    );
}

/// Verifies that a method call on an object union (`A|B`) is rejected when the
/// method is absent from one of the member classes: every object member must
/// provide the method for the runtime class-id dispatch to be sound.
#[test]
fn test_error_object_union_method_missing_from_member() {
    expect_error(
        "<?php class A { function only_a() {} } class B {} function make(bool $b): A|B { return $b ? new A() : new B(); } make(true)->only_a();",
        "Undefined method: B::only_a",
    );
}

/// Verifies the error diagnostic for object subscript requires array access.
#[test]
fn test_error_object_subscript_requires_array_access() {
    // object subscript access only works on arrays or objects implementing ArrayAccess.
    expect_error(
        "<?php class Box {} $b = new Box(); echo $b[\"k\"];",
        "Cannot index non-array",
    );
}

/// Verifies the error diagnostic for nullsafe property rejects scalar receiver.
#[test]
fn test_error_nullsafe_property_rejects_scalar_receiver() {
    // `?->` on a scalar (e.g. `?int`) is rejected; nullsafe requires object or null.
    expect_error(
        "<?php ?int $value = null; echo $value?->missing;",
        "Nullsafe property access requires an object or null",
    );
}

/// Verifies the error diagnostic for nullsafe method rejects scalar receiver.
#[test]
fn test_error_nullsafe_method_rejects_scalar_receiver() {
    // `?->` method call on a scalar is rejected; nullsafe requires object or null.
    expect_error(
        "<?php ?int $value = null; $value?->missing();",
        "Nullsafe method call requires an object or null",
    );
}

/// Verifies the error diagnostic for nullsafe first class callable is rejected.
#[test]
fn test_error_nullsafe_first_class_callable_is_rejected() {
    // nullsafe cannot be combined with the `...` closure-creation syntax.
    expect_error(
        "<?php class Box { public function run() {} } $b = new Box(); $fn = $b?->run(...);",
        "Cannot combine nullsafe operator with Closure creation",
    );
}

/// Verifies the error diagnostic for nullsafe assignment target is rejected.
#[test]
fn test_error_nullsafe_assignment_target_is_rejected() {
    // nullsafe cannot appear on the left-hand side of an assignment.
    expect_error(
        "<?php class Profile {} class User { public ?Profile $profile; } $user = new User(); $user?->profile = new Profile();",
        "Invalid assignment target",
    );
}

/// Verifies the error diagnostic for private access.
#[test]
fn test_error_private_access() {
    // private property cannot be read from outside the class.
    expect_error(
        "<?php class Secret { private $value = 7; } $s = new Secret(); echo $s->value;",
        "Cannot access private property: Secret::value",
    );
}

/// Verifies the error diagnostic for readonly assign.
#[test]
fn test_error_readonly_assign() {
    // readonly property may only be assigned during construction.
    expect_error(
        "<?php class User { public readonly $id; public function __construct($id) { $this->id = $id; } } $u = new User(1); $u->id = 2;",
        "Cannot assign to readonly property outside constructor: User::id",
    );
}

/// Verifies the error diagnostic for typed property rejects invalid default.
#[test]
fn test_error_typed_property_rejects_invalid_default() {
    // typed property with a mismatched default value is rejected at declaration time.
    expect_error(
        "<?php class Box { public int $value = \"bad\"; }",
        "Property Box::$value default expects Int, got Str",
    );
}

/// Verifies the error diagnostic for typed property rejects invalid assignment.
#[test]
fn test_error_typed_property_rejects_invalid_assignment() {
    // assigning a string to an int-typed property is rejected at the assignment site.
    expect_error(
        "<?php class Box { public int $value; } $b = new Box(); $b->value = \"bad\";",
        "Property Box::$value expects Int, got Str",
    );
}

/// Verifies the error diagnostic for typed property rejects constructor assignment from untyped
/// param.
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

/// Verifies the error diagnostic for constructor promotion outside constructor.
#[test]
fn test_error_constructor_promotion_outside_constructor() {
    // promoted property syntax is only valid inside a constructor.
    expect_error(
        "<?php class Box { public function set(public int $value) {} }",
        "Cannot declare promoted property outside a constructor",
    );
}

/// Verifies the error diagnostic for constructor promotion redeclares property.
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

/// Verifies the error diagnostic for constructor promotion rejects variadic.
#[test]
fn test_error_constructor_promotion_rejects_variadic() {
    // variadic parameters cannot be promoted.
    expect_error(
        "<?php class Box { public function __construct(public ...$values) {} }",
        "Cannot declare variadic promoted property",
    );
}

/// Verifies the error diagnostic for constructor promotion by reference requires variable arg.
#[test]
fn test_error_constructor_promotion_by_reference_requires_variable_arg() {
    // by-reference promoted parameter must receive a variable argument at the call site.
    expect_error(
        "<?php class Box { public function __construct(public int &$value) {} } $box = new Box(1);",
        "Constructor 'Box::__construct' parameter $value must be passed a variable",
    );
}

/// Verifies the error diagnostic for constructor promotion readonly by reference.
#[test]
fn test_error_constructor_promotion_readonly_by_reference() {
    // readonly promoted property cannot be by-reference.
    expect_error(
        "<?php class Box { public function __construct(public readonly int &$value) {} }",
        "Readonly promoted property cannot be by-reference",
    );
}

/// Verifies the error diagnostic for readonly class constructor promotion by reference.
#[test]
fn test_error_readonly_class_constructor_promotion_by_reference() {
    // inside a readonly class, promoted properties cannot be by-reference.
    expect_error(
        "<?php readonly class Box { public function __construct(public int &$value) {} }",
        "Readonly promoted property cannot be by-reference",
    );
}

/// Verifies the error diagnostic for constructor promotion rejects abstract constructor.
#[test]
fn test_error_constructor_promotion_rejects_abstract_constructor() {
    // promoted properties cannot appear in an abstract constructor declaration.
    expect_error(
        "<?php abstract class Box { abstract public function __construct(public int $value); }",
        "Cannot declare promoted property in an abstract constructor",
    );
}

/// Verifies the error diagnostic for typed property rejects void type.
#[test]
fn test_error_typed_property_rejects_void_type() {
    // void is not a valid property type.
    expect_error(
        "<?php class Box { public void $value; }",
        "Property Box::$value cannot use type void",
    );
}

/// Verifies the error diagnostic for typed property rejects callable type.
#[test]
fn test_error_typed_property_rejects_callable_type() {
    // callable is not a valid property type.
    expect_error(
        "<?php class Box { public callable $callback; }",
        "Property Box::$callback cannot use type callable",
    );
}

/// Verifies the error diagnostic for static property rejects readonly.
#[test]
fn test_error_static_property_rejects_readonly() {
    // static properties cannot be marked readonly.
    expect_error(
        "<?php class Box { public static readonly int $count = 1; }",
        "Static properties cannot be readonly",
    );
}

/// Verifies the error diagnostic for readonly class static property with readonly modifier.
#[test]
fn test_error_readonly_class_static_property_with_readonly_modifier() {
    // even inside a readonly class, static properties cannot be readonly.
    expect_error(
        "<?php readonly class Box { public static readonly int $count = 1; }",
        "Static properties cannot be readonly",
    );
}

/// Verifies the error diagnostic for static property undefined.
#[test]
fn test_error_static_property_undefined() {
    // accessing an absent static property reports undefined with the class name.
    expect_error(
        "<?php class Box {} echo Box::$count;",
        "Undefined static property: Box::count",
    );
}

/// Verifies the error diagnostic for static property redeclaration cannot add type to untyped
/// parent.
#[test]
fn test_error_static_property_redeclaration_cannot_add_type_to_untyped_parent() {
    // a child static property cannot acquire a type when the parent has no type.
    expect_error(
        "<?php class Base { public static $count = 1; } class Child extends Base { public static int $count = 2; }",
        "Type of Child::$count must not be defined (as in class Base)",
    );
}

/// Verifies the error diagnostic for static property redeclaration cannot reduce visibility.
#[test]
fn test_error_static_property_redeclaration_cannot_reduce_visibility() {
    // child static property cannot reduce visibility compared to the parent.
    expect_error(
        "<?php class Base { public static int $count = 1; } class Child extends Base { protected static int $count = 2; }",
        "Cannot reduce visibility when overriding static property: Child::count",
    );
}

/// Verifies the error diagnostic for private static property outside class.
#[test]
fn test_error_private_static_property_outside_class() {
    // private static property cannot be accessed from outside the declaring class.
    expect_error(
        "<?php class Box { private static int $count = 1; } echo Box::$count;",
        "Cannot access private static property: Box::count",
    );
}

/// Verifies the error diagnostic for wrong constructor args.
#[test]
fn test_error_wrong_constructor_args() {
    // missing arguments to a constructor produce the expected argument-count error.
    expect_error(
        "<?php class Point { public function __construct($x) {} } $p = new Point();",
        "Constructor 'Point::__construct' expects 1 arguments, got 0",
    );
}

/// Verifies the error diagnostic for parent outside class scope.
#[test]
fn test_error_parent_outside_class_scope() {
    // `parent::` is illegal outside a class method body.
    expect_error(
        "<?php parent::boot();",
        "Cannot use parent:: outside class method scope",
    );
}

/// Verifies the error diagnostic for self outside class scope.
#[test]
fn test_error_self_outside_class_scope() {
    // `self::` is illegal outside a class method body.
    expect_error(
        "<?php self::boot();",
        "Cannot use self:: outside class method scope",
    );
}

/// Verifies the error diagnostic for static outside class scope.
#[test]
fn test_error_static_outside_class_scope() {
    // `static::` is illegal outside a class method body.
    expect_error(
        "<?php static::boot();",
        "Cannot use static:: outside class method scope",
    );
}

/// Verifies the error diagnostic for self instance method from static method.
#[test]
fn test_error_self_instance_method_from_static_method() {
    // `self::` in a static method cannot call an instance method.
    expect_error(
        "<?php class Box { public static function run() { return self::value(); } public function value() { return 1; } } echo Box::run();",
        "Cannot call self instance method from a static method",
    );
}

/// Verifies the error diagnostic for circular inheritance.
#[test]
fn test_error_circular_inheritance() {
    // a class that extends itself is rejected as circular inheritance.
    expect_error(
        "<?php class A extends B {} class B extends A {}",
        "Circular inheritance detected",
    );
}

/// Verifies the error diagnostic for cannot reduce visibility when overriding method.
#[test]
fn test_error_cannot_reduce_visibility_when_overriding_method() {
    // child method cannot reduce visibility when overriding a parent method.
    expect_error(
        "<?php class Base { public function ping() { return 1; } } class Child extends Base { protected function ping() { return 2; } }",
        "Cannot reduce visibility when overriding method: Child::ping",
    );
}

/// Verifies the error diagnostic for subclass cannot access parent private property.
#[test]
fn test_error_subclass_cannot_access_parent_private_property() {
    // private properties are invisible to child classes; accessing them via `$this` is an error.
    expect_error(
        "<?php class Base { private $value = 1; } class Child extends Base { public function read() { return $this->value; } } $c = new Child(); echo $c->read();",
        "Cannot access private property: Child::value",
    );
}

/// Verifies the error diagnostic for missing interface method.
#[test]
fn test_error_missing_interface_method() {
    // a class that implements an interface must provide all interface methods.
    expect_error(
        "<?php interface Named { public function name(); } class User implements Named {}",
        "Class User must implement interface method Named::name",
    );
}

/// Verifies the error diagnostic for wrong signature vs interface.
#[test]
fn test_error_wrong_signature_vs_interface() {
    // implementing an interface method with a different parameter count is an error.
    expect_error(
        "<?php interface Named { public function name($x); } class User implements Named { public function name() { return \"x\"; } }",
        "Cannot change parameter count when implementing interface method: User::name",
    );
}

/// Verifies the error diagnostic for user class cannot implement throwable directly.
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

/// Verifies the error diagnostic for user class cannot implement throwable child interface
/// directly.
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

/// Verifies the error diagnostic for instantiate abstract class.
#[test]
fn test_error_instantiate_abstract_class() {
    // abstract classes cannot be instantiated directly.
    expect_error(
        "<?php abstract class Base { abstract public function run(); } $x = new Base();",
        "Cannot instantiate abstract class: Base",
    );
}

/// Verifies the error diagnostic for abstract method with body.
#[test]
fn test_error_abstract_method_with_body() {
    // abstract methods must not have a body.
    expect_error(
        "<?php abstract class Base { abstract public function run() { return 1; } }",
        "Abstract method cannot have a body: Base::run",
    );
}

/// Verifies the error diagnostic for final class cannot be extended.
#[test]
fn test_error_final_class_cannot_be_extended() {
    // a final class cannot be extended.
    expect_error(
        "<?php final class Base {} class Child extends Base {}",
        "Class Child cannot extend final class Base",
    );
}

/// Verifies the error diagnostic for final method cannot be overridden.
#[test]
fn test_error_final_method_cannot_be_overridden() {
    // a final instance method cannot be overridden in a child class.
    expect_error(
        "<?php class Base { final public function run() { return 1; } } class Child extends Base { public function run() { return 2; } }",
        "Cannot override final method Base::run",
    );
}

/// Verifies the error diagnostic for final static method cannot be overridden.
#[test]
fn test_error_final_static_method_cannot_be_overridden() {
    // a final static method cannot be overridden in a child class.
    expect_error(
        "<?php class Base { final public static function run() { return 1; } } class Child extends Base { public static function run() { return 2; } }",
        "Cannot override final method Base::run",
    );
}

/// Verifies the error diagnostic for final property cannot be overridden.
#[test]
fn test_error_final_property_cannot_be_overridden() {
    // a final property cannot be overridden in a child class.
    expect_error(
        "<?php class Base { final public $value; } class Child extends Base { public $value; }",
        "Cannot override final property Base::$value",
    );
}

/// Verifies the error diagnostic for final abstract class.
#[test]
fn test_error_final_abstract_class() {
    // a class cannot be both final and abstract.
    expect_error(
        "<?php final abstract class Base {}",
        "Cannot use the final modifier on an abstract class",
    );
}

/// Verifies the error diagnostic for abstract final class.
#[test]
fn test_error_abstract_final_class() {
    // a class cannot be both abstract and final (reverse order also rejected).
    expect_error(
        "<?php abstract final class Base {}",
        "Cannot use the final modifier on an abstract class",
    );
}

/// Verifies the error diagnostic for final abstract method.
#[test]
fn test_error_final_abstract_method() {
    // an abstract method cannot also be final.
    expect_error(
        "<?php abstract class Base { final abstract public function run(); }",
        "Cannot use the final modifier on an abstract method: Base::run",
    );
}

/// Verifies the error diagnostic for interface method cannot be final.
#[test]
fn test_error_interface_method_cannot_be_final() {
    // methods declared in an interface cannot be final.
    expect_error(
        "<?php interface Named { final public function name(); }",
        "Interface method Named::name must not be final",
    );
}

/// Verifies the error diagnostic for final property cannot be private.
#[test]
fn test_error_final_property_cannot_be_private() {
    // a property cannot be marked both final and private.
    expect_error(
        "<?php class Box { final private $value; }",
        "Property cannot be both final and private",
    );
}

/// Verifies the error diagnostic for interface inheritance cycle.
#[test]
fn test_error_interface_inheritance_cycle() {
    // two interfaces cannot extend each other (circular interface inheritance).
    expect_error(
        "<?php interface A extends B {} interface B extends A {}",
        "Circular interface inheritance detected",
    );
}

/// Verifies the error diagnostic for class cannot extend interface.
#[test]
fn test_error_class_cannot_extend_interface() {
    // a class uses `extends` for classes and `implements` for interfaces; using `extends` for an interface is an error.
    expect_error(
        "<?php interface Named { public function name(); } class User extends Named {}",
        "Class User cannot extend interface Named; use implements instead",
    );
}

// --- Date/time error tests ---

/// Verifies the error diagnostic for readonly class property is implicitly readonly.
#[test]
fn test_error_readonly_class_property_is_implicitly_readonly() {
    // inside a readonly class, instance properties are implicitly readonly.
    expect_error(
        "<?php readonly class User { public $id; public function __construct($id) { $this->id = $id; } } $u = new User(1); $u->id = 2;",
        "Cannot assign to readonly property outside constructor: User::id",
    );
}

/// Verifies the error diagnostic for readonly class cannot extend non readonly parent.
#[test]
fn test_error_readonly_class_cannot_extend_non_readonly_parent() {
    // a readonly class cannot extend a non-readonly parent class.
    expect_error(
        "<?php class Base {} readonly class Child extends Base {}",
        "readonly class cannot extend non-readonly parent",
    );
}

/// Verifies the error diagnostic for property redeclaration changes type.
#[test]
fn test_error_property_redeclaration_changes_type() {
    // child property cannot change the declared type from the parent.
    expect_error(
        "<?php class Base { public int $x = 0; } class Child extends Base { public string $x = \"hello\"; }",
        "Type of Child::$x must be int, not string (as in class Base)",
    );
}

/// Verifies the error diagnostic for property redeclaration reduces visibility.
#[test]
fn test_error_property_redeclaration_reduces_visibility() {
    // child property cannot reduce visibility compared to the parent.
    expect_error(
        "<?php class Base { public int $value = 1; } class Child extends Base { protected int $value = 2; }",
        "Cannot reduce visibility when overriding property: Child::$value",
    );
}

/// Verifies the error diagnostic for property redeclaration removes readonly.
#[test]
fn test_error_property_redeclaration_removes_readonly() {
    // child property cannot remove the readonly modifier from the parent.
    expect_error(
        "<?php class Base { public readonly int $value; public function __construct() { $this->value = 1; } } class Child extends Base { public int $value = 5; }",
        "Cannot remove readonly modifier when redeclaring property: Child::$value",
    );
}

/// Verifies the error diagnostic for property redeclaration drops parent type declaration.
#[test]
fn test_error_property_redeclaration_drops_parent_type_declaration() {
    // child property cannot drop a type declaration that the parent declared.
    expect_error(
        "<?php class Base { public int $x = 0; } class Child extends Base { public $x = 5; }",
        "Type of Child::$x must be int (as in class Base)",
    );
}

/// Verifies the error diagnostic for property redeclaration adds type to untyped parent.
#[test]
fn test_error_property_redeclaration_adds_type_to_untyped_parent() {
    // child property cannot add a type when the parent has no type on the property.
    expect_error(
        "<?php class Base { public $x = 0; } class Child extends Base { public int $x = 5; }",
        "Type of Child::$x must not be defined (as in class Base)",
    );
}

/// Verifies the error diagnostic for property redeclaration shadows private parent property.
#[test]
fn test_error_property_redeclaration_shadows_private_parent_property() {
    // private parent properties cannot be shadowed by a child; the feature is not yet supported.
    expect_error(
        "<?php class Base { private int $secret = 1; } class Child extends Base { public int $secret = 2; }",
        "shadowing private parent properties is not yet supported",
    );
}

/// Verifies the error diagnostic for property redeclaration changes by ref qualifier.
#[test]
fn test_error_property_redeclaration_changes_by_ref_qualifier() {
    // child property cannot change the by-reference qualifier from the parent.
    expect_error(
        "<?php class Base { public function __construct(public int &$ref) {} } class Child extends Base { public int $ref = 0; }",
        "Cannot change by-reference qualifier when redeclaring property: Child::$ref",
    );
}

/// Verifies the error diagnostic for abstract property in non abstract class.
#[test]
fn test_error_abstract_property_in_non_abstract_class() {
    // abstract properties are only allowed in abstract classes.
    expect_error(
        "<?php class Box { abstract public int $value { get; } }",
        "Abstract properties can only be declared in abstract classes",
    );
}

/// Verifies the error diagnostic for unhooked abstract property is rejected.
#[test]
fn test_error_unhooked_abstract_property_is_rejected() {
    // a property marked abstract without hooks is rejected; only hooked properties may be abstract.
    expect_error(
        "<?php abstract class Box { abstract public int $value; }",
        "Only hooked properties may be declared abstract",
    );
}

/// Verifies the error diagnostic for concrete class missing abstract trait property.
#[test]
fn test_error_concrete_class_missing_abstract_trait_property() {
    // a concrete class using a trait that declares an abstract hooked property must implement it.
    expect_error(
        "<?php trait HasValue { abstract public int $value { get; } } class Box { use HasValue; }",
        "Concrete class Box must declare abstract property Box::$value",
    );
}

/// Verifies the error diagnostic for interface property without hooks is rejected.
#[test]
fn test_error_interface_property_without_hooks_is_rejected() {
    // interfaces may only declare hooked properties; plain properties are rejected.
    expect_error(
        "<?php interface HasValue { public int $value; }",
        "Interfaces may only include hooked properties",
    );
}

/// Verifies the error diagnostic for interface set property rejects readonly implementation.
#[test]
fn test_error_interface_set_property_rejects_readonly_implementation() {
    // a readonly property cannot satisfy a set-only contract from an interface.
    expect_error(
        "<?php interface Writable { public int $value { set; } } class Box implements Writable { public readonly int $value; public function __construct(int $value) { $this->value = $value; } }",
        "Readonly property Box::$value cannot satisfy set property contract",
    );
}

/// Verifies that writing a get-only (read-only) hooked property is rejected.
#[test]
fn test_error_write_to_get_only_hooked_property() {
    // a property with a get hook but no set hook is read-only; external writes must be rejected.
    expect_error(
        "<?php class C { public int $x { get => 42; } } $c = new C(); $c->x = 5;",
        "Cannot write to read-only hooked property C::x",
    );
}

/// Verifies that the short `set => expr` hook form (which needs a backed property) is rejected.
#[test]
fn test_error_short_set_hook_rejected() {
    // short `set => expr` requires a backed property; only the block form is supported.
    expect_error(
        "<?php class C { private int $n = 0; public int $v { get => $this->n; set => $this->n; } }",
        "Short `set => expr` hooks require a backed property",
    );
}

/// Verifies that declaring the same hook twice on one property is rejected.
#[test]
fn test_error_duplicate_get_hook_rejected() {
    // a property may declare each hook only once.
    expect_error(
        "<?php class C { public int $x { get => 1; get => 2; } }",
        "Duplicate get property hook",
    );
}

/// Verifies that an unknown hook name (not `get`/`set`) is rejected.
#[test]
fn test_error_unknown_property_hook_rejected() {
    // only `get` and `set` are valid property hooks.
    expect_error(
        "<?php class C { public int $x { peek => 1; } }",
        "Unknown property hook 'peek'",
    );
}

/// Verifies that an interface property hook carrying a body is rejected (interface hooks are
/// abstract declarations only).
#[test]
fn test_error_interface_property_hook_with_body_rejected() {
    // interface hooked properties may only declare the hooks, not implement them.
    expect_error(
        "<?php interface I { public int $x { get => 1; } }",
        "Interface property hooks cannot have a body",
    );
}

/// Verifies the error diagnostic for interface property missing uses contract span.
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

/// Verifies the error diagnostic for interface property type mismatch uses implementation span.
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

/// Verifies the error diagnostic for deferred interface property uses contract span.
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

/// Verifies the error diagnostic for abstract property with default.
#[test]
fn test_error_abstract_property_with_default() {
    // abstract hooked properties cannot have a default value.
    expect_error(
        "<?php abstract class Box { abstract public int $value = 1 { get; } }",
        "Abstract property $value cannot have a default value",
    );
}

/// Verifies the error diagnostic for abstract property with static.
#[test]
fn test_error_abstract_property_with_static() {
    // hooked properties cannot be static.
    expect_error(
        "<?php abstract class Box { abstract public static int $value { get; } }",
        "Cannot declare hooks for static property",
    );
}

/// Verifies the error diagnostic for abstract property with final.
#[test]
fn test_error_abstract_property_with_final() {
    // hooked properties cannot also be final.
    expect_error(
        "<?php abstract class Box { abstract final public int $value { get; } }",
        "Cannot use the final modifier on an abstract property",
    );
}

/// Verifies the error diagnostic for abstract property with private.
#[test]
fn test_error_abstract_property_with_private() {
    // private hooked properties are not supported.
    expect_error(
        "<?php abstract class Box { abstract private int $value { get; } }",
        "Private abstract properties are not supported",
    );
}

/// Verifies the error diagnostic for concrete class missing abstract property.
#[test]
fn test_error_concrete_class_missing_abstract_property() {
    // a concrete subclass of an abstract class must implement all abstract hooked properties.
    expect_error(
        "<?php abstract class Shape { abstract public int $sides { get; } } class Triangle extends Shape {}",
        "Concrete class Triangle must declare abstract property Shape::$sides",
    );
}

/// Verifies the error diagnostic for concrete property redeclared as abstract.
#[test]
fn test_error_concrete_property_redeclared_as_abstract() {
    // a property with a concrete implementation in the parent cannot become abstract in the child.
    expect_error(
        "<?php class Base { public int $value = 1; } abstract class Child extends Base { abstract public int $value { get; } }",
        "Cannot make concrete property abstract: Child::$value",
    );
}

/// Verifies that `class` is rejected as a class-constant name, even though other semi-reserved
/// keywords are allowed (PHP reserves `class` for the `Foo::class` name fetch).
#[test]
fn test_error_const_named_class_is_rejected() {
    expect_error(
        "<?php class A { const class = 1; }",
        "Cannot use 'class' as a class constant name",
    );
}

/// Verifies that a non-name token (an operator) after `->` is still rejected even though
/// semi-reserved keywords are now accepted as member names.
#[test]
fn test_error_operator_after_arrow_is_rejected() {
    expect_error(
        "<?php $o = 1; echo $o->+;",
        "Expected property or method name after '->'",
    );
}

/// Verifies that writing a `public private(set)` property from outside the class is rejected,
/// while reading it (not shown) is allowed.
#[test]
fn test_error_asymmetric_visibility_external_write() {
    expect_error(
        "<?php class C { public private(set) int $v = 1; } $c = new C(); $c->v = 9;",
        "Cannot access private property: C::v",
    );
}

/// Verifies that pushing onto a `private(set)` array property from outside the class is rejected.
/// Indirect array modification is a write and must honor the `set` visibility (PHP 8.4).
#[test]
fn test_error_asymmetric_visibility_external_array_push() {
    expect_error(
        "<?php class C { public private(set) array $items = []; } $c = new C(); $c->items[] = 1;",
        "Cannot access private property: C::items",
    );
}

/// Verifies that an indexed write to a `private(set)` array property from outside the class is
/// rejected, honoring the `set` visibility (PHP 8.4).
#[test]
fn test_error_asymmetric_visibility_external_array_index_write() {
    expect_error(
        "<?php class C { public private(set) array $items = []; } $c = new C(); $c->items['k'] = 1;",
        "Cannot access private property: C::items",
    );
}

/// Verifies that a `set` visibility weaker than the `get` visibility is rejected.
#[test]
fn test_error_asymmetric_visibility_set_weaker_than_get() {
    expect_error(
        "<?php class C { private public(set) int $v = 1; }",
        "Asymmetric set visibility must not be weaker than the get visibility",
    );
}

/// Verifies that asymmetric visibility on an untyped property is rejected.
#[test]
fn test_error_asymmetric_visibility_requires_type() {
    expect_error(
        "<?php class C { public private(set) $v = 1; }",
        "Property with asymmetric visibility must have a type",
    );
}

/// Verifies that asymmetric visibility on a static property is rejected.
#[test]
fn test_error_asymmetric_visibility_on_static_property() {
    expect_error(
        "<?php class C { public private(set) static int $v = 1; }",
        "Static property may not declare asymmetric visibility",
    );
}
