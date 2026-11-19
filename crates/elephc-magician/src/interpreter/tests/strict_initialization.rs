//! Purpose:
//! Checks initialized-variable policy and owner cleanup in both eval consumers.
//!
//! Called from:
//! - Focused staged Initialization parity tests.
//!
//! Key details:
//! - Public profile admission remains separate from these semantic implementation tests.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Releases every scope and persistent lease after a result or pending error has been retired.
fn assert_clean(context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut FakeOps, label: &str) {
    for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
    for value in context.take_persistent_cell_owners() { values.release(value).unwrap(); }
    for (id, owners) in &values.cell_owners {
        assert_eq!(*owners, 0, "{label}: retained owner {id}: {:?}", values.values[id]);
    }
}

/// Walker and VM distinguish absence from initialized null under either profile.
#[test]
fn strict_initialization_eval_missing_reads_and_owners() {
    for vm in [false, true] {
        for strict in [false, true] {
            for source in ["return $absent;", "$absent=null;return $absent;", "$absent=false;return $absent;"] {
                let mut context = ElephcEvalContext::new();
                context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, strict)).unwrap();
                let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
                let result = staged_numeric_execution(source, vm, &mut context, &mut scope, &mut values);
                let missing = source.starts_with("return");
                if strict && missing {
                    assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)), "vm={vm}: {:?}", result.as_ref().err());
                    let error = context.take_pending_throw().expect("missing read Error");
                    assert!(values.object_is_a(error, "Error", false).unwrap());
                    let FakeValue::Object(fields) = values.get(error) else { panic!("Error fields"); };
                    let message = fields.iter().find(|(name, _)| name == "message").unwrap().1;
                    assert_eq!(values.get(message), FakeValue::String(elephc_semantics::initialization::variable_message("absent")));
                    values.release(error).unwrap();
                } else {
                    let EvalControl::Return(value) = result.unwrap() else { panic!("returned value"); };
                    assert_eq!(values.get(value), if source.contains("false") { FakeValue::Bool(false) } else { FakeValue::Null });
                    values.release(value).unwrap();
                }
                assert_eq!(values.warnings.len(), usize::from(missing && !strict));
                assert_clean(&mut context, &mut scope, &mut values, "missing/frame");
            }
        }
    }
}

/// Catchable missing reads, detached captures and quiet probes share native fixture behavior.
#[test]
fn strict_initialization_eval_frame_fixture_and_owners() {
    let source = include_str!("../../../../../tests/fixtures/strict/initialized_frame_reads.php").strip_prefix("<?php").unwrap();
    for dispatch in [false, true] {
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true)).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        assert!(context.set_global_scope(&mut scope));
        if dispatch {
            let program = crate::parser::parse_fragment_with_semantics(source.as_bytes(), context.semantic_profile()).unwrap();
            let result = bytecode::try_execute(&mut context, &program, &mut scope, &mut values)
                .and_then(|control| match control {
                    Some(control) => Ok(control),
                    None => execute_statements(program.statements(), &mut context, &mut scope, &mut values),
                })
                .unwrap_or_else(|error| panic!("dispatch={dispatch}: {error:?}; output={}", values.output));
            assert!(matches!(result, EvalControl::None | EvalControl::ReturnVoid));
        } else {
            staged_numeric_execution(source, false, &mut context, &mut scope, &mut values)
                .unwrap_or_else(|error| panic!("dispatch={dispatch}: {error:?}; output={}", values.output));
        }
        assert_eq!(values.output, "P:7:7|A:missing:I|NULL:fallback|AE:fallback|NULL|integer|3|0|2|ready|A:none|P:5|0:0|1:1|9|I|9|4|I|4|I|I|P|6:8|boolean:double:string:3|");
        assert!(values.warnings.is_empty(), "{:?}", values.warnings);
        assert_clean(&mut context, &mut scope, &mut values, "missing/frame");
    }
}

