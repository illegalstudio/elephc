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
