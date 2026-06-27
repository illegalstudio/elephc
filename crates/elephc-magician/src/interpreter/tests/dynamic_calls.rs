//! Purpose:
//! Interpreter tests for variable calls, call_user_func, call_user_func_array, and duplicate dynamic declarations.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
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

/// Verifies first-class callable syntax dispatches through eval's callback paths.
#[test]
fn execute_program_first_class_callables_dispatch_functions_and_methods() {
    let program = parse_fragment(
        br#"function eval_fc_double($value) {
    return $value * 2;
}
class EvalFirstClassCallableBase {
    public function __construct($offset = 1) {
        $this->offset = $offset;
    }
    public function add($value) {
        return $value + $this->offset;
    }
    public function keep($value) {
        return $value > 2;
    }
    public function sum($carry, $value) {
        return $carry + $value + $this->offset;
    }
    public function show($value, $key) {
        echo $key . $value;
    }
    public static function join($left, $right) {
        return $left . $right;
    }
    public static function compareDesc($left, $right) {
        return $right - $left;
    }
    public static function label($value) {
        return "base:" . $value;
    }
    public static function relay($value) {
        $fn = static::label(...);
        return $fn($value);
    }
}
class EvalFirstClassCallableChild extends EvalFirstClassCallableBase {
    public static function label($value) {
        return "child:" . $value;
    }
}
$function = eval_fc_double(...);
echo $function(4); echo ":";
echo (strlen(...))("abcd"); echo ":";
$box = new EvalFirstClassCallableBase(3);
$method = $box->add(...);
echo $method(4); echo ":";
echo call_user_func($box->add(...), 5); echo ":";
$static = EvalFirstClassCallableBase::join(...);
echo $static(right: "B", left: "A"); echo ":";
$mapped = array_map($box->add(...), [1, 2]);
echo $mapped[0] . $mapped[1] . ":";
$filtered = array_filter([1, 2, 3, 4], $box->keep(...));
echo count($filtered) . ":";
echo array_reduce([1, 2], $box->sum(...), 0) . ":";
array_walk(["a" => 1], $box->show(...));
echo ":";
$sorted = [3, 1, 2];
usort($sorted, EvalFirstClassCallableBase::compareDesc(...));
echo $sorted[0] . $sorted[1] . $sorted[2] . ":";
return EvalFirstClassCallableChild::relay("ok");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "8:4:7:8:AB:45:2:9:a1:321:");
    assert_eq!(
        values.get(result),
        FakeValue::String("child:ok".to_string())
    );
}

/// Verifies first-class static callables preserve late-static forwarding metadata.
#[test]
fn execute_program_first_class_static_callables_preserve_called_class() {
    let program = parse_fragment(
        br#"class EvalFirstClassStaticForwardBase {
    public static function who() {
        return static::tag();
    }
    public static function tag() {
        return "base";
    }
    public static function relaySelf() {
        $fn = self::who(...);
        return $fn();
    }
}
class EvalFirstClassStaticForwardChild extends EvalFirstClassStaticForwardBase {
    public static function relayParent() {
        $fn = parent::who(...);
        return $fn();
    }
    public static function tag() {
        return "child";
    }
}
echo EvalFirstClassStaticForwardChild::relayParent(); echo ":";
return EvalFirstClassStaticForwardChild::relaySelf();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "child:");
    assert_eq!(values.get(result), FakeValue::String("child".to_string()));
}

