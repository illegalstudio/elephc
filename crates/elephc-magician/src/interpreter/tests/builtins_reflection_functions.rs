//! Purpose:
//! Interpreter tests for eval-backed ReflectionFunction objects.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Free eval functions retain function and parameter metadata used by
//!   ReflectionFunction and ReflectionParameter.

use super::super::*;
use super::support::*;

/// Verifies eval-declared functions materialize ReflectionFunction parameter metadata.
#[test]
fn execute_program_reflects_eval_declared_function_parameters() {
    let program = parse_fragment(
        br#"function eval_reflect_free($left, $right) { return $left; }
$ref = new ReflectionFunction("eval_reflect_free");
$params = $ref->getParameters();
echo $ref->getName(); echo ":";
echo $ref->getNumberOfParameters(); echo ":";
echo $ref->getNumberOfRequiredParameters(); echo ":";
echo count($params); echo ":";
echo $params[0]->getName(); echo ":";
echo $params[1]->getPosition(); echo ":";
$declaring = $params[0]->getDeclaringFunction();
echo get_class($declaring); echo ":";
echo $declaring->getName();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "eval_reflect_free:2:2:2:left:1:ReflectionFunction:eval_reflect_free"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval-declared function metadata includes attributes, types, defaults, and flags.
#[test]
fn execute_program_reflects_eval_function_signature_metadata() {
    let program = parse_fragment(
        br#"class EvalFuncAttr {
    public $label;
    public function __construct($label) { $this->label = $label; }
    public function label() { return $this->label; }
}
#[EvalFuncAttr("free")]
function eval_reflect_rich(#[EvalFuncAttr("first")] string $name, int $count = 3, &...$items) {
    return $count;
}
$ref = new ReflectionFunction("eval_reflect_rich");
$attrs = $ref->getAttributes();
$params = $ref->getParameters();
echo count($attrs); echo ":";
echo $attrs[0]->getName(); echo ":";
echo $attrs[0]->newInstance()->label(); echo ":";
echo $ref->getNumberOfParameters(); echo ":";
echo $ref->getNumberOfRequiredParameters(); echo ":";
echo $params[0]->hasType() ? "T" : "t"; echo ":";
echo $params[0]->getType()->getName(); echo ":";
$paramAttrs = $params[0]->getAttributes();
echo count($paramAttrs); echo ":";
echo $paramAttrs[0]->newInstance()->label(); echo ":";
echo $params[1]->isOptional() ? "O" : "o"; echo ":";
echo $params[1]->getDefaultValue(); echo ":";
echo $params[2]->isVariadic() ? "V" : "v"; echo ":";
echo $params[2]->isPassedByReference() ? "R" : "r";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "1:EvalFuncAttr:free:3:1:T:string:1:first:O:3:V:R"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionFunction exposes eval-declared return type metadata.
#[test]
fn execute_program_reflection_function_reports_return_type_metadata() {
    let program = parse_fragment(
        br#"function eval_reflect_return_named(): ?int { return 1; }
function eval_reflect_return_union(): int|string { return 1; }
function eval_reflect_return_never(): never { throw new Exception("stop"); }
function eval_reflect_return_plain() {}
$namedRef = new ReflectionFunction("eval_reflect_return_named");
$named = $namedRef->getReturnType();
echo $namedRef->hasReturnType() ? "T" : "t"; echo ":";
echo $named->getName(); echo ":";
echo $named->allowsNull() ? "N" : "n"; echo ":";
echo $named->isBuiltin() ? "B" : "b"; echo ":";
$union = (new ReflectionFunction("eval_reflect_return_union"))->getReturnType();
echo count($union->getTypes()); echo ":";
foreach ($union->getTypes() as $type) {
    echo $type->getName();
    echo $type->isBuiltin() ? "B" : "b";
}
echo ":";
$never = (new ReflectionFunction("eval_reflect_return_never"))->getReturnType();
echo $never->getName(); echo ":";
echo $never->allowsNull() ? "N" : "n"; echo ":";
echo $never->isBuiltin() ? "B" : "b"; echo ":";
$plain = new ReflectionFunction("eval_reflect_return_plain");
echo $plain->hasReturnType() ? "P" : "p"; echo ":";
echo $plain->getReturnType() === null ? "Q" : "q";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "T:int:N:B:2:intBstringB:never:n:B:p:Q");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionFunction formats retained eval function metadata through `__toString()`.
#[test]
fn execute_program_reflection_function_to_string() {
    let program = parse_fragment(
        br#"function eval_reflect_string(string $name, int $count = 3, &...$items): ?string {
    return $name;
}
$ref = new ReflectionFunction("eval_reflect_string");
echo str_replace("\n", "|", $ref->__toString());
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Function [ <user> function eval_reflect_string ] {|  - Parameters [3] {|    Parameter #0 [ <required> string $name ]|    Parameter #1 [ <optional> int $count = 3 ]|    Parameter #2 [ <optional> &...$items ]|  }|  - Return [ ?string ]|}|"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionFunction origin metadata APIs report eval user-defined defaults.
#[test]
fn execute_program_reflection_function_reports_origin_metadata_defaults() {
    let program = parse_fragment(
        br#"function eval_reflect_origin_defaults() {}
$ref = new ReflectionFunction("eval_reflect_origin_defaults");
echo ($ref->getDocComment() === false) ? "D" : "d"; echo ":";
echo ($ref->getExtensionName() === false) ? "E" : "e"; echo ":";
echo ($ref->getExtension() === null) ? "X" : "x";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "D:E:X");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionFunction derives `isDeprecated()` from eval-retained attributes.
#[test]
fn execute_program_reflection_function_reports_deprecated_attribute() {
    let program = parse_fragment(
        br#"#[Deprecated]
function eval_reflect_deprecated_function() {}
function eval_reflect_plain_function() {}
$deprecated = new ReflectionFunction("eval_reflect_deprecated_function");
$plain = new ReflectionFunction("eval_reflect_plain_function");
echo $deprecated->isDeprecated() ? "D" : "d"; echo ":";
echo $plain->isDeprecated() ? "D" : "d";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "D:d");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionFunction exposes PHP-compatible name and origin predicate metadata.
#[test]
fn execute_program_reflection_function_reports_name_and_origin_predicates() {
    let program = parse_fragment(
        br#"namespace EvalReflectFnNs;
function sample(...$items) {}
$ref = new \ReflectionFunction('EvalReflectFnNs\\sample');
echo $ref->getShortName(); echo ":";
echo $ref->getNamespaceName(); echo ":";
echo $ref->inNamespace() ? "Y" : "N"; echo ":";
echo $ref->isInternal() ? "I" : "i";
echo $ref->isUserDefined() ? "U" : "u"; echo ":";
echo $ref->isAnonymous() ? "A" : "a"; echo ":";
echo $ref->isClosure() ? "C" : "c"; echo ":";
echo $ref->isDeprecated() ? "D" : "d"; echo ":";
echo $ref->isStatic() ? "S" : "s"; echo ":";
echo $ref->returnsReference() ? "R" : "r"; echo ":";
echo $ref->hasReturnType() ? "T" : "t"; echo ":";
echo $ref->getReturnType() === null ? "N" : "n"; echo ":";
echo $ref->isGenerator() ? "G" : "g"; echo ":";
echo $ref->isVariadic() ? "V" : "v"; echo ":";
echo $ref->hasTentativeReturnType() ? "H" : "h"; echo ":";
echo $ref->getTentativeReturnType() === null ? "Q" : "q"; echo ":";
echo $ref->getClosureThis() === null ? "T" : "t"; echo ":";
echo $ref->getClosureScopeClass() === null ? "S" : "s"; echo ":";
echo $ref->getClosureCalledClass() === null ? "L" : "l"; echo ":";
echo $ref->isDisabled() ? "X" : "x";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "sample:EvalReflectFnNs:Y:iU:a:c:d:s:r:t:N:g:V:h:Q:T:S:L:x"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionFunction recognizes eval closure literals and dispatches them.
#[test]
fn execute_program_reflection_function_supports_eval_closure_literals() {
    let program = parse_fragment(
        br#"$seed = 4;
$fn = function($delta = 1) use ($seed) { return $seed + $delta; };
$ref = new ReflectionFunction($fn);
echo $ref->isClosure() ? "C" : "c"; echo ":";
echo $ref->isAnonymous() ? "A" : "a"; echo ":";
echo $ref->isUserDefined() ? "U" : "u"; echo ":";
echo $ref->getNumberOfParameters(); echo ":";
echo $ref->getNumberOfRequiredParameters(); echo ":";
$vars = $ref->getClosureUsedVariables();
echo count($vars); echo ":";
echo $vars["seed"]; echo ":";
echo $ref->invoke(3); echo ":";
echo $ref->invokeArgs(["delta" => 5]);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "C:A:U:1:0:1:4:7:9");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionFunction reports eval source file and line metadata.
#[test]
fn execute_program_reflection_function_reports_source_location() {
    let program = parse_fragment(
        br#"function eval_reflect_source_fn() {
    return 1;
}
$ref = new ReflectionFunction("eval_reflect_source_fn");
echo $ref->getFileName(); echo ":";
echo $ref->getStartLine(); echo ":";
echo $ref->getEndLine();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    context.set_call_site("/tmp/eval-source.php", "/tmp", 17);
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(values.output, "/tmp/eval-source.php(17) : eval()'d code:1:3");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionFunction exposes eval static locals before and after execution.
#[test]
fn execute_program_reflection_function_reports_static_variables() {
    let program = parse_fragment(
        br#"function eval_reflect_static_vars() {
    static $count = 1;
    static $label = "fn";
    $count = $count + 1;
    return $count;
}
$ref = new ReflectionFunction("eval_reflect_static_vars");
$before = $ref->getStaticVariables();
echo $before["count"]; echo ":"; echo $before["label"]; echo ":";
echo eval_reflect_static_vars(); echo ":";
$after = $ref->getStaticVariables();
echo $after["count"]; echo ":"; echo $after["label"];
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "1:fn:2:2:fn");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval-declared functions bind named, default, by-reference, and variadic arguments.
#[test]
fn execute_program_calls_eval_function_with_rich_argument_binding() {
    let program = parse_fragment(
        br#"function eval_signature_call(string $name, &$value, int $count = 2, ...$rest) {
    $value = $value + $count;
    echo $name; echo ":";
    echo $count; echo ":";
    echo count($rest); echo ":";
}
function eval_signature_array(string $name, int $count = 2, ...$rest) {
    echo $name; echo ":";
    echo $count; echo ":";
    echo count($rest); echo ":";
    echo $rest["extra"];
}
$seed = 4;
eval_signature_call(name: "ok", value: $seed, extra: "z");
echo $seed; echo ":";
call_user_func_array("eval_signature_array", ["extra" => "z", "name" => "cb"]);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "ok:2:1:6:cb:2:1:z");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionFunction invocation dispatches eval functions with forwarded arguments.
#[test]
fn execute_program_reflection_function_invokes_eval_function() {
    let program = parse_fragment(
        br#"function eval_reflect_invoke($left = "A", $right = "B", ...$rest) {
    return $left . $right . count($rest) . $rest["extra"];
}
function eval_reflect_no_writeback(&$value) {
    $value = $value . "!";
    return $value;
}
$ref = new ReflectionFunction("eval_reflect_invoke");
echo $ref->invoke(right: "2", left: "1", extra: "X"); echo ":";
echo $ref->invokeArgs(["extra" => "Y", "left" => "3", "right" => "4"]); echo ":";
$value = "Q";
$mutate = new ReflectionFunction("eval_reflect_no_writeback");
echo $mutate->invoke($value); echo ":"; echo $value;
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "121X:341Y:Q!:Q");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
