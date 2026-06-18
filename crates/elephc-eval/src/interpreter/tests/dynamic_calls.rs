//! Purpose:
//! Interpreter tests for variable calls, call_user_func, call_user_func_array, and duplicate dynamic declarations.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - These cases verify callable dispatch across eval functions, builtins, native functions, and object methods.

use super::super::*;
use super::support::*;

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

/// Verifies static calls fall back to runtime AOT hooks when no eval class matches.
#[test]
fn execute_program_static_call_dispatches_runtime_method_hook() {
    let program = parse_fragment(
        br#"echo KnownClass::join("A", "B"); echo ":";
$cb = ["KnownClass", "join"];
echo call_user_func($cb, "C", "D"); echo ":";
$named = "KnownClass::join";
echo $named("E", "F"); echo ":";
return call_user_func_array(["KnownClass", "sum"], [2, 5]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "AB:CD:EF:");
    assert_eq!(values.get(result), FakeValue::Int(7));
}

/// Verifies runtime AOT static method fallback binds registered named arguments.
#[test]
fn execute_program_static_runtime_method_hook_binds_named_args() {
    let program = parse_fragment(
        br#"return call_user_func_array(["KnownClass", "join"], ["right" => "B", "left" => "A"]);"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut signature = NativeCallableSignature::new(2);
    assert!(signature.set_param_name(0, "left"));
    assert!(signature.set_param_name(1, "right"));
    assert!(context.define_native_static_method_signature("KnownClass", "join", signature));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("registered named AOT call should bind");

    assert_eq!(values.get(result), FakeValue::String("AB".to_string()));
}

/// Verifies runtime AOT static method fallback rejects named arguments without metadata.
#[test]
fn execute_program_static_runtime_method_hook_rejects_unregistered_named_args() {
    let program = parse_fragment(
        br#"return call_user_func_array(["KnownClass", "join"], ["right" => "B", "left" => "A"]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let error =
        execute_program(&program, &mut scope, &mut values).expect_err("named AOT call should fail");

    assert_eq!(error, EvalStatus::RuntimeFatal);
}

/// Verifies `call_user_func_array` inside eval can dispatch an eval-declared function.
#[test]
fn execute_program_call_user_func_array_dispatches_declared_function() {
    let program = parse_fragment(
        br#"function dyn($x, $y) { return $x + $y; }
return call_user_func_array("dyn", [4, 5]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(9));
}
/// Verifies `call_user_func_array` string keys bind eval-declared parameters by name.
#[test]
fn execute_program_call_user_func_array_binds_declared_named_args() {
    let program = parse_fragment(
        br#"function dyn($x, $y) { return ($x * 10) + $y; }
return call_user_func_array("dyn", ["y" => 2, "x" => 1]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(12));
}
/// Verifies context-level `call_user_func_array` dispatch binds eval-declared named args.
#[test]
fn execute_context_function_call_array_binds_declared_named_args() {
    let program = parse_fragment(br#"function dyn($x, $y) { return ($x * 10) + $y; }"#)
        .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let _ = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");
    let arg_array = values.assoc_new(2).expect("allocate argument array");
    let key_y = values.string("y").expect("allocate y key");
    let value_y = values.int(2).expect("allocate y value");
    let _ = values
        .array_set(arg_array, key_y, value_y)
        .expect("store y argument");
    let key_x = values.string("x").expect("allocate x key");
    let value_x = values.int(1).expect("allocate x value");
    let _ = values
        .array_set(arg_array, key_x, value_x)
        .expect("store x argument");

    let result = execute_context_function_call_array(&mut context, "dyn", arg_array, &mut values)
        .expect("execute context function call array");

    assert_eq!(values.get(result), FakeValue::Int(12));
}
/// Verifies `call_user_func_array` rejects positional values after named keys.
#[test]
fn execute_program_call_user_func_array_rejects_positional_after_named_arg() {
    let program = parse_fragment(
        br#"function dyn($x, $y) { return $x + $y; }
return call_user_func_array("dyn", ["y" => 2, 1]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values);

    assert_eq!(result, Err(EvalStatus::RuntimeFatal));
}
/// Verifies `call_user_func_array` inside eval can dispatch a supported builtin.
#[test]
fn execute_program_call_user_func_array_dispatches_builtin() {
    let program = parse_fragment(br#"return call_user_func_array("strlen", ["abcd"]);"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(4));
}
/// Verifies `call_user_func_array` inside eval can dispatch a registered native function.
#[test]
fn execute_program_call_user_func_array_dispatches_registered_native_function() {
    let program = parse_fragment(br#"return call_user_func_array("native_answer", [4, 5]);"#)
        .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let expected = values.int(42).expect("allocate fake result");
    let native = NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 2);
    assert!(context
        .define_native_function("native_answer", native)
        .is_ok());

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(result, expected);
}
/// Verifies `call_user_func_array` named keys can bind registered native parameters.
#[test]
fn execute_program_call_user_func_array_binds_registered_native_named_args() {
    let program = parse_fragment(
        br#"return call_user_func_array("native_answer", ["right" => 2, "left" => 1]);"#,
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
/// Verifies duplicate eval-declared function names fail in a shared context.
#[test]
fn execute_program_rejects_duplicate_declared_function() {
    let define = parse_fragment(br#"function dyn() { return 1; }"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program_with_context(&mut context, &define, &mut scope, &mut values)
        .expect("execute first declaration");
    let err = execute_program_with_context(&mut context, &define, &mut scope, &mut values)
        .expect_err("duplicate function declaration should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}
