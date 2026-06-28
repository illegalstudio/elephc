//! Purpose:
//! Integration or regression tests for diagnostic coverage of class and trait diagnostics, including instanceof parent requires parent class, trait method conflict requires insteadof, and trait property conflict must be compatible.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

/// Verifies that `instanceof parent` reports "Class has no parent class" when the class
/// has no parent.
#[test]
fn test_error_instanceof_parent_requires_parent_class() {
    expect_error(
        "<?php class A { public function f(A $x) { return $x instanceof parent; } }",
        "Class has no parent class",
    );
}

/// Verifies that a class using two traits with conflicting method names (both `foo`)
/// reports "ambiguous trait method" when no `insteadof` resolution is provided.
#[test]
fn test_error_trait_method_conflict_requires_insteadof() {
    expect_error(
        r#"<?php
trait A { public function foo() { return 1; } }
trait B { public function foo() { return 2; } }
class C { use A, B; }
"#,
        "ambiguous trait method 'foo'",
    );
}

/// Verifies that a class using two traits with a property of the same name but
/// incompatible visibility reports "incompatible duplicate property".
#[test]
fn test_error_trait_property_conflict_must_be_compatible() {
    expect_error(
        r#"<?php
trait A { public $value = 1; }
trait B { private $value = 1; }
class C { use A, B; }
"#,
        "incompatible duplicate property",
    );
}

/// Verifies that calling a protected trait method on an instance from outside the class
/// hierarchy reports "Cannot access protected method".
#[test]
fn test_error_cannot_access_protected_trait_method_outside_class() {
    expect_error(
        r#"<?php
trait A { public function foo() { return 1; } }
class C { use A { A::foo as protected; } }
$c = new C();
echo $c->foo();
"#,
        "Cannot access protected method",
    );
}

/// Verifies that circular trait composition (trait A uses B, B uses A) is detected
/// and reported as an error.
#[test]
fn test_error_circular_trait_composition() {
    expect_error(
        r#"<?php
trait A { use B; }
trait B { use A; }
class C { use A; }
"#,
        "Circular trait composition detected",
    );
}

/// Verifies that accessing a protected property from outside the class hierarchy
/// reports "Cannot access protected property: Secret::value".
#[test]
fn test_error_cannot_access_protected_property_outside_class() {
    expect_error(
        r#"<?php
class Secret {
    protected $value = 7;
}
$s = new Secret();
echo $s->value;
"#,
        "Cannot access protected property: Secret::value",
    );
}

/// Verifies that accessing a protected method from outside the class hierarchy
/// reports "Cannot access protected method: Secret::hidden".
#[test]
fn test_error_cannot_access_protected_method_outside_class() {
    expect_error(
        r#"<?php
class Secret {
    protected function hidden() {
        return 7;
    }
}
$s = new Secret();
echo $s->hidden();
"#,
        "Cannot access protected method: Secret::hidden",
    );
}

/// Verifies that declaring two classes differing only by case (Box vs box) reports
/// "Duplicate class declaration: box".
#[test]
fn test_error_duplicate_classes_differing_only_by_case() {
    expect_error(
        "<?php class Box {} class box {}",
        "Duplicate class declaration: box",
    );
}

/// Verifies that declaring two interfaces differing only by case reports
/// "Duplicate interface declaration: named".
#[test]
fn test_error_duplicate_interfaces_differing_only_by_case() {
    expect_error(
        "<?php interface Named {} interface named {}",
        "Duplicate interface declaration: named",
    );
}

/// Verifies that declaring two traits differing only by case reports
/// "Duplicate trait declaration: reusable".
#[test]
fn test_error_duplicate_traits_differing_only_by_case() {
    expect_error(
        "<?php trait Reusable {} trait reusable {}",
        "Duplicate trait declaration: reusable",
    );
}

/// Verifies that declaring two enums differing only by case reports
/// "Duplicate class or enum declaration: mode".
#[test]
fn test_error_duplicate_enums_differing_only_by_case() {
    expect_error(
        "<?php enum Mode { case A; } enum mode { case B; }",
        "Duplicate class or enum declaration: mode",
    );
}

/// Verifies that a class with two methods differing only by case (Save vs save) reports
/// "Duplicate method declaration in Box: save".
#[test]
fn test_error_duplicate_methods_differing_only_by_case() {
    expect_error(
        "<?php class Box { public function Save() { return 1; } public function save() { return 2; } }",
        "Duplicate method declaration in Box: save",
    );
}

