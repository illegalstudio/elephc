//! Purpose:
//! Verifies interpreted non-null dimension receivers and independently quiet probe roots.
//!
//! Called from:
//! - Focused staged strict-nullability interpreter tests.
//!
//! Key details:
//! - Explicit dispatcher fallback is checked without weakening public admission gates.

use super::super::*;
use super::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Runs staged syntax through the walker and optionally checks the real bytecode fallback.
fn execute(source: &str, fallback: bool, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut FakeOps,
) -> Result<EvalControl, EvalStatus> {
    let program = crate::parser::parse_fragment_with_semantics(source.as_bytes(), context.semantic_profile()).unwrap();
    if fallback { assert!(bytecode::try_execute(context, &program, scope, values)?.is_none()); }
    execute_statements(program.statements(), context, scope, values)
}

/// Retires all fixture leases, including a pending semantic error, before the owner audit.
fn finish(context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut FakeOps, source: &str) {
    if let Some(error) = context.take_pending_throw() { values.release(error).unwrap(); }
    for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
    for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
    for (id, owners) in &values.cell_owners {
        assert_eq!(*owners, 0, "{source}: owner {id}: {:?}", values.values[id]);
    }
}

/// Null receivers fail after selectors run, with borrowed and newly produced owners balanced.
#[test]
fn strict_nullability_eval_dimension_receivers_and_owners() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for fallback in [false, true] {
        for (source, expected, stdout) in [
            ("$a=null;return $a[0];", None, ""),
            ("$a=[null];return $a[0][0];", None, ""),
            ("function nullableEvalKey(){echo 'K';return 'owned-key';}$a=null;return $a[nullableEvalKey()];", None, "K"),
            ("function nullableEvalReceiver(){return [null][0];}return nullableEvalReceiver()['owned-key'];", None, ""),
            ("$a=[null];return $a[0];", Some(FakeValue::Null), ""),
            ("$a=[7];return $a[0];", Some(FakeValue::Int(7)), ""),
        ] {
            let mut context = ElephcEvalContext::new(); context.install_semantic_profile(profile).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            let result = execute(source, fallback, &mut context, &mut scope, &mut values);
            if let Some(expected) = expected {
                let EvalControl::Return(value) = result.unwrap() else { panic!("return"); };
                assert_eq!(values.get(value), expected, "{source}"); values.release(value).unwrap();
            } else {
                assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)), "{source}: {:?}", result.as_ref().err());
                let error = context.take_pending_throw().unwrap();
                assert!(values.object_is_a(error, "TypeError", false).unwrap(), "{source}");
                let FakeValue::Object(properties) = values.get(error) else { panic!("throwable object"); };
                let message = properties.iter().find(|(name, _)| name == "message").unwrap().1;
                assert_eq!(values.get(message), FakeValue::String(
                    elephc_semantics::nullability::NON_NULL_MESSAGE.into()), "{source}");
                values.release(error).unwrap();
            }
            assert_eq!(values.output, stdout, "{source}");
            assert!(values.warnings.is_empty(), "{source}: {:?}", values.warnings);
            finish(&mut context, &mut scope, &mut values, source);
        }
    }
}

/// Only receiver spines are quiet; selectors retain ordinary null receiver checks.
#[test]
fn strict_nullability_eval_quiet_spines_and_selectors() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for fallback in [false, true] {
        for (source, expected) in [
            ("$a=null;return $a[0][0]??'q';", Some(FakeValue::String("q".into()))),
            ("return $unknown[0][0]??'q';", Some(FakeValue::String("q".into()))),
            ("$a=null;return isset($a[0][0]);", Some(FakeValue::Bool(false))),
            ("$a=null;return empty($a[0][0]);", Some(FakeValue::Bool(true))),
            ("$a=null;$outer=[];return isset($outer[$a[0]]);", None),
            ("$a=null;$outer=[];return $outer[$a[0]]??'q';", None),
        ] {
            let mut context = ElephcEvalContext::new(); context.install_semantic_profile(profile).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            let result = execute(source, fallback, &mut context, &mut scope, &mut values);
            if let Some(expected) = expected {
                let EvalControl::Return(value) = result.unwrap() else { panic!("return"); };
                assert_eq!(values.get(value), expected, "{source}"); values.release(value).unwrap();
            } else { assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)), "{source}"); }
            assert!(values.warnings.is_empty(), "{source}: {:?}", values.warnings);
            finish(&mut context, &mut scope, &mut values, source);
        }
    }
}

