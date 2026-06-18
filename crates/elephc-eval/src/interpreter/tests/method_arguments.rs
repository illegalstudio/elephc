//! Purpose:
//! Interpreter tests for eval-declared method and constructor argument binding.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - These cases cover named arguments and named unpacking on instance methods,
//!   static methods, and constructors declared inside eval fragments.

use super::super::*;
use super::support::*;

/// Verifies eval-declared instance, static, and constructor methods bind named args.
#[test]
fn execute_program_binds_eval_method_named_args() {
    let program = parse_fragment(
        br#"class EvalNamedMethodBox {
    public function __construct($left, $right) {
        $this->label = $left . $right;
    }
    public function read($left, $right) {
        return $this->label . ":" . $left . ":" . $right;
    }
    public static function join($left, $right) {
        return $left . "-" . $right;
    }
}
$box = new EvalNamedMethodBox(right: "B", left: "A");
echo $box->read(right: "D", left: "C"); echo ":";
$args = ["right" => "F", "left" => "E"];
echo $box->read(...$args); echo ":";
return EvalNamedMethodBox::join(right: "H", left: "G");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "AB:C:D:AB:E:F:");
    assert_eq!(values.get(result), FakeValue::String("G-H".to_string()));
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

/// Verifies runtime/AOT method fallback binds registered native method named arguments.
#[test]
fn execute_program_binds_registered_runtime_method_named_args() {
    let program = parse_fragment(
        br#"$box = new KnownClass(10);
return $box->add2_x(right: 2, left: 3);"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut signature = NativeCallableSignature::new(2);
    assert!(signature.set_param_name(0, "left"));
    assert!(signature.set_param_name(1, "right"));
    assert!(context.define_native_method_signature("KnownClass", "add2_x", signature));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("registered runtime method named args should bind");

    assert_eq!(values.get(result), FakeValue::Int(15));
}

/// Verifies runtime/AOT method fallback rejects named arguments without metadata.
#[test]
fn execute_program_rejects_unregistered_named_args_for_runtime_method_fallback() {
    let program =
        parse_fragment(br#"return $this->answer(value: 1);"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let object = values.alloc(FakeValue::Object(Vec::new()));
    scope.set("this", object, ScopeCellOwnership::Borrowed);

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("unregistered runtime method fallback named args should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}
