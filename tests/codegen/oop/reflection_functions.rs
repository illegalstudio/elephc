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

/// Verifies AOT `ReflectionFunction` exposes function-abstract predicate metadata.
#[test]
fn test_reflection_function_reports_aot_function_abstract_predicates() {
    let out = compile_and_run(
        r#"<?php
#[Deprecated]
function reflect_function_meta_deprecated(): void {}
function reflect_function_meta_generator() { yield 1; }
function reflect_function_meta_plain(): void {}

$deprecated = new ReflectionFunction("reflect_function_meta_deprecated");
$generator = new ReflectionFunction("reflect_function_meta_generator");
$plain = new ReflectionFunction("reflect_function_meta_plain");
echo ($deprecated->isDeprecated() ? "D" : "d") . ":";
echo ($plain->isDeprecated() ? "D" : "d") . ":";
echo ($generator->isGenerator() ? "G" : "g") . ":";
echo ($plain->isGenerator() ? "G" : "g") . ":";
echo ($plain->isClosure() ? "C" : "c") . ":";
echo ($plain->returnsReference() ? "R" : "r") . ":";
echo ($plain->hasTentativeReturnType() ? "H" : "h") . ":";
echo ($plain->getTentativeReturnType() === null ? "Q" : "q") . ":";
echo $plain->isDisabled() ? "X" : "x";
"#,
    );
    assert_eq!(out, "D:d:G:g:c:r:h:Q:x");
}

/// Verifies `ReflectionFunction` exposes declared AOT return type metadata.
#[test]
fn test_reflection_function_reports_aot_return_type_metadata() {
    let out = compile_and_run(
        r#"<?php
function reflect_return_named(?int $value): ?int { return $value; }
function reflect_return_union(): int|string { return 1; }
function reflect_return_never(): never { throw new Exception("stop"); }
function reflect_return_plain() {}

$namedRef = new ReflectionFunction("reflect_return_named");
$named = $namedRef->getReturnType();
echo ($namedRef->hasReturnType() ? "T" : "t") . ":";
echo $named->getName() . ":";
echo ($named->allowsNull() ? "N" : "n") . ":";
echo ($named->isBuiltin() ? "B" : "b") . ":";
$declaring = $namedRef->getParameters()[0]->getDeclaringFunction()->getReturnType();
echo $declaring->getName() . ":";
$union = (new ReflectionFunction("reflect_return_union"))->getReturnType();
if ($union instanceof ReflectionUnionType) {
    echo count($union->getTypes()) . ":";
    foreach ($union->getTypes() as $type) {
        echo $type->getName();
        echo $type->isBuiltin() ? "B" : "b";
    }
} else {
    echo "not-union";
}
echo ":";
$never = (new ReflectionFunction("reflect_return_never"))->getReturnType();
echo $never->getName() . ":";
echo ($never->allowsNull() ? "N" : "n") . ":";
echo ($never->isBuiltin() ? "B" : "b") . ":";
$plain = new ReflectionFunction("reflect_return_plain");
echo ($plain->hasReturnType() ? "P" : "p") . ":";
echo $plain->getReturnType() === null ? "Q" : "q";
"#,
    );
    assert_eq!(out, "T:int:N:B:int:2:intBstringB:never:n:B:p:Q");
}

/// Verifies `ReflectionFunction::isVariadic()` reports the function-level variadic flag.
#[test]
fn test_reflection_function_reports_aot_variadic_flag() {
    let out = compile_and_run(
        r#"<?php
function reflect_variadic_function(string $head, string ...$tail): void {}
function reflect_fixed_function(string $head): void {}

$variadic = new ReflectionFunction("reflect_variadic_function");
$fixed = new ReflectionFunction("reflect_fixed_function");
echo ($variadic->isVariadic() ? "V" : "v") . ":";
echo $variadic->getNumberOfParameters() . ":";
echo ($fixed->isVariadic() ? "V" : "v");
"#,
    );
    assert_eq!(out, "V:2:v");
}

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

/// Verifies `ReflectionFunction` exposes supported callable-builtin metadata.
#[test]
fn test_reflection_function_reports_builtin_metadata() {
    let out = compile_and_run(
        r#"<?php
$ref = new ReflectionFunction("STRLEN");
echo $ref->getName() . ":";
echo $ref->getShortName() . ":";
echo ($ref->isInternal() ? "I" : "i") . ":";
echo ($ref->isUserDefined() ? "U" : "u") . ":";
echo ($ref->hasReturnType() ? "T" : "t") . ":";
echo $ref->getReturnType()->getName() . ":";
$params = $ref->getParameters();
echo count($params) . ":";
echo $params[0]->getName() . ":";
echo ($params[0]->hasType() ? "P" : "p") . ":";
echo $params[0]->getType()->getName() . ":";
echo ($params[0]->getDeclaringFunction()->isInternal() ? "D" : "d") . ":";
echo (new ReflectionParameter("strlen", "string"))->getDeclaringFunction()->getName();
"#,
    );
    assert_eq!(out, "strlen:strlen:I:u:T:int:1:string:P:string:D:strlen");
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