/// Verifies that calling `parent::boot()` inside a class with no parent reports
/// "Class Solo has no parent class".
#[test]
fn test_error_parent_without_parent_class() {
    expect_error(
        "<?php class Solo { public function boot() { return parent::boot(); } } $s = new Solo(); $s->boot();",
        "Class Solo has no parent class",
    );
}

/// Verifies that a subclass cannot override a final trait method and reports
/// "Cannot override final method Base::run".
#[test]
fn test_error_trait_final_method_cannot_be_overridden_by_subclass() {
    expect_error(
        "<?php trait T { final public function run() { return 1; } } class Base { use T; } class Child extends Base { public function run() { return 2; } }",
        "Cannot override final method Base::run",
    );
}

/// Verifies that a subclass cannot override a final trait property and reports
/// "Cannot override final property Base::$value".
#[test]
fn test_error_trait_final_property_cannot_be_overridden_by_subclass() {
    expect_error(
        "<?php trait T { final public $value; } class Base { use T; } class Child extends Base { public $value; }",
        "Cannot override final property Base::$value",
    );
}

/// Verifies that `self::class` outside a class context reports
/// "Cannot use self::class or static::class outside a class context".
#[test]
fn test_error_self_class_outside_class() {
    expect_error(
        "<?php echo self::class;",
        "Cannot use self::class or static::class outside a class context",
    );
}

/// Verifies that `parent::class` inside a class with no parent reports
/// "Class 'C' has no parent class".
#[test]
fn test_error_parent_class_without_parent() {
    expect_error(
        "<?php class C { public static function name() { return parent::class; } }",
        "Class 'C' has no parent class",
    );
}

/// Verifies that using `static::` in a class constant expression reports
/// "Cannot use static:: in class constant expression".
#[test]
fn test_error_static_constant_reference_in_class_constant_expression() {
    expect_error(
        "<?php class C { const A = 1; const B = static::A + 1; } echo C::B;",
        "Cannot use static:: in class constant expression",
    );
}

/// Verifies that `new static()` on a child with a required constructor parameter
/// reports a missing argument error.
#[test]
fn test_error_new_static_validates_child_constructor() {
    expect_error(
        "<?php class Base { public static function make(): Base { return new static(); } } class Child extends Base { public function __construct(string $name) {} } echo Child::make();",
        "Constructor 'Child::__construct' expects 1 arguments, got 0",
    );
}

/// Verifies the builtin `DatePeriod` constructor enforces its 3-to-4 argument arity.
#[test]
fn test_error_date_period_too_few_args() {
    expect_error(
        "<?php $p = new DatePeriod(new DateTime(\"2024-01-01\"));",
        "Constructor 'DatePeriod::__construct' expects 3 to 4 arguments, got 1",
    );
}

/// Verifies the builtin `DateTime` constructor rejects more than its 0-to-2 arguments.
#[test]
fn test_error_datetime_too_many_args() {
    expect_error(
        "<?php $d = new DateTime(\"now\", null, 3);",
        "Constructor 'DateTime::__construct' expects 0 to 2 arguments, got 3",
    );
}

/// Verifies the builtin `DateTimeImmutable` constructor rejects more than its 0-to-2 arguments.
#[test]
fn test_error_datetime_immutable_too_many_args() {
    expect_error(
        "<?php $d = new DateTimeImmutable(\"now\", null, 3);",
        "Constructor 'DateTimeImmutable::__construct' expects 0 to 2 arguments, got 3",
    );
}

/// Verifies the builtin `DateInterval` constructor requires its single duration-string argument.
#[test]
fn test_error_date_interval_too_few_args() {
    expect_error(
        "<?php $i = new DateInterval();",
        "Constructor 'DateInterval::__construct' expects 1 arguments, got 0",
    );
}

// --- #[\Override] enforcement (PHP 8.3) ---

/// Verifies that `#[Override]` on a method with no matching parent method reports
/// "no matching parent method".
#[test]
fn test_error_override_attribute_with_no_parent_method() {
    expect_error(
        "<?php class Base {} class Child extends Base { #[\\Override] public function nope(): void {} }",
        "no matching parent method",
    );
}

/// Verifies that `#[Override]` on a root class (with no parent) reports
/// "no matching parent method".
#[test]
fn test_error_override_attribute_on_root_class() {
    expect_error(
        "<?php class Solo { #[\\Override] public function alone(): void {} }",
        "no matching parent method",
    );
}