/// Persistent bindings, initialized null properties and absent reads share the native oracle.
#[test]
fn strict_initialization_eval_scope_fixture_and_owners() {
    let source = include_str!("../../../../../tests/fixtures/strict/initialized_scope_reads.php").strip_prefix("<?php").unwrap();
    let split = source.find("class InitializationProperties").unwrap();
    let nested = source.find("function initializedNested").unwrap();
    let keys = source.find("function initializedQuietKey").unwrap();
    for (label, source, expected) in [
        ("persistent bindings", &source[..nested], "3|I|3|4|I|4|A:missing|I|5|"),
        ("nested array probes", &source[nested..keys], "AE:missing|I|PF:7|7|"),
        ("ordinary probe keys", &source[keys..split], "KA|KE|Kmissing|I|"),
        ("binding/key prefix", &source[..split], "3|I|3|4|I|4|A:missing|I|5|AE:missing|I|PF:7|7|KA|KE|Kmissing|I|"),
        ("property suffix", &source[split..], "NULL|I|I|AE:missing|9:9|PF:3:3|G6|A|E|"),
        ("complete scope", source, "3|I|3|4|I|4|A:missing|I|5|AE:missing|I|PF:7|7|KA|KE|Kmissing|I|NULL|I|I|AE:missing|9:9|PF:3:3|G6|A|E|"),
    ] {
        for dispatch in [false, true] {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            assert!(context.set_global_scope(&mut scope));
            let program = crate::parser::parse_fragment_with_semantics(format!("$argc=1;{source}").as_bytes(), context.semantic_profile()).unwrap();
            let executed = if dispatch { bytecode::try_execute(&mut context, &program, &mut scope, &mut values) }
                else { Ok(None) };
            let result = executed.and_then(|control| match control {
                Some(control) => Ok(control),
                None => execute_statements(program.statements(), &mut context, &mut scope, &mut values),
            }).unwrap_or_else(|error| panic!("dispatch={dispatch}: {error:?}; output={}", values.output));
            assert!(matches!(result, EvalControl::None | EvalControl::ReturnVoid));
            assert_eq!(values.output, expected, "{label}, dispatch={dispatch}");
            assert!(values.warnings.is_empty(), "{:?}", values.warnings);
            assert_clean(&mut context, &mut scope, &mut values, label);
        }
    }
}

/// Dynamic reads and canonical removal keep old aliases and retire throwing old owners.
#[test]
fn strict_initialization_eval_extended_fixture_and_owners() {
    let source = include_str!("../../../../../tests/fixtures/strict/initialized_extended_reads.php").strip_prefix("<?php").unwrap();
    for dispatch in [false, true] {
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true)).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        assert!(context.set_global_scope(&mut scope));
        let program = crate::parser::parse_fragment_with_semantics(format!("$argc=1;{source}").as_bytes(), context.semantic_profile()).unwrap();
        let result = if dispatch { bytecode::try_execute(&mut context, &program, &mut scope, &mut values) }
            else { Ok(None) };
        let result = result.and_then(|control| match control {
            Some(control) => Ok(control),
            None => execute_statements(program.statements(), &mut context, &mut scope, &mut values),
        }).unwrap_or_else(|error| panic!("dispatch={dispatch}: {error:?}; output={}", values.output));
        assert!(matches!(result, EvalControl::None | EvalControl::ReturnVoid));
        assert_eq!(values.output, "A:missing|I|old:new|I|AE:missing|I|I:A|I|D:I|", "dispatch={dispatch}");
        assert!(values.warnings.is_empty(), "{:?}", values.warnings);
        assert_clean(&mut context, &mut scope, &mut values, "extended reads");
    }
}

/// Dynamic containers and physical reflection retain initialized-read parity.
#[test]
fn strict_initialization_eval_object_fixture_and_owners() {
    let fixture = include_str!("../../../../../tests/fixtures/strict/initialized_object_reads.php").strip_prefix("<?php").unwrap();
    for locals in [false, true] {
        for dynamic_reflection in [false, true] {
            let source = if dynamic_reflection {
                fixture.replace("RuntimeInitializedReflection::class, 'value'", "RuntimeInitializedReflection::class, $name")
            } else { fixture.to_string() };
            for dispatch in [false, true] {
                let mut context = ElephcEvalContext::new();
                context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true)
                    .with(StrictFeature::Locals, locals)).unwrap();
                let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
                assert!(context.set_global_scope(&mut scope));
                let program = crate::parser::parse_fragment_with_semantics(format!("$argc=1;{source}").as_bytes(), context.semantic_profile()).unwrap();
                let result = if dispatch { bytecode::try_execute(&mut context, &program, &mut scope, &mut values) }
                    else { Ok(None) };
                let result = result.and_then(|control| match control {
                    Some(control) => Ok(control),
                    None => execute_statements(program.statements(), &mut context, &mut scope, &mut values),
                }).unwrap_or_else(|error| panic!("dispatch={dispatch}: {error:?}; output={}", values.output));
                assert!(matches!(result, EvalControl::None | EvalControl::ReturnVoid));
                assert_eq!(values.output, "NULL|I|I|AE:missing|I|I:A|I|I|N|", "locals={locals}, dispatch={dispatch}");
                assert!(values.warnings.is_empty(), "{:?}", values.warnings);
                assert_clean(&mut context, &mut scope, &mut values, "object reads");
            }
        }
    }
}

