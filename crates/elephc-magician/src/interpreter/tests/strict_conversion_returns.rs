//! Purpose:
//! Checks exact declared returns at walker and VM call boundaries.
//!
//! Called from:
//! - Focused interpreter regressions with fake-runtime owner accounting.
//!
//! Key details:
//! - Typed callees use the existing walker while compiled callers exercise shared dispatch.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Return coercions fail before destination publication and release the rejected result owner.
#[test]
fn strict_conversion_returns_eval_engines_original_kinds_and_owners() {
    for vm in [false, true] {
        for (declaration, argument, accepted) in [
            ("float", "2.0", true), ("float", "2", false), ("int", "2", true),
            ("int", "'2'", false), ("string", "'text'", true), ("string", "2", false),
            ("bool", "false", true), ("bool", "1", false), ("int|string", "'2'", true),
            ("int|string", "2.0", false), ("?int", "null", true),
            ("stdClass|float", "2.0", true), ("stdClass|float", "2", false),
            ("stdClass|float", "$nominal", true), ("?stdClass", "null", true),
            ("array|float", "$container", true), ("array|float", "2", false),
            ("iterable|float", "$container", true), ("iterable|float", "2", false),
            ("callable|float", "'exactCallback'", true), ("callable|float", "2", false),
            ("callable|float", "'missing_callback'", false),
            ("callable", "'exactCallback'", true), ("callable", "'missing_callback'", false),
        ] {
            let declaration = format!("function exactReturn($v): {declaration} {{ return $v; }}");
            let source = format!("{} $result = 23; $result = exactReturn({argument});",
                if vm { "if (false) { function exactReturn($v) {} }" } else { "" });
            // Container and object construction stay in the walker; both callers consume the same owners.
            let declaration = format!("{declaration} function exactCallback() {{}} $nominal = new stdClass(); $container = [1];");
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            staged_numeric_execution(&declaration, false, &mut context, &mut scope, &mut values).unwrap();
            let outcome = staged_numeric_execution(&source, vm, &mut context, &mut scope, &mut values);
            if accepted { assert!(outcome.is_ok(), "{source}, vm={vm}"); }
            else {
                assert!(matches!(outcome, Err(EvalStatus::UncaughtThrowable)), "{source}, vm={vm}");
                assert_eq!(values.get(scope.visible_cell("result").unwrap()), FakeValue::Int(23));
                let error = context.take_pending_throw().expect("return error owner");
                assert!(values.object_is_a(error, "TypeError", false).unwrap()); values.release(error).unwrap();
            }
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for (id, count) in &values.cell_owners { assert_eq!(*count, 0, "{source}, vm={vm}: {:?}", values.values[id]); }
        }
    }
}
