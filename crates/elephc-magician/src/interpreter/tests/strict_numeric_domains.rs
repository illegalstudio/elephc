//! Purpose:
//! Verifies shared numeric domain rules through both eval execution engines.
//!
//! Called from:
//! - Focused Magician interpreter tests.
//!
//! Key details:
//! - Failures preserve assigned values and transfer exactly one Throwable owner.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Walker and bytecode agree on exact kinds, power restrictions and safe integer boundaries.
#[test]
fn strict_numeric_domain_eval_walker_vm_kinds_values_and_ownership() {
    for feature in [StrictFeature::Arithmetic, StrictFeature::Coercions] {
        for overflow in [false, true] {
            for vm in [false, true] {
                for (source, class, expected) in [
                    ("return '2' + 1;", "TypeError", None),
                    ("return true * 2;", "TypeError", None),
                    ("return null - 1;", "TypeError", None),
                    ("return 7.0 % 2;", "TypeError", None),
                    ("return 'a' & 'b';", "TypeError", None),
                    ("return 1 << -1;", "ArithmeticError", None),
                    ("return 1 / 0;", "DivisionByZeroError", None),
                    ("$saved = 7; $saved %= 0;", "DivisionByZeroError", None),
                    ("return 2 ** -1;", if feature == StrictFeature::Arithmetic { "ArithmeticError" } else { "" }, Some(FakeValue::Float(0.5))),
                    ("return 2.0 ** -1;", "", Some(FakeValue::Float(0.5))),
                    ("return 2 + 3;", "", Some(FakeValue::Int(5))),
                    ("return 1 << 64;", "", Some(FakeValue::Int(0))),
                    ("return -1 >> 64;", "", Some(FakeValue::Int(-1))),
                ] {
                    let profile = SemanticProfile::PERMISSIVE.with(feature, true).with(StrictFeature::Overflow, overflow);
                    let mut context = ElephcEvalContext::new();
                    context.install_semantic_profile(profile).unwrap();
                    let mut scope = ElephcEvalScope::new();
                    let mut values = FakeOps::default();
                    let result = staged_numeric_execution(source, vm, &mut context, &mut scope, &mut values);
                    if class.is_empty() {
                        let Ok(EvalControl::Return(value)) = result else { panic!("unexpected result: {source}, {profile:?}, vm={vm}"); };
                        assert_eq!(Some(values.get(value)), expected, "{source}, {profile:?}, vm={vm}");
                        values.release(value).unwrap();
                    } else {
                        assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)), "{source}, {profile:?}, vm={vm}");
                        let error = context.take_pending_throw().expect("domain Throwable owner");
                        assert!(values.object_is_a(error, class, false).unwrap(), "{source}, {profile:?}, vm={vm}");
                        values.release(error).unwrap();
                        if source.contains("$saved") { assert_eq!(values.get(scope.visible_cell("saved").unwrap()), FakeValue::Int(7)); }
                    }
                    for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
                    for (id, owners) in &values.cell_owners {
                        assert_eq!(*owners, 0, "{source}, {profile:?}, vm={vm}: {:?}", values.values[id]);
                    }
                    assert!(values.warnings.is_empty());
                }
            }
        }
    }
}