/// Slot creation, other strict rules and permissive reads retain their independent policies.
#[test]
fn strict_nullability_eval_creation_and_independent_controls() {
    for strict in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, strict);
        for source in ["$a=null;$a['outer']['inner']=7;return $a['outer']['inner'];",
            "$a=['existing'=>0];function fill(&$slot){$slot=7;}fill($a['new']);return $a['new'];"] {
            let mut context = ElephcEvalContext::new(); context.install_semantic_profile(profile).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            let EvalControl::Return(value) = execute(source, true, &mut context, &mut scope, &mut values).unwrap() else { panic!("return"); };
            assert_eq!(values.get(value), FakeValue::Int(7), "{source}"); values.release(value).unwrap();
            finish(&mut context, &mut scope, &mut values, source);
        }
    }
    for feature in [None, Some(StrictFeature::Indexing), Some(StrictFeature::Initialization)] {
        let profile = feature.map_or(SemanticProfile::PERMISSIVE, |feature| SemanticProfile::PERMISSIVE.with(feature, true));
        let mut context = ElephcEvalContext::new(); context.install_semantic_profile(profile).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        let source = "$a=null;return $a[0];";
        let EvalControl::Return(value) = execute(source, true, &mut context, &mut scope, &mut values).unwrap() else { panic!("return"); };
        assert_eq!(values.get(value), FakeValue::Null); values.release(value).unwrap();
        finish(&mut context, &mut scope, &mut values, source);
    }
}

/// Property reads share native nullability while quiet roots and computed names stay distinct.
#[test]
fn strict_nullability_eval_property_receivers_and_owners() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    let fixture = include_str!("../../../../../tests/fixtures/strict/nullable_properties.php").strip_prefix("<?php\ndeclare(strict_types=0);").unwrap();
    for fallback in [false, true] {
        let mut context = ElephcEvalContext::new(); context.install_semantic_profile(profile).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        let result = execute(fixture, fallback, &mut context, &mut scope, &mut values);
        assert!(result.is_ok(), "{:?}; {}", result.as_ref().err(), values.output);
        assert_eq!(values.output, "P|8|KN|q:0:1:q|q|CN|O|Q");
        assert!(values.warnings.is_empty(), "{:?}", values.warnings);
        finish(&mut context, &mut scope, &mut values, "property receivers");
    }
}

/// Keeps the shared native/eval receiver fixture inside the fragment parser's supported syntax.
#[test]
fn strict_nullability_eval_property_fixture_parse_contract() {
    let source = include_str!("../../../../../tests/fixtures/strict/nullable_properties.php").strip_prefix("<?php\ndeclare(strict_types=0);").unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for (index, line) in source.lines().enumerate() {
        let result = crate::parser::parse_fragment_with_semantics(line.as_bytes(), profile);
        assert!(result.is_ok(), "line {}: {line}: {:?}", index + 1, result.err());
    }
    let result = crate::parser::parse_fragment_with_semantics(source.as_bytes(), profile);
    assert!(result.is_ok(), "{:?}", result.err());
}

