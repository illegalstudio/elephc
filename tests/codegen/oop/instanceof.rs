//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object-oriented PHP instanceof, including instanceof classes and unknown target, instanceof inheritance and interfaces, and instanceof self parent and late static.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Tests instanceof with known class names, unknown class names, and non-object LHS.
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

/// Tests instanceof with class inheritance hierarchies and interface implementations.
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

/// Tests instanceof with self, static, and parent keywords in class methods.
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

/// Verifies that the LHS object expression is evaluated exactly once by calling a
/// factory method with observable side effects.
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

/// Tests instanceof against mixed-type values and nullable object return types.
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

/// Tests dynamic instanceof with string variables naming classes and interfaces,
/// including case-insensitive lookup and absolute names.
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

/// Tests dynamic instanceof with string variables naming namespaced classes.
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

/// Tests dynamic instanceof with a string naming an interface that the object
/// implements transitively through a child interface.
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

/// Tests that when the target of dynamic instanceof is an object (not a string),
/// the runtime class of the target object is used for the check.
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

/// Tests dynamic instanceof with mixed-type LHS and targets, including null targets
/// from nullable return types.
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

/// Tests dynamic instanceof where the target is a parenthesized string concatenation.
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

/// Tests dynamic instanceof where the target is a parenthesized class constant expression.
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

/// Tests that dynamic instanceof with a non-string, non-object target (integer) fails
/// with a Fatal error when the LHS is an object.
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

/// Verifies that when an invalid target causes a Fatal error, both the LHS and
/// the target expression are evaluated in source order before the error is raised.
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

/// Tests that dynamic instanceof with a null target (from a nullable return) fails
/// with a Fatal error.
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

/// Tests that dynamic instanceof with an invalid target (integer) fails with a Fatal
/// error when the LHS is a scalar.
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

/// Verifies is_a()/is_subclass_of() resolve at runtime for a boxed Mixed receiver (H4): a
/// heterogeneous array makes its elements Mixed, so the relation must be computed from the
/// object's runtime class id, not folded to false. Covers exact class, subclass, interface,
/// a non-object element, and is_subclass_of's proper-subclass (self-excluded) semantics.
#[test]
fn test_is_a_runtime_mixed_receiver() {
    let out = compile_and_run(
        r#"<?php
class Animal {}
class Dog extends Animal {}
interface Speaker {}
class Parrot implements Speaker {}
$t = [new Dog(), new Parrot(), 42, "x"];
$ok = is_a($t[0], "Dog")
   && is_a($t[0], "Animal")
   && !is_a($t[0], "Speaker")
   && is_a($t[1], "Speaker")
   && !is_a($t[2], "Animal")
   && is_subclass_of($t[0], "Animal")
   && !is_subclass_of($t[0], "Dog");
echo $ok ? "ok" : "bad";
"#,
    );
    assert_eq!(out, "ok");
}
