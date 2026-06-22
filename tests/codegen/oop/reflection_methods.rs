//! Purpose:
//! End-to-end codegen tests for ReflectionMethod invocation paths over AOT
//! class metadata.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `ReflectionMethod::invoke()` is lowered only for statically-known
//!   reflectors whose target method has declared parameter types.
//! - Tests cover inline constructors, ReflectionClass lookup, local tracking,
//!   case-insensitive method names, default values, and named arguments.

use super::*;

/// Verifies `ReflectionMethod::invoke()` calls declared AOT instance and static methods.
#[test]
fn test_reflection_method_invoke_calls_declared_aot_methods() {
    let out = compile_and_run(
        r#"<?php
class ReflectInvokeTarget {
    public function join(string $a, string $b = "B"): string { return $a . $b; }
    public static function make(string $left, string $right = "S"): string { return $left . $right; }
}

$object = new ReflectInvokeTarget();
echo (new ReflectionMethod(ReflectInvokeTarget::class, "join"))->invoke($object, "A", "C");
echo ":";
echo (new ReflectionMethod(ReflectInvokeTarget::class, "JOIN"))->invoke($object, "D");
echo ":";
echo (new ReflectionClass(ReflectInvokeTarget::class))->getMethod("join")->invoke($object, "E", "F");
echo ":";
echo (new ReflectionMethod(ReflectInvokeTarget::class, "make"))->invoke(null, right: "Y", left: "X");
echo ":";
$method = new ReflectionMethod(ReflectInvokeTarget::class, "join");
echo $method->invoke($object, "L", "M");
"#,
    );
    assert_eq!(out, "AC:DB:EF:XY:LM");
}

/// Verifies inferred AOT method signatures are rejected instead of miscompiled.
#[test]
fn test_reflection_method_invoke_rejects_inferred_aot_signature() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class ReflectInvokeInferredTarget {
    public function join($a, $b) { return $a . $b; }
}
$object = new ReflectInvokeInferredTarget();
echo (new ReflectionMethod(ReflectInvokeInferredTarget::class, "join"))->invoke($object, "A", "B");
"#,
    );
    assert!(
        err.contains("Fatal error: unsupported ReflectionMethod::invoke()"),
        "unexpected stderr: {err}"
    );
}