/// Verifies that `#[Override]` on a misspelled method name reports "no matching parent
/// method" rather than silently allowing the typo.
#[test]
fn test_error_override_attribute_on_misspelled_method() {
    expect_error(
        "<?php class Base { public function fetchAll(): void {} } class Child extends Base { #[\\Override] public function fetchAl(): void {} }",
        "no matching parent method",
    );
}

/// Verifies that the unqualified `#[Override]` form (without a leading backslash) is
/// recognized as the PHP 8.3 built-in and enforces parent-method matching.
#[test]
fn test_error_override_attribute_unqualified_form_is_recognized() {
    expect_error(
        "<?php class Base {} class Child extends Base { #[Override] public function nope(): void {} }",
        "no matching parent method",
    );
}

/// Verifies that `#[Override]` imported under an alias (e.g., `use Override as
/// MustOverride`) is still recognized as the built-in and enforces parent-method matching.
#[test]
fn test_error_override_attribute_import_alias_is_recognized() {
    expect_error(
        "<?php use Override as MustOverride; class Base {} class Child extends Base { #[MustOverride] public function nope(): void {} }",
        "no matching parent method",
    );
}

/// Verifies that a namespaced user attribute that looks like `Foo\Override` is NOT
/// treated as the PHP 8.3 built-in `#[Override]` and therefore does not enforce
/// parent-method matching.
#[test]
fn test_override_attribute_qualified_lookalike_is_not_builtin() {
    check_source(
        "<?php class Solo { #[Foo\\Override] public function alone(): void {} }",
    )
    .expect("qualified user attribute should not enforce #[\\Override]");
}

/// Verifies that a namespaced `#[Override]` attribute is NOT treated as the PHP 8.3
/// built-in when the `Override` class does not resolve to the built-in, so no
/// parent-method enforcement occurs.
#[test]
fn test_override_attribute_namespaced_unqualified_lookalike_is_not_builtin() {
    check_source(
        "<?php namespace N; class Solo { #[Override] public function alone(): void {} }",
    )
    .expect("namespaced user attribute should not enforce #[\\Override]");
}

/// Verifies that `#[Override]` on a static method with no matching parent static method
/// reports "no matching parent method".
#[test]
fn test_error_override_attribute_on_static_with_no_parent() {
    expect_error(
        "<?php class Base {} class Child extends Base { #[\\Override] public static function gone(): void {} }",
        "no matching parent method",
    );
}

/// Verifies that `#[AllowDynamicProperties]` inside a namespace is treated as a
/// user-defined attribute (not the built-in), so dynamic properties are rejected
/// with "Undefined property".
#[test]
fn test_allow_dynamic_properties_namespaced_unqualified_lookalike_is_not_builtin() {
    expect_error(
        "<?php namespace N; #[AllowDynamicProperties] class Bag {} $b = new Bag(); $b->x = 1;",
        "Undefined property: N\\Bag::x",
    );
}

// --- class_attribute_names() argument validation ---

/// Verifies that `class_attribute_names()` with an undefined class reports
/// "undefined class 'DoesNotExist'".
#[test]
fn test_error_class_attribute_names_undefined_class() {
    expect_error(
        "<?php $x = class_attribute_names('DoesNotExist');",
        "undefined class 'DoesNotExist'",
    );
}

/// Verifies that `class_attribute_names()` with a dynamic (variable) argument instead
/// of a string literal reports "requires a string literal class name".
#[test]
fn test_error_class_attribute_names_dynamic_argument() {
    expect_error(
        "<?php $name = 'Foo'; class_attribute_names($name);",
        "requires a string literal class name",
    );
}

/// Verifies that `class_attribute_names()` with no argument reports "exactly 1
/// argument".
#[test]
fn test_error_class_attribute_names_no_argument() {
    expect_error(
        "<?php class_attribute_names();",
        "exactly 1 argument",
    );
}

/// Verifies that `class_attribute_names()` with a non-string argument (e.g., integer)
/// reports "must be a string class name".
#[test]
fn test_error_class_attribute_names_non_string_argument() {
    expect_error(
        "<?php class_attribute_names(42);",
        "must be a string class name",
    );
}

// --- class_attribute_args() argument validation ---

/// Verifies that `class_attribute_args()` with an undefined class reports
/// "undefined class 'DoesNotExist'".
#[test]
fn test_error_class_attribute_args_undefined_class() {
    expect_error(
        "<?php $x = class_attribute_args('DoesNotExist', 'Foo');",
        "undefined class 'DoesNotExist'",
    );
}

