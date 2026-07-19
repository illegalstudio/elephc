//! Purpose:
//! Interpreter tests for variable calls, `call_user_func`, callable arrays, and
//! temporary release on validation failures.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases verify callable dispatch across eval functions, builtins, native functions, and object methods.

use super::super::super::*;
use super::super::support::*;

/// Verifies `call_user_func` inside eval can dispatch an eval-declared function.
#[test]
fn execute_program_call_user_func_dispatches_declared_function() {
    let program = parse_fragment(
        br#"function dyn($x) { return $x + 1; }
return call_user_func("dyn", 4);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(5));
}
/// Verifies `call_user_func` inside eval can dispatch a supported builtin.
#[test]
fn execute_program_call_user_func_dispatches_builtin() {
    let program = parse_fragment(br#"return call_user_func("strlen", "abcd");"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(4));
}
/// Verifies `call_user_func` releases literal callback temporaries after dispatch.
#[test]
fn execute_program_call_user_func_releases_literal_callback_after_dispatch() {
    let program = parse_fragment(br#"return call_user_func("strlen", "abcd");"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(4));
    assert!(
        values
            .releases
            .iter()
            .any(|release| values.get(*release) == FakeValue::String("strlen".to_string())),
        "literal callback string should be released after dispatch"
    );
}

/// Verifies `call_user_func` releases literal callback temporaries after dispatch fatal.
#[test]
fn execute_program_call_user_func_releases_literal_callback_after_dispatch_fatal() {
    let program =
        parse_fragment(br#"return call_user_func("strlen");"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values);

    assert_eq!(result, Err(EvalStatus::RuntimeFatal));
    assert!(
        values
            .releases
            .iter()
            .any(|release| values.get(*release) == FakeValue::String("strlen".to_string())),
        "literal callback string should be released after fatal dispatch"
    );
}

/// Verifies `call_user_func` releases literal callback when later arg evaluation fails.
#[test]
fn execute_program_call_user_func_releases_literal_callback_after_arg_eval_fatal() {
    let program = parse_fragment(br#"return call_user_func("strlen", MISSING_ARG);"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values);

    assert_eq!(result, Err(EvalStatus::RuntimeFatal));
    assert!(
        values
            .releases
            .iter()
            .any(|release| values.get(*release) == FakeValue::String("strlen".to_string())),
        "literal callback string should be released when argument evaluation fails"
    );
}

/// Verifies `call_user_func` inside eval can dispatch a registered native function.
#[test]
fn execute_program_call_user_func_dispatches_registered_native_function() {
    let program =
        parse_fragment(br#"return call_user_func("native_answer");"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let expected = values.int(42).expect("allocate fake result");
    let native = NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 0);
    assert!(context
        .define_native_function("native_answer", native)
        .is_ok());

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(result, expected);
}
/// Verifies string variable calls inside eval can dispatch a supported builtin.
#[test]
fn execute_program_variable_call_dispatches_builtin() {
    let program = parse_fragment(
        br#"$fn = "strlen";
return $fn("abcd");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(4));
}
/// Verifies callable array entries can be invoked through postfix dynamic calls.
#[test]
fn execute_program_postfix_variable_call_dispatches_builtin() {
    let program = parse_fragment(
        br#"$callbacks = ["strlen"];
return $callbacks[0]("abc");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(3));
}
/// Verifies variable calls bind eval-declared function arguments by name.
#[test]
fn execute_program_variable_call_binds_declared_named_args() {
    let program = parse_fragment(
        br#"function dyn($x, $y) { return ($x * 10) + $y; }
$fn = "dyn";
return $fn(y: 2, x: 1);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(12));
}
/// Verifies variable calls can dispatch registered native functions with named args.
#[test]
fn execute_program_variable_call_binds_registered_native_named_args() {
    let program = parse_fragment(
        br#"$fn = "native_answer";
return $fn(right: 2, left: 1);"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let expected = values.int(42).expect("allocate fake result");
    let mut native =
        NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 2);
    assert!(native.set_param_name(0, "left"));
    assert!(native.set_param_name(1, "right"));
    assert!(context
        .define_native_function("native_answer", native)
        .is_ok());

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(result, expected);
}
/// Verifies direct callable-array variable calls dispatch object methods.
#[test]
fn execute_program_callable_array_variable_dispatches_object_method() {
    let program = parse_fragment(
        br#"$box = new Box(41);
$cb = [$box, "add_x"];
return $cb(1);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(42));
    assert_released_array_callable_index_temps(&values);
}
/// Verifies `call_user_func` dispatches callable arrays with object receivers.
#[test]
fn execute_program_call_user_func_dispatches_object_method_array() {
    let program = parse_fragment(
        br#"$box = new Box(42);
$cb = [$box, "read_x"];
return call_user_func($cb);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(42));
    assert_released_array_callable_index_temps(&values);
}

/// Verifies callable-array index temporaries are released when validation rejects the callback.
#[test]
fn execute_program_call_user_func_releases_array_callable_index_temps_after_validation_fatal() {
    let program = parse_fragment(
        br#"class EvalMissingArrayCallableMethod {}
$missing = new EvalMissingArrayCallableMethod();
try {
    call_user_func([$missing, "MiSsInG"]);
    return false;
} catch (TypeError $e) {
    return true;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Bool(true));
    assert_released_array_callable_index_temps(&values);
}
/// Verifies `call_user_func_array` dispatches callable arrays with positional args.
#[test]
fn execute_program_call_user_func_array_dispatches_object_method_array() {
    let program = parse_fragment(
        br#"$box = new Box(39);
return call_user_func_array([$box, "add2_x"], [1, 2]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(42));
}

/// Verifies static method callable arrays dispatch eval-declared static methods.
#[test]
fn execute_program_static_callable_array_dispatches_eval_method() {
    let program = parse_fragment(
        br#"class EvalStaticCallableBox {
    public static function join($left, $right) {
        return $left . $right;
    }
}
$cb = ["EvalStaticCallableBox", "join"];
echo $cb(right: "B", left: "A"); echo ":";
echo call_user_func($cb, "C", "D"); echo ":";
echo call_user_func_array($cb, ["right" => "F", "left" => "E"]); echo ":";
$named = "EvalStaticCallableBox::join";
return $named(right: "H", left: "G");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "AB:CD:EF:");
    assert_eq!(values.get(result), FakeValue::String("GH".to_string()));
}

/// Verifies fake runtime releases include both temporary callable-array index cells.
fn assert_released_array_callable_index_temps(values: &FakeOps) {
    assert!(
        values
            .releases
            .iter()
            .any(|release| values.get(*release) == FakeValue::Int(0)),
        "array callable index 0 temporary should be released"
    );
    assert!(
        values
            .releases
            .iter()
            .any(|release| values.get(*release) == FakeValue::Int(1)),
        "array callable index 1 temporary should be released"
    );
}
