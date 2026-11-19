//! Purpose:
//! Verifies checked dimension reads, intentional probes and ownership in eval.
//!
//! Called from:
//! - Focused staged Indexing interpreter regressions.
//!
//! Key details:
//! - Array bytecode is not yet implemented, so its dispatcher deliberately falls back to the walker.

use super::super::*;
use super::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Executes the ordinary walker, optionally verifying the real bytecode fallback decision first.
fn execute(source: &str, fallback: bool, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut FakeOps,
) -> Result<EvalControl, EvalStatus> {
    let program = crate::parser::parse_fragment_with_semantics(source.as_bytes(), context.semantic_profile()).unwrap();
    if fallback { assert!(bytecode::try_execute(context, &program, scope, values)?.is_none()); }
    execute_statements(program.statements(), context, scope, values)
}

/// Retires frame, persistent and pending error leases before checking every fake allocation.
fn finish(context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut FakeOps, source: &str) {
    if let Some(error) = context.take_pending_throw() { values.release(error).unwrap(); }
    for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
    for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
    for (id, owners) in &values.cell_owners {
        assert_eq!(*owners, 0, "{source}: owner {id}: {:?}", values.values[id]);
    }
}

/// Real absence throws while initialized null, scalar zero and string bytes remain present.
#[test]
fn strict_indexing_eval_reads_preserve_presence_and_owners() {
    for fallback in [false, true] {
        for (source, expected) in [
            ("$a=[null];return $a[0];", Some(FakeValue::Null)),
            ("$a=[0];return $a[0];", Some(FakeValue::Int(0))),
            ("$a=['x'=>null];return $a['x'];", Some(FakeValue::Null)),
            ("$a=['x'=>3];return $a['x'];", Some(FakeValue::Int(3))),
            ("$a='abc';return $a[-1];", Some(FakeValue::String("c".to_string()))),
            ("$a='é';return $a[-1];", Some(FakeValue::Bytes(vec![0xa9]))),
            ("$a=[1];return $a[5];", None),
            ("$a=['x'=>null];return $a['absent'];", None),
            ("$a='abc';return $a[3];", None),
            ("$a='abc';return $a[-4];", None),
        ] {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Indexing, true)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            let result = execute(source, fallback, &mut context, &mut scope, &mut values);
            match expected {
                Some(expected) => {
                    let EvalControl::Return(value) = result.unwrap() else { panic!("return"); };
                    assert_eq!(values.get(value), expected, "{source}"); values.release(value).unwrap();
                }
                None => {
                    assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)), "{source}: {:?}", result.as_ref().err());
                    let error = context.take_pending_throw().unwrap();
                    assert!(values.object_is_a(error, "OutOfBoundsException", false).unwrap(), "{source}");
                    values.release(error).unwrap();
                }
            }
            finish(&mut context, &mut scope, &mut values, source);
        }
    }
}

/// Quiet roots and nested probes retain absence while key expressions remain ordinary reads.
#[test]
fn strict_indexing_eval_probes_and_key_errors() {
    for fallback in [false, true] {
        for (source, expected) in [
            ("$a=[1];return $a[5]??'q';", Some(FakeValue::String("q".to_string()))),
            ("return $unknown['x'][5]??'q';", Some(FakeValue::String("q".to_string()))),
            ("$a=[1];return isset($a[5]);", Some(FakeValue::Bool(false))),
            ("$a=[1];return empty($a[5]);", Some(FakeValue::Bool(true))),
            ("$a='abc';return $a[8]??'q';", Some(FakeValue::String("q".to_string()))),
            ("$a=[1];return $a[([1][5])]??'q';", None),
        ] {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Indexing, true)).unwrap();
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

/// Explicit slot creation and permissive missing reads retain their existing behavior.
#[test]
fn strict_indexing_eval_creation_and_permissive_controls() {
    for strict in [false, true] {
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Indexing, strict)).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        let source = "$a=[];$a['outer']['inner']=7;return $a['outer']['inner'];";
        let EvalControl::Return(value) = execute(source, true, &mut context, &mut scope, &mut values).unwrap() else { panic!("return"); };
        assert_eq!(values.get(value), FakeValue::Int(7)); values.release(value).unwrap();
        finish(&mut context, &mut scope, &mut values, source);
    }
    let mut context = ElephcEvalContext::new(); let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
    let source = "$a=[1];return $a[5];";
    let EvalControl::Return(value) = execute(source, false, &mut context, &mut scope, &mut values).unwrap() else { panic!("return"); };
    assert_eq!(values.get(value), FakeValue::Null); values.release(value).unwrap();
    finish(&mut context, &mut scope, &mut values, source);
}

