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

/// Verifies final class constants cannot be redeclared by subclasses.
#[test]
fn test_final_class_constant_override_fails() {
    let err = compile_expect_type_error(
        r#"<?php
class Base {
    final public const LIMIT = 1;
}
class Child extends Base {
    public const LIMIT = 2;
}
"#,
    );
    assert!(err.contains("cannot override final constant"), "{err}");
}

/// Verifies final interface constants cannot be redeclared by child interfaces or implementors.
#[test]
fn test_final_interface_constant_override_fails() {
    let err = compile_expect_type_error(
        r#"<?php
interface Limits {
    final public const MAX = 100;
}
class Bound implements Limits {
    public const MAX = 200;
}
"#,
    );
    assert!(
        err.contains("cannot override final interface constant"),
        "{err}"
    );
}

/// Verifies child interfaces cannot redeclare final parent interface constants.
#[test]
fn test_final_parent_interface_constant_override_fails() {
    let err = compile_expect_type_error(
        r#"<?php
interface Limits {
    final public const MAX = 100;
}
interface ChildLimits extends Limits {
    public const MAX = 200;
}
"#,
    );
    assert!(
        err.contains("cannot override final interface constant"),
        "{err}"
    );
}

/// Verifies private class constants cannot be final.
#[test]
fn test_final_private_class_constant_fails() {
    let err = compile_expect_type_error(
        r#"<?php
class Hidden {
    final private const SECRET = 1;
}
"#,
    );
    assert!(
        err.contains("Private constant Hidden::SECRET cannot be final"),
        "{err}"
    );
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
