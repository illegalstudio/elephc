//! Purpose:
//! Checks noncoercive scalar parameter binding in both eval engines.
//!
//! Called from:
//! - Focused interpreter tests with explicit owner accounting.
//!
//! Key details:
//! - Binding failures preserve caller results and transfer a catchable TypeError owner.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Exact declarations accept their own kinds and reject scalar conversion without leaking arguments.
#[test]
fn strict_conversion_parameters_eval_engines_exact_members_and_owners() {
    for vm in [false, true] {
        for (declaration, argument, accepted) in [
            ("float", "2.0", true), ("float", "2", false), ("int", "2", true),
            ("int", "'2'", false), ("string", "'text'", true), ("string", "2", false),
            ("bool", "false", true), ("bool", "1", false), ("int|string", "'2'", true),
            ("int|string", "2.0", false), ("?int", "null", true),
            ("stdClass|float", "2.0", true), ("stdClass|float", "2", false),
            ("stdClass|float", "$nominal", true), ("?stdClass", "null", true),
        ] {
            let declaration = format!("function exactParameter({declaration} $v) {{ return $v; }}");
            let source = format!("{} $result = 23; $result = exactParameter({argument});",
                if vm { "if (false) { function exactParameter($v) {} }" } else { "" });
            // Object construction is staged in the walker; both callers consume the same owned value.
            let declaration = format!("{declaration} $nominal = new stdClass();");
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            staged_numeric_execution(&declaration, false, &mut context, &mut scope, &mut values).unwrap();
            let outcome = staged_numeric_execution(&source, vm, &mut context, &mut scope, &mut values);
            if accepted { assert!(outcome.is_ok(), "{source}, vm={vm}"); }
            else {
                assert!(matches!(outcome, Err(EvalStatus::UncaughtThrowable)), "{source}, vm={vm}");
                assert_eq!(values.get(scope.visible_cell("result").unwrap()), FakeValue::Int(23));
                let error = context.take_pending_throw().expect("binding error owner");
                assert!(values.object_is_a(error, "TypeError", false).unwrap()); values.release(error).unwrap();
            }
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}, vm={vm}: {:?}", values.values[id]); }
        }
    }
}
