//! Purpose:
//! End-to-end codegen tests for ReflectionFunction invocation paths over AOT
//! function metadata.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `ReflectionFunction::invoke()` and `invokeArgs()` are lowered only for
//!   statically-known reflectors whose target function has declared parameter
//!   types.
//! - Tests cover inline constructors, local tracking, case-insensitive function
//!   names, defaults, named arguments, and static argument arrays.

use super::*;

/// Verifies `ReflectionFunction` exposes AOT function name and origin metadata.
#[test]
fn test_reflection_function_reports_aot_name_origin_predicates() {
    let out = compile_and_run(
        r#"<?php
namespace ReflectFunctionMetaNs;

function sample(): void {}

$ref = new \ReflectionFunction("ReflectFunctionMetaNs\\sample");
echo $ref->getName() . ":";
echo $ref->getShortName() . ":";
echo $ref->getNamespaceName() . ":";
echo ($ref->inNamespace() ? "Y" : "N") . ":";
echo ($ref->isInternal() ? "I" : "i") . ":";
echo $ref->isUserDefined() ? "U" : "u";
"#,
    );
    assert_eq!(out, "ReflectFunctionMetaNs\\sample:sample:ReflectFunctionMetaNs:Y:i:U");
}

/// Verifies `ReflectionFunction::invoke()` calls declared AOT functions.
#[test]
fn test_reflection_function_invoke_calls_declared_aot_functions() {
    let out = compile_and_run(
        r#"<?php
function reflect_function_invoke_target(string $left, string $right = "B"): string {
    return $left . $right;
}

function reflect_function_invoke_zero(): string {
    return "Z";
}

echo (new ReflectionFunction("REFLECT_FUNCTION_INVOKE_TARGET"))->invoke("A", "C");
echo ":";
echo (new ReflectionFunction(function: "\\reflect_function_invoke_target"))->invoke(right: "Y", left: "X");
echo ":";
$ref = new ReflectionFunction("reflect_function_invoke_target");
echo $ref->invoke("L");
echo ":";
echo (new ReflectionFunction("reflect_function_invoke_zero"))->invoke();
"#,
    );
    assert_eq!(out, "AC:XY:LB:Z");
}

/// Verifies `ReflectionFunction::invokeArgs()` forwards static argument arrays.
#[test]
fn test_reflection_function_invoke_args_calls_declared_aot_functions() {
    let out = compile_and_run(
        r#"<?php
function reflect_function_invoke_args_target(string $left, string $right = "B"): string {
    return $left . $right;
}

echo (new ReflectionFunction("reflect_function_invoke_args_target"))->invokeArgs(["right" => "Y", "left" => "X"]);
echo ":";
$localArgs = ["right" => "P", "left" => "O"];
$ref = new ReflectionFunction("reflect_function_invoke_args_target");
echo $ref->invokeArgs($localArgs);
echo ":";
echo $ref->invokeArgs(...[["A", "C"]]);
echo ":";
echo $ref->invokeArgs(args: ["Q"]);
"#,
    );
    assert_eq!(out, "XY:OP:AC:QB");
}

/// Verifies inferred AOT function signatures are rejected instead of miscompiled.
#[test]
fn test_reflection_function_invoke_rejects_inferred_aot_signature() {
    let err = compile_and_run_expect_failure(
        r#"<?php
function reflect_function_invoke_inferred($left, $right) {
    return $left . $right;
}

echo (new ReflectionFunction("reflect_function_invoke_inferred"))->invoke("A", "B");
"#,
    );
    assert!(
        err.contains("Fatal error: unsupported ReflectionFunction::invoke()"),
        "unexpected stderr: {err}"
    );
}
