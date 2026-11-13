//! Purpose:
//! Verifies original numeric builtin argument kinds at walker and VM boundaries.
//!
//! Called from:
//! - Focused Magician interpreter tests.
//!
//! Key details:
//! - Domain failures precede PHP coercion and leave failed destinations unchanged.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Numeric builtin domains and checked overflow remain independent across engines and call surfaces.
#[test]
fn strict_numeric_parameters_eval_engines_call_surfaces_and_owners() {
    for feature in [StrictFeature::Arithmetic, StrictFeature::Coercions] {
        for overflow in [false, true] {
            for vm in [false, true] {
                for (call, expected, failure, vm_supported) in [
                    ("abs(-3)", Some(FakeValue::Int(3)), "", true),
                    ("abs(-2.5)", Some(FakeValue::Float(2.5)), "", true),
                    ("abs('2')", None, "TypeError", true),
                    ("abs(true)", None, "TypeError", true),
                    ("abs(null)", None, "TypeError", true),
                    ("abs([1])", None, "TypeError", false),
                    ("intdiv(9, 2)", Some(FakeValue::Int(4)), "", true),
                    ("intdiv(9.0, 2)", None, "TypeError", true),
                    ("intdiv(9, '2')", None, "TypeError", true),
                    ("intdiv(true, 2)", None, "TypeError", true),
                    ("intdiv(null, 2)", None, "TypeError", true),
                    ("abs(-9223372036854775807 - 1)", if overflow { None } else { Some(FakeValue::Float(9_223_372_036_854_775_808.0)) }, "ArithmeticError", true),
                    ("intdiv(-9223372036854775807 - 1, -1)", None, "ArithmeticError", true),
                    ("abs(num: -3)", Some(FakeValue::Int(3)), "", false),
                    ("call_user_func('abs', '2')", None, "TypeError", true),
                    ("call_user_func_array('intdiv', [9.0, 2])", None, "TypeError", false),
                    ("$alias('2')", None, "TypeError", false),
                ] {
                    if vm && !vm_supported { continue; }
                    let shapes = if vm { "if (false) { function abs($n) {} function intdiv($a, $b) {} function call_user_func($f, $a) {} }" } else { "" };
                    let alias = if call.starts_with("$alias") { "$alias = abs(...);" } else { "" };
                    let source = format!("{shapes} {alias} $result = 23; $result = {call}; return $result;");
                    let mut context = ElephcEvalContext::new();
                    context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(feature, true).with(StrictFeature::Overflow, overflow)).unwrap();
                    let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
                    let outcome = staged_numeric_execution(&source, vm, &mut context, &mut scope, &mut values);
                    if let Some(expected) = expected {
                        let Ok(EvalControl::Return(value)) = outcome else { panic!("{source}, {feature:?}, overflow={overflow}, vm={vm}"); };
                        assert_eq!(values.get(value), expected, "{source}, {feature:?}, overflow={overflow}, vm={vm}");
                        values.release(value).unwrap();
                    } else {
                        assert!(matches!(outcome, Err(EvalStatus::UncaughtThrowable)), "{source}, {feature:?}, overflow={overflow}, vm={vm}");
                        let error = context.take_pending_throw().expect("numeric parameter failure owner");
                        assert!(values.object_is_a(error, failure, false).unwrap(), "{source}"); values.release(error).unwrap();
                        assert_eq!(values.get(scope.visible_cell("result").unwrap()), FakeValue::Int(23));
                    }
                    assert!(values.warnings.is_empty(), "{source}");
                    for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
                    for (id, count) in &values.cell_owners { assert_eq!(*count, 0, "{source}: {:?}", values.values[id]); }
                }
            }
        }
    }
}
