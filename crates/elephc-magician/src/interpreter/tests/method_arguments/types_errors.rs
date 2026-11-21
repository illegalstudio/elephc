//! Purpose:
//! Interpreter tests for duplicate/unknown arguments, omission rules, and scalar,
//! object, or variadic method type hints.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Binding diagnostics and runtime coercion checks use separate cases.

use super::super::super::*;
use super::super::support::*;

/// Verifies eval-declared variadic methods reject duplicate named variadic keys.
#[test]
fn execute_program_rejects_duplicate_eval_method_variadic_named_arg() {
    let program = parse_fragment(
        br#"class EvalDuplicateVariadicBox {
    public function read(...$tail) {
        return count($tail);
    }
}
$box = new EvalDuplicateVariadicBox();
return $box->read(name: "A", name: "B");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("duplicate named variadic argument should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies defaults before required eval method parameters raise a catchable argument error.
#[test]
fn execute_program_rejects_eval_method_default_before_required_omission() {
    let source = r#"class EvalRequiredAfterDefaultBox {
    public function read($left = "A", $right) {
        return $left . $right;
    }
}
$box = new EvalRequiredAfterDefaultBox();
try {
    $box->read(right: "B");
} catch (__CATCH_TYPE__ $error) {
    return get_class($error);
}
return "missed";"#;
    for catch_type in ["Throwable", "Error", "TypeError"] {
        let program = parse_fragment(source.replace("__CATCH_TYPE__", catch_type).as_bytes())
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values)
            .expect("catch default before required parameter omission");

        assert_eq!(
            values.get(result),
            FakeValue::String("ArgumentCountError".to_string()),
            "catch {catch_type}",
        );
    }
}

/// Verifies eval-declared method scalar type hints coerce weak scalar arguments.
#[test]
fn execute_program_enforces_eval_method_scalar_type_hints() {
    let program = parse_fragment(
        br#"class EvalTypedScalarBox {
    public function read(int $id, string $label, bool $flag) {
        echo $id + 1; echo ":";
        echo $label; echo ":";
        return $flag ? "T" : "F";
    }
}
$box = new EvalTypedScalarBox();
return $box->read("7", 8, 1);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "8:8:");
    assert_eq!(values.get(result), FakeValue::String("T".to_string()));
}

/// Verifies eval-declared method scalar type hints reject non-coercible values.
#[test]
fn execute_program_rejects_eval_method_scalar_type_mismatch() {
    let program = parse_fragment(
        br#"class EvalTypedScalarFailBox {
    public function read(int $id) {
        return $id;
    }
}
$box = new EvalTypedScalarFailBox();
try { $box->read("not numeric"); } catch (TypeError $error) { return get_class($error); } return "missed";"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values)
        .expect("non-numeric string raises a catchable parameter TypeError");

    assert_eq!(values.get(result), FakeValue::String("TypeError".to_string()));
}

/// Verifies eval-declared method class/interface type hints accept matching eval objects.
#[test]
fn execute_program_enforces_eval_method_object_type_hints() {
    let program = parse_fragment(
        br#"interface EvalTypedReadable {}
class EvalTypedDep implements EvalTypedReadable {}
class EvalTypedObjectBox {
    public function read(EvalTypedReadable $dep, ?EvalTypedDep $nullable) {
        echo get_class($dep); echo ":";
        return $nullable === null ? "N" : "bad";
    }
}
$dep = new EvalTypedDep();
$box = new EvalTypedObjectBox();
return $box->read($dep, null);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "EvalTypedDep:");
    assert_eq!(values.get(result), FakeValue::String("N".to_string()));
}

/// Verifies eval-declared variadic method type hints apply to each captured argument.
#[test]
fn execute_program_enforces_eval_method_variadic_type_hints() {
    let program = parse_fragment(
        br#"class EvalTypedVariadicBox {
    public function sum(int ...$items) {
        return $items[0] + $items[1];
    }
}
$box = new EvalTypedVariadicBox();
return $box->sum("3", 4);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(7));
}