/// A skipped hop suppresses selectors, while a real null returned by a successful hop stays visible.
#[test]
fn strict_nullability_eval_nullsafe_chains_preserve_steps() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for fallback in [false, true] {
        for (source, expected, stdout) in [
            ("$gap=[];$n=null;return $n?->child->value;", Some(FakeValue::Null), ""),
            ("$gap=[];function nullableChainName(){echo 'bad';return 'child';}$n=null;return $n?->{nullableChainName()}->value;", Some(FakeValue::Null), ""),
            ("$gap=[];class NullableChainNode{public mixed $child=null;}$n=new NullableChainNode();return $n?->child->value;", None, ""),
            ("$gap=[];function nullableChainKey(){echo 'K';return 0;}class NullableChainNode{public mixed $child=null;}$n=new NullableChainNode();return $n?->child[nullableChainKey()];", None, "K"),
            ("$gap=[];class NullableChainNode{public mixed $child=null;}$n=new NullableChainNode();return $n?->child->value??'q';", Some(FakeValue::String("q".into())), ""),
            ("$gap=[];class NullableChainNode{public mixed $child=null;}$n=new NullableChainNode();return empty($n?->child->value);", Some(FakeValue::Bool(true)), ""),
        ] {
            let mut context = ElephcEvalContext::new(); context.install_semantic_profile(profile).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            let result = execute(source, fallback, &mut context, &mut scope, &mut values);
            if let Some(expected) = expected {
                let EvalControl::Return(value) = result.unwrap() else { panic!("return"); };
                assert_eq!(values.get(value), expected, "{source}"); values.release(value).unwrap();
            } else { assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)), "{source}: {:?}", result.as_ref().err()); }
            assert_eq!(values.output, stdout, "{source}");
            finish(&mut context, &mut scope, &mut values, source);
        }
    }
    for strict in [false, true] {
        let mut context = ElephcEvalContext::new(); context.install_semantic_profile(
            SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, strict)).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        let source = "$gap=[];function nullableEmptyName(){echo 'bad';return 'child';}$n=null;return empty($n?->{nullableEmptyName()}->value);";
        let EvalControl::Return(value) = execute(source, true, &mut context, &mut scope, &mut values).unwrap() else { panic!("return"); };
        assert_eq!(values.get(value), FakeValue::Bool(true)); assert!(values.output.is_empty());
        values.release(value).unwrap(); finish(&mut context, &mut scope, &mut values, source);
    }
}

/// Non-null declarations check the source before weak scalar conversion and preserve open contracts.
#[test]
fn strict_nullability_eval_declared_boundaries_and_owners() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    let source = include_str!("../../../../../tests/fixtures/strict/nullable_boundaries.php")
        .strip_prefix("<?php\ndeclare(strict_types=0);").unwrap();
    for fallback in [false, true] {
        let mut context = ElephcEvalContext::new(); context.install_semantic_profile(profile).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        let result = execute(source, fallback, &mut context, &mut scope, &mut values);
        assert!(result.is_ok(), "{:?}; {}", result.as_ref().err(), values.output);
        assert_eq!(values.output, "LP|R|S:kept|NULL:7:8:NULL:NULL");
        finish(&mut context, &mut scope, &mut values, "declared boundaries");
    }
}

/// A refused null reference candidate cannot replace the published property or attach a source alias.
#[test]
fn strict_nullability_eval_property_reference_preflight_preserves_storage() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    let source = "class NullableRefSlot{public string $value='kept';}$slot=new NullableRefSlot();$candidate=null;try{$slot->value=&$candidate;}catch(TypeError $e){echo 'N';}$candidate='changed';echo ':',$slot->value;";
    let mut context = ElephcEvalContext::new(); context.install_semantic_profile(profile).unwrap();
    let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
    let result = execute(source, true, &mut context, &mut scope, &mut values);
    assert!(result.is_ok(), "{:?}; {}", result.as_ref().err(), values.output);
    assert_eq!(values.output, "N:kept");
    finish(&mut context, &mut scope, &mut values, source);
}

/// Neutral builtin declarations reject only null while retaining nullable defaults and weak kinds.
#[test]
fn strict_nullability_eval_builtin_original_contracts_and_owners() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    let source = include_str!("../../../../../tests/fixtures/strict/nullable_eval_builtins.php")
        .strip_prefix("<?php\ndeclare(strict_types=0);").unwrap();
    for fallback in [false, true] {
        let mut context = ElephcEvalContext::new(); context.install_semantic_profile(profile).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        let result = execute(source, fallback, &mut context, &mut scope, &mut values);
        assert!(result.is_ok(), "{:?}; {}", result.as_ref().err(), values.output);
        assert_eq!(values.output, "DNSCAFLTJ|bc:1:NULL:0");
        finish(&mut context, &mut scope, &mut values, "builtin contracts");
    }
}
