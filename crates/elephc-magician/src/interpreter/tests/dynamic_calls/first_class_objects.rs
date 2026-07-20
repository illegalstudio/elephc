//! Purpose:
//! Interpreter tests for first-class function/method callables, invokable eval
//! objects, magic fallback, and named object-method arguments.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Static called-class state and invalid object callables are covered.

use super::super::super::*;
use super::super::support::*;

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
$walked = ["a" => 1];
array_walk($walked, $box->show(...));
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
class EvalPrivateInvokeCallbackArray {
    private function __invoke() {
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
    call_user_func([new EvalPrivateInvokeCallbackArray(), "__invoke"]);
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
TypeError:call_user_func(): Argument #1 ($callback) must be a valid callback, cannot access private method EvalPrivateInvokeCallbackArray::__invoke()|\
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