/// Presence checks preserve independently selected exact-key and initialized-variable guarantees.
#[test]
fn strict_indexing_eval_feature_policies_remain_independent() {
    for indexing in [false, true] {
        for exact in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Indexing, indexing)
                .with(StrictFeature::ArrayKeys, exact);
            let mut context = ElephcEvalContext::new(); context.install_semantic_profile(profile).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            values.fixed_profile = Some(profile);
            let source = "$a=['1'=>7];return $a[1];";
            let result = execute(source, true, &mut context, &mut scope, &mut values);
            if indexing && exact {
                assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)));
                let error = context.take_pending_throw().unwrap();
                assert!(values.object_is_a(error, "OutOfBoundsException", false).unwrap());
                values.release(error).unwrap();
            } else {
                let EvalControl::Return(value) = result.unwrap() else { panic!("return"); };
                assert_eq!(values.get(value), if exact { FakeValue::Null } else { FakeValue::Int(7) });
                values.release(value).unwrap();
            }
            finish(&mut context, &mut scope, &mut values, source);
        }
    }
    for initialized in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Indexing, true)
            .with(StrictFeature::Initialization, initialized);
        let mut context = ElephcEvalContext::new(); context.install_semantic_profile(profile).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        let source = "$a=[1];return $a[$missing]??'q';";
        let result = execute(source, true, &mut context, &mut scope, &mut values);
        if initialized {
            assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)));
            let error = context.take_pending_throw().unwrap();
            assert!(values.object_is_a(error, "Error", false).unwrap()); values.release(error).unwrap();
        } else {
            let EvalControl::Return(value) = result.unwrap() else { panic!("return"); };
            assert_eq!(values.get(value), FakeValue::String("q".to_string())); values.release(value).unwrap();
        }
        assert_eq!(values.warnings.len(), usize::from(!initialized));
        finish(&mut context, &mut scope, &mut values, source);
    }
}

/// The opaque native fixture must parse under the same selected profile before execution.
#[test]
fn strict_indexing_opaque_fixture_parse_contract() {
    let source = format!("{}{}",
        include_str!("../../../../../tests/fixtures/strict/indexed_eval_reads.php").strip_prefix("<?php").unwrap(),
        include_str!("../../../../../tests/fixtures/strict/indexed_eval_edge_reads.php").strip_prefix("<?php declare(strict_types=0);").unwrap());
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Indexing, true);
    let result = crate::parser::parse_fragment_with_semantics(source.as_bytes(), profile);
    assert!(result.is_ok(), "{:?}", result.err());
}

/// Public interpreter entries preserve the guarantee across permissive and strict cache warmups.
#[test]
fn strict_indexing_public_entries_preserve_profiles_and_owners() {
    for walker in [false, true] {
        for strict in [false, true, false] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Indexing, strict);
            let source = b"$indexingCacheArray=[1];return $indexingCacheArray[8];";
            let program = crate::parser::parse_fragment_with_semantics(source, profile).unwrap();
            let mut context = ElephcEvalContext::new(); context.install_semantic_profile(profile).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            values.fixed_profile = Some(profile);
            let result = with_executable_cache_key(source, || if walker {
                execute_program_statements(&program, &mut context, &mut scope, &mut values)
                    .and_then(|control| match control { EvalControl::Return(value) => Ok(value), _ => values.null() })
            } else { execute_program_with_context(&mut context, &program, &mut scope, &mut values) });
            if !walker {
                assert!(bytecode::lookup_executable(source, profile).expect("cached fallback").is_err());
                assert!(!bytecode::last_execution_used_vm());
            }
            if strict {
                assert_eq!(result, Err(EvalStatus::UncaughtThrowable));
                let error = context.take_pending_throw().unwrap();
                assert!(values.object_is_a(error, "OutOfBoundsException", false).unwrap()); values.release(error).unwrap();
            } else { assert_eq!(values.get(result.as_ref().copied().unwrap()), FakeValue::Null); values.release(result.unwrap()).unwrap(); }
            finish(&mut context, &mut scope, &mut values, "public profile/cache");
        }
    }
}