/// Verifies invokable eval objects dispatch through variable and callback call paths.
#[test]
fn execute_program_invokes_eval_object_callables() {
    let program = parse_fragment(
        br#"function eval_plain_call_side_effect() {
    echo "bad";
    return "x";
}
class EvalInvokableBox {
    public function __construct($label = "box") {
        $this->label = $label;
    }
    public function __invoke($left = "A", $right = "B") {
        return $this->label . ":" . $left . $right;
    }
}
class EvalPlainCallableProbe {}
$box = new EvalInvokableBox("box");
$plain = new EvalPlainCallableProbe();
echo is_callable($box) ? "Y:" : "N:";
echo is_callable($plain) ? "bad:" : "plain:";
echo $box(right: "D", left: "C"); echo ":";
try {
    $plain(eval_plain_call_side_effect());
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo ":";
echo (new EvalInvokableBox("new"))("E", "F"); echo ":";
echo call_user_func($box, "G", "H"); echo ":";
$first = $box(...);
echo $first("K", "L"); echo ":";
return call_user_func_array($box, ["right" => "J", "left" => "I"]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Y:plain:box:CD:Error:Object of type EvalPlainCallableProbe is not callable:new:EF:box:GH:box:KL:"
    );
    assert_eq!(values.get(result), FakeValue::String("box:IJ".to_string()));
}

/// Verifies call_user_func rejects non-invokable eval objects with PHP's TypeError.
#[test]
fn execute_program_call_user_func_rejects_non_invokable_eval_object() {
    let program = parse_fragment(
        br#"class EvalPlainCallbackError {}
$plain = new EvalPlainCallbackError();
try {
    call_user_func($plain);
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    call_user_func_array($plain, []);
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "TypeError:call_user_func(): Argument #1 ($callback) must be a valid callback, no array or string given|\
TypeError:call_user_func_array(): Argument #1 ($callback) must be a valid callback, no array or string given"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies call_user_func rejects invalid object-method callable arrays with PHP's TypeError.
#[test]
fn execute_program_call_user_func_rejects_invalid_object_method_arrays() {
    let program = parse_fragment(
        br#"class EvalMissingCallbackArray {}
class EvalPrivateCallbackArray {
    private function hidden() {
        return "bad";
    }
}
class EvalInstanceCallbackArray {
    public function inst() {
        return "bad";
    }
}
$missing = new EvalMissingCallbackArray();
try {
    call_user_func([$missing, "MiSsInG"]);
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    call_user_func_array([$missing, "missing"], []);
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    call_user_func([new EvalPrivateCallbackArray(), "hidden"]);
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    call_user_func(["EvalInstanceCallbackArray", "inst"]);
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "TypeError:call_user_func(): Argument #1 ($callback) must be a valid callback, class EvalMissingCallbackArray does not have a method \"MiSsInG\"|\
TypeError:call_user_func_array(): Argument #1 ($callback) must be a valid callback, class EvalMissingCallbackArray does not have a method \"missing\"|\
TypeError:call_user_func(): Argument #1 ($callback) must be a valid callback, cannot access private method EvalPrivateCallbackArray::hidden()|\
TypeError:call_user_func(): Argument #1 ($callback) must be a valid callback, non-static method EvalInstanceCallbackArray::inst() cannot be called statically"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies call_user_func callable arrays still dispatch through magic method fallbacks.
#[test]
fn execute_program_call_user_func_arrays_dispatch_magic_method_fallbacks() {
    let program = parse_fragment(
        br#"class EvalMagicCallbackArray {
    public function __call($method, $args) {
        return $method . ":" . $args[0];
    }
    public static function __callStatic($method, $args) {
        return $method . ":" . $args[0];
    }
}
$box = new EvalMagicCallbackArray();
echo is_callable([$box, "missing"]) ? "Y:" : "N:";
echo call_user_func([$box, "missing"], "A") . ":";
echo call_user_func_array([$box, "missing"], ["B"]) . ":";
echo is_callable(["EvalMagicCallbackArray", "static_missing"]) ? "S:" : "s:";
return call_user_func(["EvalMagicCallbackArray", "static_missing"], "C");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Y:missing:A:missing:B:S:");
    assert_eq!(
        values.get(result),
        FakeValue::String("static_missing:C".to_string())
    );
}

/// Verifies object-method callable arrays preserve eval named-argument binding.
#[test]
fn execute_program_object_method_callable_array_binds_eval_named_args() {
    let program = parse_fragment(
        br#"class EvalObjectCallableArrayBox {
    public function join($left, $right) {
        return $left . $right;
    }
}
$box = new EvalObjectCallableArrayBox();
$cb = [$box, "join"];
echo is_callable($cb) ? "Y:" : "N:";
return call_user_func_array($cb, ["right" => "B", "left" => "A"]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Y:");
    assert_eq!(values.get(result), FakeValue::String("AB".to_string()));
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
    let mut context = ElephcEvalContext::new();
    let mut join_signature = NativeCallableSignature::new(2);
    assert!(join_signature.set_param_name(0, "left"));
    assert!(join_signature.set_param_name(1, "right"));
    assert!(context.define_native_static_method_signature(
        "KnownClass",
        "join",
        join_signature
    ));
    let mut sum_signature = NativeCallableSignature::new(2);
    assert!(sum_signature.set_param_name(0, "left"));
    assert!(sum_signature.set_param_name(1, "right"));
    assert!(context.define_native_static_method_signature(
        "KnownClass",
        "sum",
        sum_signature
    ));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

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

/// Verifies runtime AOT static method fallback fills registered scalar defaults.
#[test]
fn execute_program_static_runtime_method_hook_binds_default_args() {
    let program = parse_fragment(
        br#"echo KnownClass::join("A"); echo ":";
return call_user_func_array(["KnownClass", "join"], ["left" => "C"]);"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut signature = NativeCallableSignature::new(2);
    assert!(signature.set_param_name(0, "left"));
    assert!(signature.set_param_name(1, "right"));
    assert!(signature.set_param_default(1, NativeCallableDefault::String("B".to_string())));
    assert!(context.define_native_static_method_signature("KnownClass", "join", signature));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("registered AOT defaults should bind");

    assert_eq!(values.output, "AB:");
    assert_eq!(values.get(result), FakeValue::String("CB".to_string()));
}

/// Verifies runtime AOT static method fallback honors by-reference parameter metadata.
#[test]
fn execute_program_static_runtime_method_hook_rejects_by_ref_temporary_arg() {
    let program = parse_fragment(br#"return KnownClass::sum(1, 2);"#)
        .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut signature = NativeCallableSignature::new(2);
    assert!(signature.set_param_name(0, "left"));
    assert!(signature.set_param_name(1, "right"));
    assert!(signature.set_param_by_ref(0, true));
    assert!(context.define_native_static_method_signature("KnownClass", "sum", signature));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect_err("literal cannot satisfy a static by-reference method parameter");

    assert_eq!(err, EvalStatus::RuntimeFatal);
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

    assert_eq!(error, EvalStatus::UncaughtThrowable);
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
