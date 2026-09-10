//! Purpose:
//! End-to-end codegen tests for ReflectionClass and ReflectionObject
//! construction helpers over reflected class metadata.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Covers inherited ReflectionObject construction helpers that must route
//!   through the same dynamic class-name lowering as ReflectionClass.

use super::*;

/// Verifies `ReflectionObject` inherits working construction helpers from `ReflectionClass`.
#[test]
fn test_reflection_object_construction_helpers_use_runtime_class() {
    let out = compile_and_run_capture(
        r#"<?php
class ReflectObjectConstructBase {}
class ReflectObjectConstructChild extends ReflectObjectConstructBase {
    public function __construct(string $left = "L", string $right = "R") {
        echo $left . $right . "|";
    }
}

$object = new ReflectObjectConstructChild("I", "N");
$ref = new ReflectionObject($object);
$first = $ref->newInstance("A", "B");
echo get_class($first) . ":";
$second = $ref->newInstanceArgs(["right" => "Y", "left" => "X"]);
echo get_class($second) . ":";
$third = $ref->newInstance(right: "N", left: "M");
echo get_class($third) . ":";
$fourth = $ref->newInstanceWithoutConstructor();
echo get_class($fourth);
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "IN|AB|ReflectObjectConstructChild:XY|ReflectObjectConstructChild:MN|ReflectObjectConstructChild:ReflectObjectConstructChild"
    );
}

/// Verifies `ReflectionClass` construction helpers support inferred constructor signatures.
#[test]
fn test_reflection_class_construction_helpers_call_inferred_constructor_signature() {
    let out = compile_and_run_capture(
        r#"<?php
class ReflectConstructInferredTarget {
    public function __construct($left, $right = "B") {
        echo $left . $right . "|";
    }
}

$ref = new ReflectionClass(ReflectConstructInferredTarget::class);
$ref->newInstance("A", "C");
$ref->newInstanceArgs(["right" => "Y", "left" => "X"]);
$ref->newInstance(right: "N", left: "M");
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "AC|XY|MN|");
}

/// Verifies DOM `ReflectionClass` constructors resolve case-insensitively and
/// transport unknown literal and runtime class names as PHP ReflectionExceptions.
#[test]
fn test_reflection_class_dom_names_and_unknowns_match_php() {
    let out = compile_and_run_capture(
        r#"<?php
$literal = new ReflectionClass("domdocument");
echo $literal->getName() . ":";
echo $literal->isInternal() ? "I" : "i";
echo ":" . ($literal->isInstantiable() ? "Y" : "N") . "|";

$name = "\\DOMDocument";
$dynamic = new ReflectionClass($name);
echo $dynamic->getName() . ":";
echo $dynamic->isInternal() ? "I" : "i";
echo ":" . ($dynamic->isInstantiable() ? "Y" : "N") . "|";

foreach (["\\UnknownDomThing", "dOm\\UnKnOwN"] as $name) {
    try {
        new ReflectionClass($name);
        echo "bad|";
    } catch (Exception $error) {
        echo get_class($error) . ":" . $error->getCode() . ":" . $error->getMessage() . "|";
    }
}
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(
        out.stdout,
        "DOMDocument:I:Y|DOMDocument:I:Y|ReflectionException:-1:Class \"\\UnknownDomThing\" does not exist|ReflectionException:-1:Class \"dOm\\UnKnOwN\" does not exist|"
    );
}
