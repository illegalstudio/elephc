//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object-oriented PHP instanceof, including instanceof classes and unknown target, instanceof inheritance and interfaces, and instanceof self parent and late static.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_instanceof_classes_and_unknown_target() {
    let out = compile_and_run(
        r#"<?php
class A {}
class B {}
$a = new A();
echo ($a instanceof A) ? "T" : "F";
echo ($a instanceof B) ? "T" : "F";
echo (42 instanceof A) ? "T" : "F";
echo ($a instanceof Missing) ? "T" : "F";
"#,
    );
    assert_eq!(out, "TFFF");
}

#[test]
fn test_instanceof_inheritance_and_interfaces() {
    let out = compile_and_run(
        r#"<?php
interface Named {
    public function name();
}

interface Entity extends Named {
    public function id();
}

class Base {}

class User extends Base implements Entity {
    public function name() { return "user"; }
    public function id() { return 1; }
}

$user = new User();
$base = new Base();
echo ($user instanceof User) ? "T" : "F";
echo ($user instanceof Base) ? "T" : "F";
echo ($user instanceof Entity) ? "T" : "F";
echo ($user instanceof Named) ? "T" : "F";
echo ($base instanceof User) ? "T" : "F";
"#,
    );
    assert_eq!(out, "TTTTF");
}

#[test]
fn test_instanceof_self_parent_and_late_static() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public function check(Base $x) {
        echo ($x instanceof self) ? "S" : "s";
        echo ($x instanceof static) ? "T" : "t";
    }
}

class Child extends Base {
    public function checkParent(Base $x) {
        echo ($x instanceof parent) ? "P" : "p";
    }
}

$base = new Base();
$child = new Child();
$base->check($child);
$child->check($base);
$child->checkParent($child);
"#,
    );
    assert_eq!(out, "STStP");
}

#[test]
fn test_instanceof_lhs_evaluates_once() {
    let out = compile_and_run(
        r#"<?php
class Item {}

class Factory {
    public $count = 0;

    public function make() {
        $this->count = $this->count + 1;
        return new Item();
    }
}

$factory = new Factory();
echo ($factory->make() instanceof Item) ? "T" : "F";
echo $factory->count;
"#,
    );
    assert_eq!(out, "T1");
}

#[test]
fn test_instanceof_handles_mixed_and_nullable_object_values() {
    let out = compile_and_run(
        r#"<?php
interface Named {}
class User implements Named {}

function id(mixed $value): mixed {
    return $value;
}

function maybe(bool $flag): ?User {
    if ($flag) {
        return new User();
    }
    return null;
}

$mixedObject = id(new User());
$mixedScalar = id(7);
echo ($mixedObject instanceof User) ? "T" : "F";
echo ($mixedObject instanceof Named) ? "T" : "F";
echo ($mixedScalar instanceof User) ? "T" : "F";
echo (maybe(true) instanceof User) ? "T" : "F";
echo (maybe(false) instanceof User) ? "T" : "F";
"#,
    );
    assert_eq!(out, "TTFTF");
}

#[test]
fn test_dynamic_instanceof_string_class_and_interface_targets() {
    let out = compile_and_run(
        r#"<?php
interface Named {}
class User implements Named {}
class Other {}

$user = new User();
$className = "User";
$interfaceName = "Named";
$otherName = "Other";
$lowerName = "user";
$absoluteName = "\\User";
$missing = "Missing";

echo ($user instanceof $className) ? "T" : "F";
echo ($user instanceof $interfaceName) ? "T" : "F";
echo ($user instanceof $otherName) ? "T" : "F";
echo ($user instanceof $lowerName) ? "T" : "F";
echo ($user instanceof $absoluteName) ? "T" : "F";
echo ($user instanceof $missing) ? "T" : "F";
"#,
    );
    assert_eq!(out, "TTFTTF");
}

#[test]
fn test_dynamic_instanceof_namespaced_string_targets() {
    let out = compile_and_run(
        r#"<?php
namespace App;
class User {}

$user = new User();
$localName = "User";
$qualifiedName = "App\\User";
$absoluteName = "\\App\\User";

echo ($user instanceof $localName) ? "T" : "F";
echo ($user instanceof $qualifiedName) ? "T" : "F";
echo ($user instanceof $absoluteName) ? "T" : "F";
"#,
    );
    assert_eq!(out, "FTT");
}