/// Verifies that `class_attribute_args()` with a dynamic class name argument reports
/// "requires a string literal class name".
#[test]
fn test_error_class_attribute_args_dynamic_class_argument() {
    expect_error(
        "<?php $name = 'Foo'; class_attribute_args($name, 'Bar');",
        "requires a string literal class name",
    );
}

/// Verifies that `class_attribute_args()` with a dynamic attribute name argument reports
/// "requires a string literal attribute name".
#[test]
fn test_error_class_attribute_args_dynamic_attr_argument() {
    expect_error(
        "<?php #[Foo] class C {} $name = 'Foo'; class_attribute_args('C', $name);",
        "requires a string literal attribute name",
    );
}

/// Verifies that `class_attribute_args()` called with only one argument (instead of
/// two) reports "exactly 2 arguments".
#[test]
fn test_error_class_attribute_args_wrong_arity() {
    expect_error(
        "<?php class_attribute_args('Foo');",
        "exactly 2 arguments",
    );
}

/// Verifies that `class_attribute_args()` with a non-string first argument reports
/// "first argument must be a string class name".
#[test]
fn test_error_class_attribute_args_non_string_class() {
    expect_error(
        "<?php class_attribute_args(1, 'Foo');",
        "first argument must be a string class name",
    );
}

/// Verifies that `class_attribute_args()` with a non-string second argument reports
/// "second argument must be a string attribute name".
#[test]
fn test_error_class_attribute_args_non_string_attr() {
    expect_error(
        "<?php #[Foo] class C {} class_attribute_args('C', 1);",
        "second argument must be a string attribute name",
    );
}

/// Verifies that `class_attribute_args()` on an attribute with named arguments reports
/// "requested attribute uses argument metadata that is not supported yet".
#[test]
fn test_error_class_attribute_named_args_are_not_silently_dropped() {
    expect_error(
        "<?php #[Foo(name: \"Ada\")] class C {} class_attribute_args('C', 'Foo');",
        "requested attribute uses argument metadata that is not supported yet",
    );
}

/// Verifies that `class_attribute_args()` on an attribute with expression arguments
/// (e.g., `1 + 2`) reports "requested attribute uses argument metadata that is not
/// supported yet".
#[test]
fn test_error_class_attribute_expression_args_are_not_silently_dropped() {
    expect_error(
        "<?php #[Foo(1 + 2)] class C {} class_attribute_args('C', 'Foo');",
        "requested attribute uses argument metadata that is not supported yet",
    );
}

/// Verifies that `class_get_attributes()` on a class with a still-unsupported
/// (non-foldable arithmetic) attribute argument reports "class has attribute
/// argument metadata that is not supported yet" rather than silently dropping it.
/// Float arguments are now supported, so this guards a genuinely unsupported shape.
#[test]
fn test_error_class_get_attributes_unsupported_arg_not_silently_dropped() {
    expect_error(
        "<?php #[Foo(1 + 2)] class C {} class_get_attributes('C');",
        "class has attribute argument metadata that is not supported yet",
    );
}

// --- class_get_attributes() argument validation ---

/// Verifies that `class_get_attributes()` with an undefined class reports
/// "undefined class 'DoesNotExist'".
#[test]
fn test_error_class_get_attributes_undefined_class() {
    expect_error(
        "<?php $x = class_get_attributes('DoesNotExist');",
        "undefined class 'DoesNotExist'",
    );
}

/// Verifies that `class_get_attributes()` with a dynamic (variable) argument reports
/// "requires a string literal class name".
#[test]
fn test_error_class_get_attributes_dynamic_argument() {
    expect_error(
        "<?php $name = 'Foo'; class_get_attributes($name);",
        "requires a string literal class name",
    );
}

/// Verifies that `class_get_attributes()` with no argument reports "exactly 1
/// argument".
#[test]
fn test_error_class_get_attributes_no_argument() {
    expect_error(
        "<?php class_get_attributes();",
        "exactly 1 argument",
    );
}

/// Verifies that `class_get_attributes()` with a non-string argument reports
/// "must be a string class name".
#[test]
fn test_error_class_get_attributes_non_string_argument() {
    expect_error(
        "<?php class_get_attributes(42);",
        "must be a string class name",
    );
}

