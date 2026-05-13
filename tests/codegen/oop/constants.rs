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

#[test]
fn test_class_constant_int() {
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

#[test]
fn test_class_constant_string() {
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

#[test]
fn test_class_constant_inherited_from_parent() {
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

#[test]
fn test_class_constant_self_access_inside_method() {
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

#[test]
fn test_interface_constant() {
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

#[test]
fn test_class_constant_with_attribute_compiles() {
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