/// Runs a shared initialized-read fixture through both internal entries and stable-local profiles.
fn assert_initialized_fixture(source: &str, expected: &str, label: &str) {
    let source = source.strip_prefix("<?php").unwrap();
    for locals in [false, true] {
        for dispatch in [false, true] {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true)
                .with(StrictFeature::Locals, locals)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            assert!(context.set_global_scope(&mut scope));
            let program = crate::parser::parse_fragment_with_semantics(format!("$argc=1;{source}").as_bytes(), context.semantic_profile()).unwrap();
            let result = if dispatch { bytecode::try_execute(&mut context, &program, &mut scope, &mut values) }
                else { Ok(None) };
            let result = result.and_then(|control| match control {
                Some(control) => Ok(control),
                None => execute_statements(program.statements(), &mut context, &mut scope, &mut values),
            }).unwrap_or_else(|error| panic!("{label}, locals={locals}, dispatch={dispatch}: {error:?}; output={}", values.output));
            assert!(matches!(result, EvalControl::None | EvalControl::ReturnVoid));
            assert_eq!(values.output, expected, "{label}, locals={locals}, dispatch={dispatch}");
            assert!(values.warnings.is_empty(), "{:?}", values.warnings);
            assert_clean(&mut context, &mut scope, &mut values, label);
        }
    }
}

/// Mixed parameters and union views preserve initialized-null and missing-property distinctions.
#[test]
fn strict_initialization_eval_boxed_fixture_and_owners() {
    assert_initialized_fixture(include_str!("../../../../../tests/fixtures/strict/initialized_boxed_reads.php"),
        "NULL:NULL|I|I|AE:missing|AE:missing|I|I|I|NULL|I|AE:missing|I|I|", "boxed reads");
}

/// Known and boxed runtime-name accessors preserve initialized-slot and quiet-probe semantics.
#[test]
fn strict_initialization_eval_magic_fixture_and_owners() {
    assert_initialized_fixture(include_str!("../../../../../tests/fixtures/strict/initialized_magic_reads.php"),
        "ready:1|PF:ready:3|ready:1|PF:ready|3|NULL\n0|ready:4|T|null:null|ready:ready|6|G7|AE:G7|child:1|", "magic reads");
}

/// Public walker and VM entries enforce initialization after a permissive cache warmup.
#[test]
fn strict_initialization_public_entries_preserve_profiles_and_owners() {
    for walker in [false, true] {
        for strict in [false, true, false] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, strict);
            let program = crate::parser::parse_fragment_with_semantics(b"return $publicAbsent;", profile).unwrap();
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(profile).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            let result = if walker {
                execute_program_statements(&program, &mut context, &mut scope, &mut values)
                    .and_then(|control| match control { EvalControl::Return(value) => Ok(value), _ => values.null() })
            } else { execute_program_with_context(&mut context, &program, &mut scope, &mut values) };
            if strict {
                assert_eq!(result, Err(EvalStatus::UncaughtThrowable));
                let error = context.take_pending_throw().expect("initialized read Error");
                assert!(values.object_is_a(error, "Error", false).unwrap());
                values.release(error).unwrap();
            } else { values.release(result.unwrap()).unwrap(); }
            assert_eq!(values.warnings.len(), usize::from(!strict));
            assert_clean(&mut context, &mut scope, &mut values, "public profile/cache");
        }
    }
}

/// Both interpreter entries suppress same-pair getter reentry and retire its guard on errors.
#[test]
fn strict_initialization_recursive_getters_and_owners() {
    assert_initialized_fixture(include_str!("../../../../../tests/fixtures/strict/initialized_recursive_getters.php"),
        "E:1|ready:2|right:2|text:2.5|other:1:1|value|", "recursive getters");
}
