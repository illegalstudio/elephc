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

/// User-sort callbacks retire each owned integer result in both legacy and checked consumers.
#[test]
fn strict_comparison_eval_comparator_owned_returns_are_retired() {
    for selected in [false, true] {
        for name in ["usort", "uasort", "uksort"] {
            for vm in [false, true] {
                let mut context = ElephcEvalContext::new();
                context.install_semantic_profile(SemanticProfile::PERMISSIVE
                    .with(StrictFeature::Comparisons, selected)).unwrap();
                let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
                let comparison = if name == "uksort" { "strcmp($left, $right)" } else { "$left <=> $right" };
                let setup = format!("$items = ['b' => 2, 'a' => 1]; \
                    function ownedIntegerComparator($left, $right): mixed {{ return {comparison}; }} \
                    function ownedIntegerSortProxy($items) {{ {name}($items, 'ownedIntegerComparator'); echo implode(',', $items); }}");
                staged_numeric_execution(&setup, false, &mut context, &mut scope, &mut values).unwrap();
                let source = if vm {
                    "if (false) { function ownedIntegerSortProxy($items) {} } \
                        ownedIntegerSortProxy($items);".to_string()
                } else { format!("{name}($items, 'ownedIntegerComparator'); echo implode(',', $items);") };
                staged_numeric_execution(&source, vm, &mut context, &mut scope, &mut values).unwrap();
                assert_eq!(values.output, "1,2", "{name}: selected={selected} vm={vm}");
                clean_sort_context(&mut context, &mut scope, &mut values, name);
            }
        }
    }
}

/// Explicit source comparators must return actual integers without hidden result conversion.
#[test]
fn strict_comparison_eval_comparator_results_require_integer_kind() {
    let fixture = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/strict/comparator_checked_results.php"));
    let fragment = fixture.split_once("$argc > 0 ? '").unwrap().1.split_once("' : '';").unwrap().0;
    crate::parser::parse_fragment_with_semantics(fragment.as_bytes(),
        SemanticProfile::PERMISSIVE.with(StrictFeature::Comparisons, true)).unwrap();
    for name in ["usort", "uasort", "uksort"] {
        for value in ["true", "1.5", "'1'", "[1]", "new stdClass()"] {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE
                .with(StrictFeature::Comparisons, true)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            let source = format!("function invalidIntegerComparator($left, $right): mixed {{ return {value}; }} \
                $items = ['b' => 2, 'a' => 1]; {name}($items, 'invalidIntegerComparator'); echo 'X';");
            let result = staged_numeric_execution(&source, false, &mut context, &mut scope, &mut values);
            assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)),
                "{name} returning {value}: {:?}", result.as_ref().err());
            assert!(values.output.is_empty(), "{name} returning {value}: {}", values.output);
            let error = context.take_pending_throw().expect("comparator result TypeError");
            assert!(values.object_is_a(error, "TypeError", false).unwrap());
            values.release(error).unwrap();
            clean_sort_context(&mut context, &mut scope, &mut values, name);
        }
    }
}

/// A failed replacement insertion retires callback, entry and unfinished-container leases.
#[test]
fn strict_comparison_eval_comparator_partial_results_are_retired() {
    for name in ["usort", "uasort", "uksort"] {
        for failed_insert in [0, 1] {
            let mut context = ElephcEvalContext::new();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            let setup = "function partialSortComparator($left, $right): mixed { return $left <=> $right; } \
                return ['b' => 2, 'a' => 1];";
            let EvalControl::Return(array) = staged_numeric_execution(setup, false,
                &mut context, &mut scope, &mut values).unwrap() else { panic!("source array"); };
            let callback = values.string("partialSortComparator").unwrap();
            values.fail_array_set_call(failed_insert);
            let result = eval_user_sort_replacement(name, array, callback, &mut context, &mut values);
            assert_eq!(result, Err(EvalStatus::UnsupportedConstruct), "{name}: insertion {failed_insert}");
            values.release(callback).unwrap(); values.release(array).unwrap();
            clean_sort_context(&mut context, &mut scope, &mut values, name);
        }
    }
}

