//! Purpose:
//! Checks exact callability for local contracts in both eval engines.
//!
//! Called from:
//! - Focused interpreter strict local regressions.
//!
//! Key details:
//! - String shape alone does not establish callable membership and rejected writes keep owners.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Callable local stores validate lookup independently from strict coercions in walker and VM.
#[test]
fn strict_structural_locals_eval_checks_callable_lookup_and_owners() {
    for vm in [false, true] {
        for (source, accepts) in [("$value=1;", false), ("$value='missingStructuralCallback';", false),
            ("$value='replacementStructuralCallback';", true), ("$value='REPLACEMENTSTRUCTURALCALLBACK';", true)] {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            staged_numeric_execution("function originalStructuralCallback(): int {return 1;} \
                function replacementStructuralCallback(): int {return 2;} \
                callable $value='originalStructuralCallback';", false, &mut context, &mut scope, &mut values).unwrap();
            let result = staged_numeric_execution(source, vm, &mut context, &mut scope, &mut values);
            if accepts { result.unwrap_or_else(|status| panic!("{source}, vm={vm}: {status:?}")); }
            else {
                assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)), "{source}, vm={vm}");
                assert_eq!(values.get(scope.visible_cell("value").unwrap()), FakeValue::String("originalStructuralCallback".into()));
                let error = context.take_pending_throw().expect("local callable TypeError owner");
                assert!(values.object_is_a(error, "TypeError", false).unwrap()); values.release(error).unwrap();
            }
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
            for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}, vm={vm}: {:?}", values.values[id]); }
        }
    }
}
