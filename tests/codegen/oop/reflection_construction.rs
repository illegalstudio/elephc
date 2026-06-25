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
$second = $ref->newInstanceArgs(["X", "Y"]);
echo get_class($second) . ":";
$third = $ref->newInstanceWithoutConstructor();
echo get_class($third);
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "IN|AB|ReflectObjectConstructChild:XY|ReflectObjectConstructChild:ReflectObjectConstructChild"
    );
}
