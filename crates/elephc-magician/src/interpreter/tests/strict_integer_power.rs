//! Purpose:
//! Verifies eval power result kinds independently of the overflow guarantee.
//!
//! Called from:
//! - Focused Magician interpreter tests.
//!
//! Key details:
//! - Both execution engines preserve full integer precision and independently owned results.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Numeric domains preserve integer power and only the Overflow flag rejects promotion.
#[test]
fn strict_integer_power_eval_preserves_kinds_and_independent_overflow_policy() {
    for feature in [StrictFeature::Arithmetic, StrictFeature::Coercions] {
        for checked in [false, true] {
            for vm in [false, true] {
                for (source, expected) in [
                    ("return 2 ** 3;", Some(FakeValue::Int(8))),
                    ("return 9007199254740993 ** 1;", Some(FakeValue::Int(9007199254740993))),
                    ("return (-9223372036854775807 - 1) ** 1;", Some(FakeValue::Int(i64::MIN))),
                    ("return 2.0 ** 3;", Some(FakeValue::Float(8.0))),
                    ("return 654 ** 32;", if checked { None } else { Some(FakeValue::Float(1.2545499179770422e90)) }),
                ] {
                    let mut context = ElephcEvalContext::new();
                    context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(feature, true).with(StrictFeature::Overflow, checked)).unwrap();
                    let mut scope = ElephcEvalScope::new();
                    let mut values = FakeOps::default();
                    let outcome = staged_numeric_execution(source, vm, &mut context, &mut scope, &mut values);
                    if let Some(expected) = expected {
                        let Ok(EvalControl::Return(value)) = outcome else { panic!("{source}, {feature:?}, vm={vm}, checked={checked}"); };
                        assert_eq!(values.get(value), expected, "{source}, {feature:?}, vm={vm}, checked={checked}");
                        values.release(value).unwrap();
                    } else {
                        assert!(matches!(outcome, Err(EvalStatus::UncaughtThrowable)), "{source}, {feature:?}, vm={vm}");
                        let error = context.take_pending_throw().expect("power overflow owner");
                        assert!(values.object_is_a(error, "ArithmeticError", false).unwrap());
                        values.release(error).unwrap();
                    }
                    for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
                    for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}: {:?}", values.values[id]); }
                }
            }
        }
    }
}


/// Direct and indirect builtin powers share numeric domains, precise results and owned failure state.
#[test]
fn strict_numeric_builtin_pow_eval_profiles_call_surfaces_and_owners() {
    for feature in [StrictFeature::Arithmetic, StrictFeature::Coercions] {
        for checked in [false, true] {
            for vm in [false, true] {
                for (call, expected, failure, vm_supported) in [
                    ("pow(2, 3)", Some(FakeValue::Int(8)), "", true),
                    ("pow(9007199254740993, 1)", Some(FakeValue::Int(9007199254740993)), "", true),
                    ("pow(2.0, 3)", Some(FakeValue::Float(8.0)), "", true),
                    ("pow('2', 3)", None, "TypeError", true),
                    ("pow(true, 3)", None, "TypeError", true),
                    ("pow(2, -1)", if feature == StrictFeature::Arithmetic { None } else { Some(FakeValue::Float(0.5)) }, "ArithmeticError", true),
                    ("pow(2, 63)", if checked { None } else { Some(FakeValue::Float(9_223_372_036_854_775_808.0)) }, "ArithmeticError", true),
                    ("pow(exponent: 3, num: 2)", Some(FakeValue::Int(8)), "", false),
                    ("$alias(2, 3)", Some(FakeValue::Int(8)), "", false),
                    ("call_user_func('pow', 2, 3)", Some(FakeValue::Int(8)), "", true),
                    ("call_user_func_array('pow', [2, 3])", Some(FakeValue::Int(8)), "", false),
                ] {
                    if vm && !vm_supported { continue; }
                    let shapes = if vm { "if (false) { function pow($base, $exponent) {} function call_user_func($f, $a, $b) {} }" } else { "" };
                    let alias = if call.starts_with("$alias") { "$alias = pow(...);" } else { "" };
                    let source = format!("{shapes} {alias} $result = 23; $result = {call}; return $result;");
                    let mut context = ElephcEvalContext::new();
                    context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(feature, true).with(StrictFeature::Overflow, checked)).unwrap();
                    let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
                    let outcome = staged_numeric_execution(&source, vm, &mut context, &mut scope, &mut values);
                    if let Some(expected) = expected {
                        let Ok(EvalControl::Return(value)) = outcome else { panic!("{source}, {feature:?}, checked={checked}, vm={vm}"); };
                        assert_eq!(values.get(value), expected, "{source}, {feature:?}, checked={checked}, vm={vm}");
                        values.release(value).unwrap();
                    } else {
                        assert!(matches!(outcome, Err(EvalStatus::UncaughtThrowable)), "{source}, {feature:?}, checked={checked}, vm={vm}");
                        let error = context.take_pending_throw().expect("builtin power failure owner");
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
