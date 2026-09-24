//! Purpose:
//! End-to-end codegen tests for ReflectionFunction invocation paths over AOT
//! function metadata.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `ReflectionFunction::invoke()` and `invokeArgs()` are lowered for
//!   statically-known reflectors whose target user function has declared
//!   parameter types, plus supported callable builtins.
//! - Tests cover inline constructors, local tracking, case-insensitive function
//!   names, defaults, named arguments, and static argument arrays.

use super::*;

/// Packed-or-hash storage reflects as one PHP type, with composite type methods called after narrowing.
#[test]
fn test_reflection_function_array_storage_has_one_php_type() {
    let out = compile_and_run(
        r#"<?php
function reflectArrayType(array $items): array { return $items; }
function reflectNullableArrayType(?array $items): ?array { return $items; }
function reflectUnionArrayType(array|string $items): array|string { return $items; }
$plain = new ReflectionFunction("reflectArrayType");
$nullable = new ReflectionFunction("reflectNullableArrayType");
$union = new ReflectionFunction("reflectUnionArrayType");
echo get_class($plain->getParameters()[0]->getType()), ":", $plain->getParameters()[0]->getType(),
    ":", $plain->getReturnType(), "|";
echo get_class($nullable->getParameters()[0]->getType()), ":", $nullable->getParameters()[0]->getType(),
    ":", $nullable->getReturnType(), "|";
$parameterType = $union->getParameters()[0]->getType();
$returnType = $union->getReturnType();
echo get_class($parameterType), ":";
if ($parameterType instanceof ReflectionUnionType) {
    echo count($parameterType->getTypes());
} else {
    echo "not-parameter-union";
}
echo ":";
if ($returnType instanceof ReflectionUnionType) {
    echo count($returnType->getTypes());
} else {
    echo "not-return-union";
}
"#,
    );
    assert_eq!(out, "ReflectionNamedType:array:array|ReflectionNamedType:?array:?array|ReflectionUnionType:2:2");
}

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

/// Verifies `ReflectionFunction::invoke()` and `invokeArgs()` call supported builtins.
#[test]
fn test_reflection_function_invoke_calls_builtin_functions() {
    let out = compile_and_run(
        r#"<?php
echo (new ReflectionFunction("STRLEN"))->invoke("abc");
echo ":";
echo (new ReflectionFunction("strlen"))->invoke(string: "abcd");
echo ":";
$ref = new ReflectionFunction("strlen");
echo $ref->invokeArgs(["abcde"]);
echo ":";
echo $ref->invokeArgs(args: ["string" => "abcdef"]);
"#,
    );
    assert_eq!(out, "3:4:5:6");
}