/// Verifies that declaring a class named `ReflectionAttribute` reports
/// "Cannot redeclare built-in reflection type: ReflectionAttribute".
#[test]
fn test_error_reflection_attribute_redeclaration() {
    expect_error(
        "<?php class ReflectionAttribute {}",
        "Cannot redeclare built-in reflection type: ReflectionAttribute",
    );
}

/// Verifies that declaring an interface named `ReflectionAttribute` reports
/// "Cannot redeclare built-in reflection type: ReflectionAttribute".
#[test]
fn test_error_reflection_attribute_interface_redeclaration() {
    expect_error(
        "<?php interface ReflectionAttribute {}",
        "Cannot redeclare built-in reflection type: ReflectionAttribute",
    );
}

/// Verifies that declaring a trait named `ReflectionAttribute` reports
/// "Cannot redeclare built-in reflection type: ReflectionAttribute".
#[test]
fn test_error_reflection_attribute_trait_redeclaration() {
    expect_error(
        "<?php trait ReflectionAttribute {}",
        "Cannot redeclare built-in reflection type: ReflectionAttribute",
    );
}

/// Verifies that `new ReflectionAttribute()` reports "Cannot access private
/// constructor: ReflectionAttribute::__construct".
#[test]
fn test_error_reflection_attribute_constructor_is_private() {
    expect_error(
        "<?php $r = new ReflectionAttribute();",
        "Cannot access private constructor: ReflectionAttribute::__construct",
    );
}

/// Verifies that accessing `ReflectionAttribute::__name` property reports
/// "Cannot access private property: ReflectionAttribute::__name".
#[test]
fn test_error_reflection_attribute_internal_properties_are_private() {
    expect_error(
        "<?php #[A] class C {} $attrs = class_get_attributes('C'); echo $attrs[0]->__name;",
        "Cannot access private property: ReflectionAttribute::__name",
    );
}

/// Verifies that declaring a class named `ReflectionClass` reports
/// "Cannot redeclare built-in reflection type: ReflectionClass".
#[test]
fn test_error_reflection_class_redeclaration() {
    expect_error(
        "<?php class ReflectionClass {}",
        "Cannot redeclare built-in reflection type: ReflectionClass",
    );
}

/// Verifies that `new ReflectionClass('Missing')` reports "undefined class 'Missing'".
#[test]
fn test_error_reflection_class_undefined_class() {
    expect_error(
        "<?php $r = new ReflectionClass('Missing');",
        "ReflectionClass::__construct(): undefined class 'Missing'",
    );
}

/// Verifies that `new ReflectionClass($name)` with a dynamic variable reports
/// "requires a string literal class name".
#[test]
fn test_error_reflection_class_dynamic_argument() {
    expect_error(
        "<?php $name = 'C'; class C {} $r = new ReflectionClass($name);",
        "requires a string literal class name",
    );
}

/// Verifies that `new ReflectionMethod('C', 'missing')` on an undefined method reports
/// "undefined method 'C::missing'".
#[test]
fn test_error_reflection_method_undefined_method() {
    expect_error(
        "<?php class C {} $r = new ReflectionMethod('C', 'missing');",
        "undefined method 'C::missing'",
    );
}

/// Verifies that `new ReflectionProperty('C', 'missing')` on an undefined property
/// reports "undefined property 'C::$missing'".
#[test]
fn test_error_reflection_property_undefined_property() {
    expect_error(
        "<?php class C {} $r = new ReflectionProperty('C', 'missing');",
        "undefined property 'C::$missing'",
    );
}

/// Verifies that `new ReflectionMethod` on a method with unsupported attribute
/// argument metadata reports "method has attribute argument metadata that is not
/// supported yet".
#[test]
fn test_error_reflection_method_unsupported_attribute_args() {
    expect_error(
        "<?php class C { #[A(1 + 2)] public function f() {} } $r = new ReflectionMethod('C', 'f');",
        "method has attribute argument metadata that is not supported yet",
    );
}

/// Verifies that an anonymous class missing its body is rejected with a clear diagnostic.
#[test]
fn test_error_anonymous_class_missing_body() {
    expect_error(
        "<?php $o = new class;",
        "Expected '{' to open anonymous class body",
    );
}

/// Verifies that a nullsafe dynamic method call (`$obj?->$m()`) is rejected (not yet supported).
#[test]
fn test_error_nullsafe_dynamic_method_call() {
    expect_error(
        "<?php $obj?->$m();",
        "Nullsafe dynamic method calls are not supported yet",
    );
}
