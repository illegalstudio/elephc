//! Purpose:
//! Verifies shared comparison semantics and owner retirement in both eval engines.
//!
//! Called from:
//! - Focused staged comparison tests and parse-cache boundary controls.
//!
//! Key details:
//! - Public admission remains closed while builtin and native-class parity is integrated.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Both consumers preserve bytewise strings, exact scalar kinds and TypeError ownership.
#[test]
fn strict_comparison_eval_scalar_values_failures_and_owners() {
    for vm in [false, true] {
        for (source, expected) in [
            ("return '10' < '9';", Some(FakeValue::Bool(true))),
            ("return '10' <=> '9';", Some(FakeValue::Int(-1))),
            ("return '02' <=> '2';", Some(FakeValue::Int(-1))),
            ("return 2 <=> 3;", Some(FakeValue::Int(-1))),
            ("return 2.0 < 3.0;", Some(FakeValue::Bool(true))),
            ("return true <=> false;", Some(FakeValue::Int(1))),
            ("return null <=> null;", Some(FakeValue::Int(0))),
            ("return 1 === '1';", Some(FakeValue::Bool(false))),
            ("return 1 < 2.0;", None), ("return '1' < 2;", None),
            ("return true < 2;", None), ("return null < 1;", None),
            ("return $container < $container;", None),
        ] {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Comparisons, true)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            // Build the container in the walker; both engines execute the comparison.
            staged_numeric_execution("$container = [1];", false, &mut context, &mut scope, &mut values).unwrap();
            let outcome = staged_numeric_execution(source, vm, &mut context, &mut scope, &mut values);
            if let Some(expected) = expected {
                let EvalControl::Return(value) = outcome.unwrap_or_else(|status| panic!("{source}: {status:?}"))
                    else { panic!("missing return: {source}"); };
                assert_eq!(values.get(value), expected, "{source}, vm={vm}");
                values.release(value).unwrap();
            } else {
                assert!(matches!(outcome, Err(EvalStatus::UncaughtThrowable)), "{source}, vm={vm}");
                let error = context.take_pending_throw().expect("comparison error owner");
                assert!(values.object_is_a(error, "TypeError", false).unwrap()); values.release(error).unwrap();
            }
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
            for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}, vm={vm}: {:?}", values.values[id]); }
        }
    }
}

/// Source restrictions include unreachable expressions and keep permissive and literal-string controls.
#[test]
fn strict_comparison_eval_parse_rejects_loose_syntax() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Comparisons, true);
    for source in ["return 1 == 1;", "if (false) { return 1 != 2; }", "return 1 <> 2;"] {
        assert_eq!(crate::parser::parse_fragment_with_semantics(source.as_bytes(), profile).unwrap_err(),
            crate::errors::EvalParseError::StrictComparisonSyntax, "{source}");
        if source.contains("<>") { assert!(crate::parser::parse_fragment(source.as_bytes()).is_err()); }
        else { crate::parser::parse_fragment(source.as_bytes()).unwrap(); }
    }
    crate::parser::parse_fragment_with_semantics(b"return 'a == b';", profile).unwrap();
}

/// Exact switch matching keeps variable subjects live after labels and retires every temporary.
#[test]
fn strict_comparison_eval_switch_exact_live_subjects_and_owners() {
    for source in [
        "$value = '1'; switch ($value) { case 1: echo 'X'; break; case '1': echo 'S'; break; default: echo 'X'; }",
        "switch ('1') { case 1: echo 'X'; break; case '1': echo 'S'; break; default: echo 'X'; }",
        "$value = 0; switch ($value) { case null: echo 'X'; break; case false: echo 'X'; break; default: echo 'S'; }",
        "$value = 1; switch ($value) { case ($value = '2'): echo 'S'; break; default: echo 'X'; }",
    ] {
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Comparisons, true)).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        staged_numeric_execution(source, false, &mut context, &mut scope, &mut values)
            .unwrap_or_else(|status| panic!("{source}: {status:?}"));
        assert_eq!(values.output, "S", "{source}");
        for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
        for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
        for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}: {:?}", values.values[id]); }
    }
}
