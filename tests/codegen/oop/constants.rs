//! Purpose:
//! End-to-end codegen tests for class, interface, trait, and enum constants.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Constant values are inlined by codegen rather than looked up at runtime.
//! - Inheritance and visibility cases cover schema/codegen agreement.

use super::*;

/// Verifies class constant int.
#[test]
fn test_class_constant_int() {
    //! Verifies integer class constant is inlined and accessible via ClassName::CONST.
    let out = compile_and_run(
        r#"<?php
class Math {
    const PI = 314;
}
echo Math::PI;
"#,
    );
    assert_eq!(out, "314");
}

/// Verifies class constant string.
#[test]
fn test_class_constant_string() {
    //! Verifies string class constant is inlined and accessible via ClassName::CONST.
    let out = compile_and_run(
        r#"<?php
class Greet {
    const HELLO = "hi";
}
echo Greet::HELLO;
"#,
    );
    assert_eq!(out, "hi");
}

/// Verifies class constant inherited from parent.
#[test]
fn test_class_constant_inherited_from_parent() {
    //! Verifies child class inherits parent constants via ClassName::CONST lookup.
    let out = compile_and_run(
        r#"<?php
class Base {
    const VERSION = 7;
}
class Child extends Base {}
echo Child::VERSION;
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies class constant expression can reference self constant.
#[test]
fn test_class_constant_expression_can_reference_self_constant() {
    //! Verifies constant expressions can use self:: to reference other constants in the same class.
    let out = compile_and_run(
        r#"<?php
class Box {
    const A = 1;
    const B = self::A + 2;
}
echo Box::B;
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies inherited class constant expression keeps lexical self.
#[test]
fn test_inherited_class_constant_expression_keeps_lexical_self() {
    //! Verifies self:: in a parent constant expression refers to the defining class, not the runtime subclass.
    //! Regression: lexical self must not be replaced with runtime dynamic dispatch.
    let out = compile_and_run(
        r#"<?php
class Base {
    const A = 1;
    const B = self::A + 2;
}
class Child extends Base {
    const A = 10;
}
echo Child::B;
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies class constant expression can reference parent constant.
#[test]
fn test_class_constant_expression_can_reference_parent_constant() {
    //! Verifies constant expressions can use parent:: to access inherited constants.
    let out = compile_and_run(
        r#"<?php
class Base {
    const A = 1;
}
class Child extends Base {
    const B = parent::A + 2;
}
echo Child::B;
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies class constant expression can use self class.
#[test]
fn test_class_constant_expression_can_use_self_class() {
    //! Verifies self::class magic constant works inside a constant expression.
    let out = compile_and_run(
        r#"<?php
class Box {
    const NAME = self::class;
}
echo Box::NAME;
"#,
    );
    assert_eq!(out, "Box");
}

/// Verifies class constant self access inside method.
#[test]
fn test_class_constant_self_access_inside_method() {
    //! Verifies self::CONST inside an instance method resolves to the defining class constant.
    let out = compile_and_run(
        r#"<?php
class Box {
    const SIZE = 42;
    public function describe(): int { return self::SIZE; }
}
$b = new Box();
echo $b->describe();
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies interface constant.
#[test]
fn test_interface_constant() {
    //! Verifies interface constants are accessible through implementing class and via ClassName::CONST.
    let out = compile_and_run(
        r#"<?php
interface Limits {
    const MAX = 100;
}
class Bound implements Limits {
    public function get(): int { return Limits::MAX; }
}
$b = new Bound();
echo $b->get();
"#,
    );
    assert_eq!(out, "100");
}

/// Verifies class constant with attribute compiles.
#[test]
fn test_class_constant_with_attribute_compiles() {
    //! Verifies constants with PHP attributes compile without error; attribute is discarded.
    let out = compile_and_run(
        r#"<?php
class Cfg {
    #[Documented]
    const TIMEOUT = 30;
}
echo Cfg::TIMEOUT;
"#,
    );
    assert_eq!(out, "30");
}

/// Verifies `$obj::class` on a simple variable receiver returns the runtime class name.
/// Regression for the Symfony `polyfill-deepclone` `$v[1]::class` wall: `::class` on an
/// expression receiver (not a named class) desugars to `get_class($expr)`.
#[test]
fn test_expr_class_on_object_variable() {
    //! Verifies `$obj::class` resolves to the object's class name via get_class.
    let out = compile_and_run(
        r#"<?php
class Box {}
$b = new Box();
echo $b::class;
"#,
    );
    assert_eq!(out, "Box");
}

/// Verifies an enum case is a first-class object for `$expr::class` (the DeepClone value is a
/// `\UnitEnum`). A simple-variable enum receiver reports the enum class name via get_class.
#[test]
fn test_expr_class_on_enum_case_variable() {
    //! Verifies `$enumCase::class` resolves to the enum class name.
    let out = compile_and_run(
        r#"<?php
enum Suit { case Hearts; case Spades; }
$c = Suit::Spades;
echo $c::class;
"#,
    );
    assert_eq!(out, "Suit");
}

/// Verifies `$v[i]::class` — `::class` on an array-access expression receiver — the
/// `polyfill-deepclone/DeepClone.php:87` receiver shape (`$v[1]::class`). Uses plain objects
/// as the array elements (enum cases in array literals lose object identity — a separate,
/// pre-existing gap tracked independently of this `::class` fix).
#[test]
fn test_expr_class_on_array_access_receiver() {
    //! Verifies `::class` works on an array-access expression (the DeepClone receiver shape).
    let out = compile_and_run(
        r#"<?php
class Box {}
$v = [new Box(), new Box()];
echo $v[1]::class;
"#,
    );
    assert_eq!(out, "Box");
}

/// Verifies `$obj::class` returns the *runtime* class (the actual instance class), not the
/// declared/variable type — the semantic distinction from the compile-time `Foo::class` form.
/// A Derived stored as Base still reports "Derived", matching PHP `get_class`.
#[test]
fn test_expr_class_returns_runtime_class() {
    //! Verifies `$obj::class` is a runtime get_class, not a compile-time declared type.
    let out = compile_and_run(
        r#"<?php
class Base {}
class Derived extends Base {}
$b = new Derived();
echo $b::class;
"#,
    );
    assert_eq!(out, "Derived");
}

/// Verifies the `polyfill-deepclone` concat shape `$v[1]::class . '::' . <tail>` compiles and
/// concatenates. The real DeepClone tail is `$v[1]->name` (enum-case `->name`, a separate
/// gate); here the tail is a literal so this regression isolates the `$expr::class` wall.
#[test]
fn test_expr_class_concat_shape() {
    //! Verifies `$expr::class` composes in a string concat expression.
    let out = compile_and_run(
        r#"<?php
class Box {}
$b = new Box();
echo $b::class . '::marker';
"#,
    );
    assert_eq!(out, "Box::marker");
}
