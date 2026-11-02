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

/// Dynamic and constructorless stdClass allocation reserve the hash tail used by property writes.
#[test]
fn test_dynamic_stdclass_factories_preserve_property_storage_and_heap_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function dynamicStdClassFactory(string $class): mixed { return new $class(); }
function reflectedStdClassFactory(string $class): mixed {
    return (new ReflectionClass($class))->newInstanceWithoutConstructor();
}
for ($index = 0; $index < 4; $index++) {
    $first = dynamicStdClassFactory('stdClass');
    $second = reflectedStdClassFactory('stdClass');
    $first->value = 'first'; $second->value = 'second';
    echo $first->value, ':', $second->value, '|';
    unset($first, $second);
}
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "first:second|".repeat(4));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

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