/// Verifies non-closure `ReflectionFunction` objects report no used variables.
#[test]
fn test_reflection_function_reports_empty_closure_used_variables() {
    let out = compile_and_run(
        r#"<?php
function reflect_function_closure_vars_plain(): void {}

$user = new ReflectionFunction("reflect_function_closure_vars_plain");
$builtin = new ReflectionFunction("strlen");
echo count($user->getClosureUsedVariables()) . ":";
echo count($builtin->getClosureUsedVariables()) . ":";
$vars = $user->getClosureUsedVariables();
$vars["x"] = "changed";
echo count($user->getClosureUsedVariables());
"#,
    );
    assert_eq!(out, "0:0:0");
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

/// Verifies `ReflectionFunction::invoke()` supports inferred AOT signatures.
#[test]
fn test_reflection_function_invoke_calls_inferred_aot_signature() {
    let out = compile_and_run(
        r#"<?php
function reflect_function_invoke_inferred($left, $right) {
    return $left . $right;
}

echo (new ReflectionFunction("reflect_function_invoke_inferred"))->invoke("A", "B");
"#,
    );
    assert_eq!(out, "AB");
}

/// Verifies `returnsReference()` reports the declaration instead of a baked `false`.
///
/// It was registered with `builtin_reflection_constant_false_bool_method`, so it answered `false`
/// for every callable — including `function &f()`, where PHP answers `true` (#1231). Every
/// sibling predicate that can vary is property-backed; this one now is too, reading
/// `__returns_reference` off the same `FunctionSig::by_ref_return` the member is built from.
///
/// No static method is among the rows: a by-reference return of a `static` local is refused by
/// the lowering ("this compiler can transfer only a local it can promote to a managed reference
/// cell in place"), so such a declaration cannot be compiled to reflect on.
#[test]
fn test_returns_reference_reports_the_declaration() {
    let out = compile_and_run(
        r#"<?php
class Box { public $v = 1; }

function &getv(Box $o) { return $o->v; }
function plain(Box $o) { return $o->v; }

class RefHolder {
    public $w = 2;
    public function &get() { return $this->w; }
    public function plain() { return $this->w; }
}

echo (new ReflectionFunction('getv'))->returnsReference() ? "y" : "n";
echo (new ReflectionFunction('plain'))->returnsReference() ? "y" : "n";
echo (new ReflectionMethod('RefHolder', 'get'))->returnsReference() ? "y" : "n";
echo (new ReflectionMethod('RefHolder', 'plain'))->returnsReference() ? "y" : "n";
"#,
    );

    assert_eq!(out, "ynyn");
}

/// Verifies the listing path agrees with the constructed one.
///
/// `getMethods()` builds its entries through a different path than `new ReflectionMethod(...)`,
/// and a flag threaded through only one of them is how the two come to disagree about the same
/// declaration — which is the shape this issue is an instance of.
#[test]
fn test_listed_and_constructed_returns_reference_agree() {
    let out = compile_and_run(
        r#"<?php
class RefHolder {
    public $w = 2;
    public function &get() { return $this->w; }
    public function plain() { return $this->w; }
}

$listed = [];
foreach ((new ReflectionClass('RefHolder'))->getMethods() as $m) {
    $listed[$m->getName()] = $m->returnsReference();
}
echo $listed['get'] ? "y" : "n";
echo $listed['plain'] ? "y" : "n";
echo (new ReflectionMethod('RefHolder', 'get'))->returnsReference() === $listed['get'] ? "same" : "differs";
"#,
    );

    assert_eq!(out, "ynsame");
}

/// Verifies an interface's `function &m()` declaration reports through every path.
///
/// Interface methods are built by their own constructor, which wrote a baked `false` into both
/// records it makes even though its signature carries `by_ref_return`: the listed method, which
/// `getMethods()` and `new ReflectionMethod(...)` both read, and the declaring-function record a
/// parameter's `getDeclaringFunction()` reads. Entries are keyed by name because the listing
/// order of an interface's `getMethods()` is not what this asserts.
#[test]
fn test_interface_method_returns_reference_reports_the_declaration() {
    let out = compile_and_run(
        r#"<?php
interface RefContract {
    public function &byRef(int $n): array;
    public function byVal(): array;
}

$listed = [];
foreach ((new ReflectionClass('RefContract'))->getMethods() as $m) {
    $listed[$m->getName()] = $m->returnsReference();
}
echo $listed['byRef'] ? "y" : "n";
echo $listed['byVal'] ? "y" : "n";
$byRef = new ReflectionMethod('RefContract', 'byRef');
echo $byRef->returnsReference() ? "y" : "n";
echo (new ReflectionMethod('RefContract', 'byVal'))->returnsReference() ? "y" : "n";
$params = $byRef->getParameters();
$declaring = $params[0]->getDeclaringFunction();
echo $declaring->returnsReference() ? "y" : "n";
"#,
    );

    assert_eq!(out, "ynyny");
}

/// Verifies a Reflection object codegen builds INSIDE another one can be stringified.
///
/// `ReflectionParameter::getDeclaringFunction()` hands back a `ReflectionFunction` that codegen
/// allocates while it builds the parameter; nothing in the program ever names the class. Its
/// methods were never lowered, its vtable held null, and every implicit `__toString` on it jumped
/// to address zero (#1229); `sprintf("%s")` read the class metadata instead and reported the
/// object as not convertible. The program deliberately never constructs a `ReflectionFunction`
/// nor calls a method on `$f`: either would pull the class in and hide the defect. It asserts
/// the type and class of each result, not the rendered text, which is PR #1120's subject.
#[test]
fn test_declaring_function_object_stringifies_without_naming_its_class() {
    let out = compile_and_run(
        r#"<?php
$p = new ReflectionParameter("strlen", "string");
$f = $p->getDeclaringFunction();
echo get_class($f), "|";
echo gettype((string) $f), "|";
echo gettype("" . $f), "|";
echo gettype("{$f}"), "|";
echo gettype(strval($f)), "|";
echo strlen($f) >= 0 ? "len" : "bad", "|";
echo gettype(sprintf("%s", $f));
"#,
    );

    assert_eq!(out, "ReflectionFunction|string|string|string|string|len|string");
}

/// Verifies the declaring METHOD object of a parameter can be stringified the same way.
///
/// The declaring function of a method's parameter is a `ReflectionMethod`, the second class the
/// same codegen slot can hold (#1229).
#[test]
fn test_declaring_method_object_stringifies_without_naming_its_class() {
    let out = compile_and_run(
        r#"<?php
class DeclaringMethodHost { public function run(int $a) {} }
$p = new ReflectionParameter(["DeclaringMethodHost", "run"], 0);
$f = $p->getDeclaringFunction();
echo get_class($f), "|", gettype((string) $f);
"#,
    );

    assert_eq!(out, "ReflectionMethod|string");
}

/// Verifies a parameter's declaring CLASS object can be stringified.
///
/// `ReflectionParameter::getDeclaringClass()` is typed `mixed` like the declaring function, and
/// codegen builds its `ReflectionClass` the same way (#1229).
#[test]
fn test_parameter_declaring_class_object_stringifies_without_naming_its_class() {
    let out = compile_and_run(
        r#"<?php
class DeclaringClassHost { public function run(int $a) {} }
$p = new ReflectionParameter(["DeclaringClassHost", "run"], 0);
$c = $p->getDeclaringClass();
echo get_class($c), "|", gettype((string) $c);
"#,
    );

    assert_eq!(out, "ReflectionClass|string");
}

/// Verifies an enum case's enum object can be stringified.
///
/// `ReflectionEnumUnitCase::getEnum()` returns a `ReflectionEnum` codegen builds from the case's
/// `__enum` slot, the last materializer of the #1229 family.
#[test]
fn test_enum_case_enum_object_stringifies_without_naming_its_class() {
    let out = compile_and_run(
        r#"<?php
enum MaterializedEnumHost { case A; }
$case = new ReflectionEnumUnitCase(MaterializedEnumHost::class, "A");
$e = $case->getEnum();
echo get_class($e), "|", gettype((string) $e);
"#,
    );

    assert_eq!(out, "ReflectionEnum|string");
}

/// Verifies the declaring function reached through `call_user_func()` can be stringified.
///
/// The call lowers to a callable descriptor invoke, not a `MethodCall`, so a rule that only
/// watched direct calls missed it and the #1229 crash survived on this route.
#[test]
fn test_declaring_function_through_call_user_func_stringifies() {
    let out = compile_and_run(
        r#"<?php
$p = new ReflectionParameter("strlen", "string");
$f = call_user_func([$p, "getDeclaringFunction"]);
echo get_class($f), "|", gettype((string) $f);
"#,
    );

    assert_eq!(out, "ReflectionFunction|string");
}

/// Verifies the declaring function reached through a first-class callable can be stringified.
///
/// A first-class callable keeps its target as `object::getdeclaringfunction`, so the getter's
/// name is only visible behind that qualifier (#1229).
#[test]
fn test_declaring_function_through_first_class_callable_stringifies() {
    let out = compile_and_run(
        r#"<?php
$p = new ReflectionParameter("strlen", "string");
$getter = $p->getDeclaringFunction(...);
$f = $getter();
echo get_class($f), "|", gettype((string) $f);
"#,
    );

    assert_eq!(out, "ReflectionFunction|string");
}

/// Verifies a getter name formed from literal fragments still lowers its companion object.
///
/// The complete getter does not appear in string data: the method name is assembled by runtime
/// concatenation, so reachability must combine literal fragments before giving up (#1252).
#[test]
fn test_declaring_function_through_concatenated_dynamic_method_name_stringifies() {
    let out = compile_and_run(
        r#"<?php
$p = new ReflectionParameter("strlen", "string");
$getter = "get" . "DeclaringFunction";
$f = $p->$getter();
echo get_class($f), "|", gettype((string) $f);
"#,
    );

    assert_eq!(out, "ReflectionFunction|string");
}

/// Verifies the legacy parameter class getter lowers its materialized result without naming it.
#[test]
fn test_reflection_parameter_class_getter_object_lowers_without_explicit_type_name() {
    let out = compile_and_run(
        r#"<?php
class GetterClassDependency {}
function getterClassSurface(GetterClassDependency $value): void {}
$parameter = new ReflectionParameter("getterClassSurface", "value");
$class = $parameter->getClass();
echo get_class($class), ":", gettype((string) $class);
"#,
    );

    assert_eq!(out, "ReflectionClass:string");
}

/// Verifies type objects returned by Reflection getters need no explicit type-name references.
///
/// These slots contain freshly materialized `ReflectionType` objects, but the PHP program never
/// names their concrete synthetic classes or narrows by `instanceof` (#1252).
#[test]
fn test_reflection_type_getter_objects_lower_without_explicit_type_names() {
    let out = compile_and_run(
        r#"<?php
interface GetterTypeA {}
interface GetterTypeB {}
class GetterTypeBoth implements GetterTypeA, GetterTypeB {}
function getterTypeSurface(int|string $value, GetterTypeA&GetterTypeB $both): int|string { return $value; }
class GetterTypeProperty { public int|string $value; }
$parameterUnion = (new ReflectionParameter("getterTypeSurface", "value"))->getType();
$parameterIntersection = (new ReflectionParameter("getterTypeSurface", "both"))->getType();
$returnType = (new ReflectionFunction("getterTypeSurface"))->getReturnType();
$settableType = (new ReflectionProperty(GetterTypeProperty::class, "value"))->getSettableType();
foreach ([$parameterUnion, $parameterIntersection, $returnType, $settableType] as $type) {
    echo get_class($type), ":", gettype((string) $type), "|";
}
"#,
    );

    assert_eq!(
        out,
        "ReflectionUnionType:string|ReflectionIntersectionType:string|ReflectionUnionType:string|ReflectionUnionType:string|"
    );
}

/// Verifies the declaring function reached through a method name held in a variable can be
/// stringified — the third route that is not a literal `MethodCall` (#1229).
#[test]
fn test_declaring_function_through_dynamic_method_name_stringifies() {
    let out = compile_and_run(
        r#"<?php
$p = new ReflectionParameter("strlen", "string");
$getter = "getDeclaringFunction";
$f = $p->$getter();
echo get_class($f), "|", gettype((string) $f);
"#,
    );

    assert_eq!(out, "ReflectionFunction|string");
}