#[test]
fn test_dynamic_instanceof_transitive_interface_string_target() {
    let out = compile_and_run(
        r#"<?php
interface Root {}
interface Child extends Root {}
class User implements Child {}

$user = new User();
$target = "Root";
echo ($user instanceof $target) ? "T" : "F";
"#,
    );
    assert_eq!(out, "T");
}

#[test]
fn test_dynamic_instanceof_object_target_uses_target_runtime_class() {
    let out = compile_and_run(
        r#"<?php
class A {}
class B {}

$a = new A();
$targetA = new A();
$targetB = new B();

echo ($a instanceof $targetA) ? "T" : "F";
echo ($a instanceof $targetB) ? "T" : "F";
"#,
    );
    assert_eq!(out, "TF");
}

#[test]
fn test_dynamic_instanceof_mixed_targets_and_scalar_lhs() {
    let out = compile_and_run(
        r#"<?php
class User {}
class Other {}

function id(mixed $value): mixed {
    return $value;
}

function maybe(bool $flag): ?User {
    if ($flag) {
        return new User();
    }
    return null;
}

$target = id("User");
$missingTarget = id("Missing");
$objectTarget = id(new Other());
$object = id(new User());
$scalar = id(7);

echo ($object instanceof $target) ? "T" : "F";
echo ($scalar instanceof $target) ? "T" : "F";
echo ($scalar instanceof $missingTarget) ? "T" : "F";
echo ($scalar instanceof $objectTarget) ? "T" : "F";
echo (maybe(true) instanceof $target) ? "T" : "F";
echo (maybe(false) instanceof $target) ? "T" : "F";
"#,
    );
    assert_eq!(out, "TFFFTF");
}

#[test]
fn test_dynamic_instanceof_parenthesized_expression_target() {
    let out = compile_and_run(
        r#"<?php
class User {}
$user = new User();
$prefix = "Us";
$suffix = "er";
echo ($user instanceof ($prefix . $suffix)) ? "T" : "F";
"#,
    );
    assert_eq!(out, "T");
}

#[test]
fn test_dynamic_instanceof_parenthesized_class_constant_target() {
    let out = compile_and_run(
        r#"<?php
class User {}
$user = new User();
echo ($user instanceof (User::class)) ? "T" : "F";
"#,
    );
    assert_eq!(out, "T");
}

#[test]
fn test_dynamic_instanceof_invalid_target_fails_for_object_lhs() {
    let out = compile_and_run_capture(
        r#"<?php
class User {}
$user = new User();
$target = 42;
echo ($user instanceof $target) ? "T" : "F";
"#,
    );
    assert!(!out.success, "program unexpectedly succeeded: {}", out.stdout);
    assert!(
        out.stderr
            .contains("Fatal error: Class name must be a valid object or a string"),
        "unexpected stderr: {}",
        out.stderr
    );
}

#[test]
fn test_dynamic_instanceof_invalid_target_evaluates_lhs_then_target() {
    let out = compile_and_run_capture(
        r#"<?php
function lhs(): int {
    echo "L";
    return 7;
}

function rhs(): int {
    echo "R";
    return 42;
}

echo (lhs() instanceof (rhs())) ? "T" : "F";
"#,
    );
    assert!(!out.success, "program unexpectedly succeeded: {}", out.stdout);
    assert_eq!(out.stdout, "LR");
    assert!(
        out.stderr
            .contains("Fatal error: Class name must be a valid object or a string"),
        "unexpected stderr: {}",
        out.stderr
    );
}

#[test]
fn test_dynamic_instanceof_null_object_target_fails() {
    let out = compile_and_run_capture(
        r#"<?php
class User {}

function maybe(bool $flag): ?User {
    if ($flag) {
        return new User();
    }
    return null;
}

$user = new User();
$target = maybe(false);
echo ($user instanceof $target) ? "T" : "F";
"#,
    );
    assert!(!out.success, "program unexpectedly succeeded: {}", out.stdout);
    assert!(
        out.stderr
            .contains("Fatal error: Class name must be a valid object or a string"),
        "unexpected stderr: {}",
        out.stderr
    );
}

#[test]
fn test_dynamic_instanceof_invalid_target_fails_for_scalar_lhs() {
    let out = compile_and_run_capture(
        r#"<?php
$value = 7;
$target = 42;
echo ($value instanceof $target) ? "T" : "F";
"#,
    );
    assert!(!out.success, "program unexpectedly succeeded: {}", out.stdout);
    assert!(
        out.stderr
            .contains("Fatal error: Class name must be a valid object or a string"),
        "unexpected stderr: {}",
        out.stderr
    );
}
