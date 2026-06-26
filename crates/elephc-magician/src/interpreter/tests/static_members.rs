//! Purpose:
//! Interpreter tests for eval-declared static properties and static methods.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases cover storage persistence, visibility checks, and late static binding.

use super::super::*;
use super::support::*;

/// Verifies static properties persist and can be read and written through static methods.
#[test]
fn execute_program_reads_writes_eval_static_members() {
    let program = parse_fragment(
        br#"class EvalStaticCounter {
    public static int $count = 1;
    public static function bump($step) {
        self::$count += $step;
        return self::$count;
    }
}
echo EvalStaticCounter::$count; echo ":";
echo EvalStaticCounter::bump(2); echo ":";
return EvalStaticCounter::$count;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "1:3:");
    assert_eq!(values.get(result), FakeValue::Int(3));
}

/// Verifies `static::` uses the called class while `self::` keeps the declaring class.
#[test]
fn execute_program_late_binds_eval_static_property_access() {
    let program = parse_fragment(
        br#"class EvalStaticBase {
    protected static int $n = 2;
    public static function add($x) {
        static::$n += $x;
        return static::$n;
    }
    public static function baseRead() {
        return self::$n;
    }
}
class EvalStaticChild extends EvalStaticBase {
    protected static int $n = 10;
}
echo EvalStaticChild::add(4); echo ":";
echo EvalStaticBase::add(3); echo ":";
return EvalStaticBase::baseRead();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "14:5:");
    assert_eq!(values.get(result), FakeValue::Int(5));
}

/// Verifies private static property access from global eval scope throws Error.
#[test]
fn execute_program_private_eval_static_property_from_global_scope_throws_error() {
    let program = parse_fragment(
        br#"class EvalStaticPrivate {
    private static int $secret = 4;
}
try {
    echo EvalStaticPrivate::$secret;
    echo "bad";
} catch (Error $e) {
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
        "Error:Cannot access private property EvalStaticPrivate::$secret"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies invalid eval-declared static property access throws PHP-compatible Error values.
#[test]
fn execute_program_invalid_eval_static_property_access_throws_error() {
    let program = parse_fragment(
        br#"class EvalStaticPropertyErrors {
    public int $instance = 1;
    public static int $typed;
}
try {
    echo EvalStaticPropertyErrors::$missing;
    echo "bad";
} catch (Error $e) {
    echo get_class($e); echo ":"; echo $e->getMessage();
}
echo "|";
try {
    echo EvalStaticPropertyErrors::$instance;
    echo "bad";
} catch (Error $e) {
    echo get_class($e); echo ":"; echo $e->getMessage();
}
echo "|";
try {
    echo EvalStaticPropertyErrors::$typed;
    echo "bad";
} catch (Error $e) {
    echo get_class($e); echo ":"; echo $e->getMessage();
}
echo "|";
try {
    EvalStaticPropertyErrors::$missing = 9;
    echo "bad";
} catch (Error $e) {
    echo get_class($e); echo ":"; echo $e->getMessage();
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Error:Access to undeclared static property EvalStaticPropertyErrors::$missing|\
Error:Access to undeclared static property EvalStaticPropertyErrors::$instance|\
Error:Typed static property EvalStaticPropertyErrors::$typed must not be accessed before initialization|\
Error:Access to undeclared static property EvalStaticPropertyErrors::$missing"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies generated/AOT static property misses distinguish missing classes from missing properties.
#[test]
fn execute_program_invalid_runtime_static_property_access_throws_error() {
    let program = parse_fragment(
        br#"try {
    echo KnownClass::$missing;
    echo "bad";
} catch (Error $e) {
    echo get_class($e); echo ":"; echo $e->getMessage();
}
echo "|";
try {
    echo MissingRuntimeStaticClass::$missing;
    echo "bad";
} catch (Error $e) {
    echo get_class($e); echo ":"; echo $e->getMessage();
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Error:Access to undeclared static property KnownClass::$missing|\
Error:Class \"MissingRuntimeStaticClass\" not found"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies invalid eval-declared static method calls throw PHP-compatible Error values.
#[test]
fn execute_program_invalid_static_method_calls_throw_error() {
    let program = parse_fragment(
        br#"class EvalStaticCallRules {
    public function read() { return 1; }
}
class EvalStaticMissingRules {}
abstract class EvalStaticAbstractRules {
    abstract public static function abs();
}
try {
    EvalStaticCallRules::read();
    echo "bad";
} catch (Error $e) {
    echo get_class($e); echo ":"; echo $e->getMessage();
}
echo "|";
try {
    EvalStaticMissingRules::missing();
    echo "bad";
} catch (Error $e) {
    echo get_class($e); echo ":"; echo $e->getMessage();
}
echo "|";
try {
    EvalStaticAbstractRules::abs();
    echo "bad";
} catch (Error $e) {
    echo get_class($e); echo ":"; echo $e->getMessage();
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Error:Non-static method EvalStaticCallRules::read() cannot be called statically|\
Error:Call to undefined method EvalStaticMissingRules::missing()|\
Error:Cannot call abstract method EvalStaticAbstractRules::abs()"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval allows object-style calls to accessible static methods.
#[test]
fn execute_program_allows_instance_call_to_eval_static_method() {
    let program = parse_fragment(
        br#"class EvalStaticInstanceRules {
    public static function read() { return 1; }
}
$box = new EvalStaticInstanceRules();
return $box->read();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(1));
}

/// Verifies missing and inaccessible instance methods dispatch through `__call`.
#[test]
fn execute_program_dispatches_eval_magic_call() {
    let program = parse_fragment(
        br#"class EvalMagicCallBox {
    private function hidden($value) { return "bad"; }
    public function __call($method, $args) {
        return $method . ":" . $args[0] . ":" . $args["name"];
    }
}
$box = new EvalMagicCallBox();
echo $box->DoThing("A", name: "B"); echo ":";
return $box->hidden("C", name: "D");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "DoThing:A:B:");
    assert_eq!(
        values.get(result),
        FakeValue::String("hidden:C:D".to_string())
    );
}

/// Verifies missing and inaccessible static methods dispatch through `__callStatic`.
#[test]
fn execute_program_dispatches_eval_magic_call_static() {
    let program = parse_fragment(
        br#"class EvalMagicStaticBox {
    private static function hidden($value) { return "bad"; }
    public static function __callStatic($method, $args) {
        return $method . ":" . $args[0] . ":" . $args["name"];
    }
}
echo EvalMagicStaticBox::DoStatic("A", name: "B"); echo ":";
return EvalMagicStaticBox::Hidden("C", name: "D");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "DoStatic:A:B:");
    assert_eq!(
        values.get(result),
        FakeValue::String("Hidden:C:D".to_string())
    );
}
