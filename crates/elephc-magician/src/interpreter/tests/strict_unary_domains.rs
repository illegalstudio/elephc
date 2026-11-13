//! Purpose:
//! Verifies unary and update operand domains through both eval engines.
//!
//! Called from:
//! - Focused Magician interpreter tests.
//!
//! Key details:
//! - Invalid operands do not alter a pending destination or issue coercion warnings.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Independent numeric profiles guard unary values and failed prefix/postfix writes without leaking.
#[test]
fn strict_unary_domain_eval_walker_vm_values_updates_and_owners() {
    for feature in [StrictFeature::Arithmetic, StrictFeature::Coercions] {
        for checked in [false, true] {
            for vm in [false, true] {
                for (source, expected) in [
                    ("return -true;", None), ("return -null;", None), ("return -'2';", None),
                    ("return ~7.0;", None), ("return ~false;", None),
                    ("$saved = 'abc'; $saved++; return $saved;", None),
                    ("$saved = false; ++$saved; return $saved;", None),
                    ("$saved = null; --$saved; return $saved;", None),
                    ("return ~'A';", if feature == StrictFeature::Arithmetic { None } else { Some(FakeValue::Bytes(vec![0xbe])) }),
                    ("return -2.5;", Some(FakeValue::Float(-2.5))),
                    ("return ~7;", Some(FakeValue::Int(-8))),
                    ("$saved = 7; $old = $saved; $saved++; return $old * 10 + $saved;", Some(FakeValue::Int(78))),
                    ("$saved = 7.5; --$saved; return $saved;", Some(FakeValue::Float(6.5))),
                ] {
                    let mut context = ElephcEvalContext::new();
                    context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(feature, true).with(StrictFeature::Overflow, checked)).unwrap();
                    let mut scope = ElephcEvalScope::new();
                    let mut values = FakeOps::default();
                    let outcome = staged_numeric_execution(source, vm, &mut context, &mut scope, &mut values);
                    if let Some(expected) = expected {
                        let Ok(EvalControl::Return(value)) = outcome else { panic!("{source}, {feature:?}, vm={vm}, checked={checked}"); };
                        assert_eq!(values.get(value), expected, "{source}, {feature:?}, vm={vm}");
                        values.release(value).unwrap();
                    } else {
                        assert!(matches!(outcome, Err(EvalStatus::UncaughtThrowable)), "{source}, {feature:?}, vm={vm}");
                        let error = context.take_pending_throw().expect("unary domain Throwable owner");
                        assert!(values.object_is_a(error, "TypeError", false).unwrap(), "{source}");
                        values.release(error).unwrap();
                        if source.contains("$saved = 'abc'") { assert_eq!(values.get(scope.visible_cell("saved").unwrap()), FakeValue::String("abc".into())); }
                        if source.contains("$saved = false") { assert_eq!(values.get(scope.visible_cell("saved").unwrap()), FakeValue::Bool(false)); }
                        if source.contains("$saved = null") { assert_eq!(values.get(scope.visible_cell("saved").unwrap()), FakeValue::Null); }
                    }
                    assert!(values.warnings.is_empty(), "{source}");
                    for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
                    for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}: {:?}", values.values[id]); }
                }
            }
        }
    }
}


/// Numeric updates promote only when the independently selected Overflow guarantee permits it.
#[test]
fn strict_unary_domain_eval_update_boundaries_preserve_independent_overflow() {
    for feature in [StrictFeature::Arithmetic, StrictFeature::Coercions] {
        for checked in [false, true] {
            for vm in [false, true] {
                for source in ["$saved = 9223372036854775807; ++$saved; return $saved;",
                    "$saved = -9223372036854775807 - 1; --$saved; return $saved;"] {
                    let mut context = ElephcEvalContext::new();
                    context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(feature, true).with(StrictFeature::Overflow, checked)).unwrap();
                    let mut scope = ElephcEvalScope::new();
                    let mut values = FakeOps::default();
                    let outcome = staged_numeric_execution(source, vm, &mut context, &mut scope, &mut values);
                    if checked {
                        assert!(matches!(outcome, Err(EvalStatus::UncaughtThrowable)), "{source}, vm={vm}");
                        let error = context.take_pending_throw().expect("update overflow owner");
                        assert!(values.object_is_a(error, "ArithmeticError", false).unwrap());
                        values.release(error).unwrap();
                        assert!(matches!(values.get(scope.visible_cell("saved").unwrap()), FakeValue::Int(_)));
                    } else {
                        let Ok(EvalControl::Return(value)) = outcome else { panic!("{source}, vm={vm}"); };
                        assert!(matches!(values.get(value), FakeValue::Float(_)));
                        assert!(matches!(values.get(scope.visible_cell("saved").unwrap()), FakeValue::Float(_)));
                        values.release(value).unwrap();
                    }
                    for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
                    for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}: {:?}", values.values[id]); }
                }
            }
        }
    }
}