/// Retires the test activation and checks every independent callback/container lease.
fn clean_sort_context(context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut FakeOps, name: &str) {
    for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
    for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
    for (id, owners) in &values.cell_owners {
        assert_eq!(*owners, 0, "{name}: {:?}", values.values[id]);
    }
}

/// Search leases balance in permissive mode, including misses and an escaping owned string key.
#[test]
fn strict_comparison_eval_search_permissive_ownership_controls() {
    for (source, expected) in [
        ("return in_array('1', $haystack);", FakeValue::Bool(true)),
        ("return array_search('1', $haystack, false);", FakeValue::Int(0)),
        ("return array_search('1', $haystack, true);", FakeValue::Int(1)),
        ("return array_search('absent', $haystack, true);", FakeValue::Bool(false)),
        ("return array_search('1', $map, true);", FakeValue::String("name".into())),
    ] {
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        staged_numeric_execution("$haystack = [1, '1']; $map = ['name' => '1'];", false,
            &mut context, &mut scope, &mut values).unwrap();
        let EvalControl::Return(value) = staged_numeric_execution(source, false,
            &mut context, &mut scope, &mut values).unwrap_or_else(|status| panic!("{source}: {status:?}"))
            else { panic!("missing return: {source}"); };
        assert_eq!(values.get(value), expected, "{source}"); values.release(value).unwrap();
        for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
        for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
        for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}: {:?}", values.values[id]); }
    }
}

/// Source/value builtin dispatch shares explicit modes without enabling unrelated coercion rules.
#[test]
fn strict_comparison_eval_builtin_modes_and_owner_contracts() {
    for (source, expected, vm) in [
        ("return in_array(1, $haystack, true);", Some(FakeValue::Bool(true)), false),
        ("return in_array(1, $haystack, true);", Some(FakeValue::Bool(true)), true),
        ("return array_search('1', $haystack, true);", Some(FakeValue::Int(1)), false),
        ("return in_array(strict: true, needle: 1, haystack: $haystack);", Some(FakeValue::Bool(true)), false),
        ("return in_array(...[1, $haystack, true]);", Some(FakeValue::Bool(true)), false),
        ("return call_user_func('in_array', 1, $haystack, true);", Some(FakeValue::Bool(true)), false),
        ("return in_array(1, $haystack, 1);", Some(FakeValue::Bool(true)), false),
        ("return in_array(1, $haystack);", None, false),
        ("return in_array(1, $haystack, false);", None, true),
        ("return in_array(strict: false, needle: 1, haystack: $haystack);", None, false),
        ("return array_search(...[1, $haystack, false]);", None, false),
        ("return call_user_func('in_array', 1, $haystack);", None, false),
        ("sort($haystack);", None, false),
        ("return array_unique($haystack);", None, false),
        ("return max(1, 2);", None, false),
    ] {
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Comparisons, true)).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        staged_numeric_execution("$haystack = [1, '1'];", false, &mut context, &mut scope, &mut values).unwrap();
        // Builtins are interpreter fallback calls, reached from a compiled user-function caller.
        let caller = if vm {
            let declaration = format!("function strictComparisonVmCall($haystack) {{ {source} }}");
            staged_numeric_execution(&declaration, false, &mut context, &mut scope, &mut values).unwrap();
            "if (false) { function strictComparisonVmCall($haystack) {} } return strictComparisonVmCall($haystack);"
        } else { source };
        let result = staged_numeric_execution(caller, vm, &mut context, &mut scope, &mut values);
        if let Some(expected) = expected {
            let EvalControl::Return(value) = result.unwrap_or_else(|status| panic!("{source}, vm={vm}: {status:?}"))
                else { panic!("missing return: {source}"); };
            assert_eq!(values.get(value), expected, "{source}, vm={vm}");
            values.release(value).unwrap();
        } else {
            assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)), "{source}, vm={vm}: {:?}", result.as_ref().err());
            let error = context.take_pending_throw().expect("comparison mode TypeError");
            assert!(values.object_is_a(error, "TypeError", false).unwrap());
            values.release(error).unwrap();
        }
        for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
        for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
        for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{source}, vm={vm}: {:?}", values.values[id]); }
    }
}

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
