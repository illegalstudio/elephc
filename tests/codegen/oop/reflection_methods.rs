//! Purpose:
//! End-to-end codegen tests for ReflectionMethod invocation paths over AOT
//! class metadata.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `ReflectionMethod::invoke()` and `invokeArgs()` are lowered only for statically-known
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

/// Verifies `ReflectionMethod::invokeArgs()` forwards static argument arrays.
#[test]
fn test_reflection_method_invoke_args_calls_declared_aot_methods() {
    let out = compile_and_run(
        r#"<?php
class ReflectInvokeArgsTarget {
    public function join(string $left, string $right = "B"): string { return $left . $right; }
    public static function make(string $left, string $right = "S"): string { return $left . $right; }
}

$object = new ReflectInvokeArgsTarget();
echo (new ReflectionMethod(ReflectInvokeArgsTarget::class, "join"))->invokeArgs($object, ["right" => "Y", "left" => "X"]);
echo ":";
echo (new ReflectionMethod(ReflectInvokeArgsTarget::class, "JOIN"))->invokeArgs($object, ["Q"]);
echo ":";
echo (new ReflectionMethod(ReflectInvokeArgsTarget::class, "make"))->invokeArgs(null, ["right" => "N", "left" => "M"]);
echo ":";
echo (new ReflectionClass(ReflectInvokeArgsTarget::class))->getMethod("join")->invokeArgs(object: $object, args: ["left" => "L"]);
echo ":";
$method = new ReflectionMethod(ReflectInvokeArgsTarget::class, "join");
echo $method->invokeArgs(...[$object, ["A", "C"]]);
"#,
    );
    assert_eq!(out, "XY:QB:MN:LB:AC");
}

/// Verifies constructors returned by `ReflectionClass::getConstructor()` can be invoked.
#[test]
fn test_reflection_method_invoke_calls_aot_constructor_from_reflection_class() {
    let out = compile_and_run(
        r#"<?php
class ReflectInvokeCtorTarget {
    public string $label = "";
    public function __construct(string $left, string $right = "B") {
        $this->label = $left . $right;
    }
    public function label(): string {
        return $this->label;
    }
}

$object = new ReflectInvokeCtorTarget("A", "A");
$result = (new ReflectionClass(ReflectInvokeCtorTarget::class))->getConstructor()->invoke($object, "X", "Y");
echo ($result === null ? "null" : "value") . ":" . $object->label();
echo ":";
$ctor = (new ReflectionClass(ReflectInvokeCtorTarget::class))->getConstructor();
$ctor->invokeArgs($object, ["right" => "N", "left" => "M"]);
echo $object->label();
"#,
    );
    assert_eq!(out, "null:XY:MN");
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