/// Verifies eval-declared methods reject unknown named arguments.
#[test]
fn execute_program_rejects_unknown_eval_method_named_arg() {
    let program = parse_fragment(
        br#"class EvalUnknownNamedMethodBox {
    public function read($left) {
        return $left;
    }
}
$box = new EvalUnknownNamedMethodBox();
return $box->read(missing: "bad");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("unknown named method argument should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies a fragment's `declare(strict_types=1)` rejects weak scalar coercion for ordinary
/// method and function calls instead of silently applying the coercive path.
#[test]
fn execute_program_strict_eval_rejects_weak_scalar_arguments() {
    let method = parse_fragment(
        br#"declare(strict_types=1);
class StrictTypedScalarBox {
    public function read(int $id) {
        return $id;
    }
}
try {
    (new StrictTypedScalarBox())->read("7");
} catch (TypeError $error) {
    return $error->getMessage();
}
return "missed";"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&method, &mut scope, &mut values).expect("catch strict method use");
    assert_eq!(
        values.get(result),
        FakeValue::String(
            "StrictTypedScalarBox::read(): Argument #1 ($id) must be of type int, string given"
                .to_string()
        )
    );

    let function = parse_fragment(
        br#"declare(strict_types=1);
function strictScalarReader(int $id) {
    return $id;
}
try {
    strictScalarReader("7");
} catch (TypeError $error) {
    return $error->getMessage();
}
return "missed";"#,
    )
    .expect("parse eval fragment");
    let result = execute_program(&function, &mut scope, &mut values)
        .expect("catch strict function use");
    assert_eq!(
        values.get(result),
        FakeValue::String(
            "strictScalarReader(): Argument #1 ($id) must be of type int, string given"
                .to_string()
        )
    );

    let variadic = parse_fragment(
        br#"declare(strict_types=1);
function strictVariadicReader(int ...$items) {
    return $items[0];
}
try {
    strictVariadicReader(1, "2");
} catch (TypeError $error) {
    return $error->getMessage();
}
return "missed";"#,
    )
    .expect("parse eval fragment");
    let result = execute_program(&variadic, &mut scope, &mut values)
        .expect("catch strict variadic use");
    assert_eq!(
        values.get(result),
        FakeValue::String(
            "strictVariadicReader(): Argument #2 must be of type int, string given".to_string()
        )
    );
}

/// Verifies strict eval keeps PHP's single weak conversion: an `int` argument widens into a
/// declared `float` parameter.
#[test]
fn execute_program_strict_eval_allows_int_to_float_widening() {
    let program = parse_fragment(
        br#"declare(strict_types=1);
function strictFloatReader(float $value) {
    return $value;
}
return strictFloatReader(7);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Float(7.0));
}

/// Verifies a strict fragment rejects weak coercion of a returned value into the declared
/// return type.
#[test]
fn execute_program_strict_eval_rejects_weak_returned_scalars() {
    let program = parse_fragment(
        br#"declare(strict_types=1);
function strictIntReturnReader(): int {
    return "7";
}
try {
    strictIntReturnReader();
} catch (TypeError $error) {
    return $error->getMessage();
}
return "missed";"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("catch strict return");

    assert_eq!(
        values.get(result),
        FakeValue::String(
            "strictIntReturnReader(): Return value must be of type int, string returned"
                .to_string()
        )
    );
}

/// Verifies a saved eval function and closure keep the strict mode of the fragment that
/// declared them when a strict or coercive fragment calls them.
#[test]
fn execute_program_saved_functions_keep_their_defining_strict_mode() {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let weak = parse_fragment(
        br#"function weakScalarReader(int $id) {
    return $id;
}
function weakOuter() {
    return weakScalarReader("7");
}
$weakClosure = function () {
    $nested = function (int $id) { return $id; };
    return $nested("8");
};"#,
    )
    .expect("parse weak eval fragment");
    let result = execute_program_with_context(&mut context, &weak, &mut scope, &mut values)
        .expect("install weak eval declarations");
    values.release(result).expect("release installed value");

    let strict = parse_fragment(
        br#"declare(strict_types=1);
$outer = weakOuter();
$inner = $weakClosure();
return $outer + $inner;"#,
    )
    .expect("parse strict eval fragment");
    let result = execute_program_with_context(&mut context, &strict, &mut scope, &mut values)
        .expect("weak bodies must remain coercive under a strict caller");
    assert_eq!(values.get(result), FakeValue::Int(15));

    let strict_declaration = parse_fragment(
        br#"declare(strict_types=1);
function strictScalarReader(int $id) {
    return $id;
}
function strictOuter() {
    return strictScalarReader("9");
}"#,
    )
    .expect("parse strict eval declaration");
    let result = execute_program_with_context(
        &mut context,
        &strict_declaration,
        &mut scope,
        &mut values,
    )
    .expect("install strict eval declarations");
    values.release(result).expect("release installed value");

    let coercive = parse_fragment(
        br#"try {
    strictOuter();
} catch (TypeError $error) {
    return $error->getMessage();
}
return "missed";"#,
    )
    .expect("parse coercive eval fragment");
    let result = execute_program_with_context(&mut context, &coercive, &mut scope, &mut values)
        .expect("a strict-defined body keeps its mode under a coercive caller");
    assert_eq!(
        values.get(result),
        FakeValue::String(
            "strictScalarReader(): Argument #1 ($id) must be of type int, string given"
                .to_string()
        )
    );
}

/// Verifies the eval parser accepts exactly `0` or `1` as the `strict_types` value.
#[test]
fn parse_fragment_rejects_invalid_strict_types_values() {
    for accepted in ["declare(strict_types=0);", "declare(strict_types=1);"] {
        parse_fragment(accepted.as_bytes()).expect("valid strict_types value");
    }
    for rejected in [
        "declare(strict_types=2);",
        "declare(strict_types=-1);",
        "declare(strict_types=1.0);",
    ] {
        assert!(
            parse_fragment(rejected.as_bytes()).is_err(),
            "invalid strict_types fragment parsed: {rejected}"
        );
    }
    for displaced in [
        "echo 1; declare(strict_types=1);",
        "$value = 1; declare(strict_types=0);",
    ] {
        assert!(
            parse_fragment(displaced.as_bytes()).is_err(),
            "non-leading strict_types fragment parsed: {displaced}"
        );
    }
}

/// Verifies `call_user_func()` forwards the caller's strict mode instead of using the
/// engine-frame coercive mode that `array_map()` and other callback builtins use.
#[test]
fn execute_program_strict_eval_call_user_func_forwards_strict_mode() {
    let program = parse_fragment(
        br#"declare(strict_types=1);
function strictCallUserFuncReader(int $value) {
    return $value;
}
try {
    call_user_func("strictCallUserFuncReader", "7");
} catch (TypeError $error) {
    return $error->getMessage();
}
return "missed";"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("catch strict call_user_func");

    assert_eq!(
        values.get(result),
        FakeValue::String(
            "strictCallUserFuncReader(): Argument #1 ($value) must be of type int, string given"
                .to_string()
        )
    );
}
