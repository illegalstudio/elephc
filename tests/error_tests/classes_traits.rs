use super::*;

#[test]
fn test_error_instanceof_parent_requires_parent_class() {
    expect_error(
        "<?php class A { public function f(A $x) { return $x instanceof parent; } }",
        "Class has no parent class",
    );
}

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

#[test]
fn test_error_duplicate_classes_differing_only_by_case() {
    expect_error(
        "<?php class Box {} class box {}",
        "Duplicate class declaration: box",
    );
}

#[test]
fn test_error_duplicate_interfaces_differing_only_by_case() {
    expect_error(
        "<?php interface Named {} interface named {}",
        "Duplicate interface declaration: named",
    );
}

#[test]
fn test_error_duplicate_traits_differing_only_by_case() {
    expect_error(
        "<?php trait Reusable {} trait reusable {}",
        "Duplicate trait declaration: reusable",
    );
}

#[test]
fn test_error_duplicate_enums_differing_only_by_case() {
    expect_error(
        "<?php enum Mode { case A; } enum mode { case B; }",
        "Duplicate class or enum declaration: mode",
    );
}

#[test]
fn test_error_duplicate_methods_differing_only_by_case() {
    expect_error(
        "<?php class Box { public function Save() { return 1; } public function save() { return 2; } }",
        "Duplicate method declaration in Box: save",
    );
}

#[test]
fn test_error_parent_without_parent_class() {
    expect_error(
        "<?php class Solo { public function boot() { return parent::boot(); } } $s = new Solo(); $s->boot();",
        "Class Solo has no parent class",
    );
}

#[test]
fn test_error_trait_final_method_cannot_be_overridden_by_subclass() {
    expect_error(
        "<?php trait T { final public function run() { return 1; } } class Base { use T; } class Child extends Base { public function run() { return 2; } }",
        "Cannot override final method Base::run",
    );
}

#[test]
fn test_error_trait_final_property_cannot_be_overridden_by_subclass() {
    expect_error(
        "<?php trait T { final public $value; } class Base { use T; } class Child extends Base { public $value; }",
        "Cannot override final property Base::$value",
    );
}

#[test]
fn test_error_self_class_outside_class() {
    expect_error(
        "<?php echo self::class;",
        "Cannot use self::class or static::class outside a class context",
    );
}

#[test]
fn test_error_parent_class_without_parent() {
    expect_error(
        "<?php class C { public static function name() { return parent::class; } }",
        "Class 'C' has no parent class",
    );
}

#[test]
fn test_error_new_static_validates_child_constructor() {
    expect_error(
        "<?php class Base { public static function make(): Base { return new static(); } } class Child extends Base { public function __construct(string $name) {} } echo Child::make();",
        "Constructor 'Child::__construct' expects 1 arguments, got 0",
    );
}
