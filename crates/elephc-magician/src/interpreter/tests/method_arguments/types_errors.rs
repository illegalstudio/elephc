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

/// Verifies defaults before required eval method parameters do not make earlier slots optional.
#[test]
fn execute_program_rejects_eval_method_default_before_required_omission() {
    let program = parse_fragment(
        br#"class EvalRequiredAfterDefaultBox {
    public function read($left = "A", $right) {
        return $left . $right;
    }
}
$box = new EvalRequiredAfterDefaultBox();
return $box->read(right: "B");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("default before required parameter should remain required");

    assert_eq!(err, EvalStatus::RuntimeFatal);
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
return $box->read("not numeric");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("non-numeric string should fail int parameter type");

    assert_eq!(err, EvalStatus::RuntimeFatal);
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
