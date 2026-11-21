//! Purpose:
//! Verifies staged checked modulo through both interpreter consumers.
//!
//! Called from:
//! - Focused Magician unit tests.
//!
//! Key details:
//! - Covers numeric-string conversion, ordered diagnostics and source-preserving failures.

use super::super::*;
use super::support::*;
use super::strict_semantics::staged_numeric_execution;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Both eval engines preserve PHP integer conversion, modulo edges and diagnostic ordering.
#[test]
fn strict_eval_modulo_staged_walker_vm_values_diagnostics_and_owners() {
    for (source, expected, warnings, class) in [
        ("return -9223372036854775808 % -1;", Some(0), 0, ""),
        ("return 7 % 2;", Some(1), 0, ""),
        ("return -7 % 2;", Some(-1), 0, ""),
        ("return '9223372036854775807' % 2;", Some(1), 0, ""),
        ("return '9223372036854775808' % 2;", Some(1), 0, ""),
        ("return '1e20' % 2;", Some(1), 1, ""),
        ("return 1.0e20 % 2;", Some(0), 1, ""),
        ("return '1e999' % 2;", Some(0), 1, ""),
        ("return 7.5 % 2;", Some(1), 1, ""),
        ("return '5.5x' % '2.5y';", Some(1), 4, ""),
        ("return '5.5x' % 'bad';", None, 2, "TypeError"),
        ("return 1 % 0;", None, 0, "DivisionByZeroError"),
        ("$saved = 7; $saved %= 0;", None, 0, "DivisionByZeroError"),
    ] {
        for vm in [false, true] {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true)).unwrap();
            let mut scope = ElephcEvalScope::new();
            let mut values = FakeOps::default();
            let outcome = staged_numeric_execution(source, vm, &mut context, &mut scope, &mut values);
            match (expected, outcome) {
                (Some(expected), Ok(EvalControl::Return(value))) => {
                    assert_eq!(values.get(value), FakeValue::Int(expected), "{source}, vm={vm}");
                    values.release(value).unwrap();
                }
                (None, Err(EvalStatus::UncaughtThrowable)) => {
                    let error = context.take_pending_throw().expect("modulo exception owner");
                    assert!(values.object_is_a(error, class, false).unwrap(), "{source}, vm={vm}");
                    values.release(error).unwrap();
                    if source.contains("$saved") {
                        assert_eq!(values.get(scope.visible_cell("saved").unwrap()), FakeValue::Int(7));
                    }
                }
                _ => panic!("unexpected modulo outcome: {source}, vm={vm}"),
            }
            assert_eq!(values.warnings.len(), warnings, "{source}, vm={vm}");
            if source.contains("'2.5y'") {
                for (index, message) in values.warnings.iter().enumerate() {
                    assert_eq!(message.starts_with("Warning:"), index % 2 == 0, "{source}");
                }
            }
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for (id, owners) in &values.cell_owners {
                assert_eq!(*owners, 0, "{source}, vm={vm}: {:?}", values.values[id]);
            }
        }
    }
}
