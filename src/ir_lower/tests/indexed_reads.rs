//! Purpose:
//! Verifies checked native array, hash and string reads without changing intentional probes.
//!
//! Called from:
//! - Focused staged strict-indexing tests.
//!
//! Key details:
//! - Indexing remains gated publicly until boxed and interpreter consumers are integrated.

use crate::{codegen_support::platform::Target, ir::{Immediate, Op}, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/indexed_reads.php");
const EXPECTED: &str = "PBH:NULL:11:4|c:a|SN|q:q:q:i:e|quiet|D|rkKfXF:2|WZ";

/// Ordinary reads carry checked metadata on every target and quiet reads stay separate.
#[test]
fn indexed_native_reads_emit_on_every_target() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Indexing, true);
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(target).unwrap();
        let module = super::semantic_sources::lower_capturing_arguments(SOURCE, target, profile, false, SourceMode::Php);
        let checked: Vec<_> = module.functions.iter().flat_map(|function| &function.instructions)
            .filter(|inst| inst.immediate == Some(Immediate::CheckedElementRead)).collect();
        for op in [Op::ArrayGet, Op::HashGet, Op::StrCharAt, Op::RuntimeCall, Op::LoadArrayElemRefCellExisting] {
            assert!(checked.iter().any(|inst| inst.op == op), "{target:?}: {op:?}");
        }
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let runtime = crate::codegen_support::generate_runtime_with_semantics(8_388_608,
            target, module.required_runtime_features, false, false, profile);
        assert!(runtime.contains("__rt_throw_missing_array_key:"));
        assert!(runtime.contains("__rt_array_get_mixed_key_hash_missing_unchecked:"));
        let implicit = super::semantic_sources::lower_capturing_arguments(
            "<?php $a=[1]; try { echo $a[$argc+8]; } catch(Throwable $e) { echo get_class($e); }",
            target, profile, false, SourceMode::Php);
        let asm = crate::codegen::generate_user_asm_from_ir(&implicit, false, false).unwrap();
        assert!(asm.contains("OutOfBoundsException"), "{target:?}: implicit exception class metadata");
    }
}

/// Checked reads preserve initialized null, byte offsets, probes, effects and temporary owners.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn indexed_native_reads_values_and_owners() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Indexing, true);
    for optimize in [false, true] {
        for mode in [SourceMode::Php, SourceMode::Lfc] {
            let module = super::semantic_sources::lower_capturing_arguments(SOURCE, Target::detect_host(), profile, optimize, mode);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "{optimize} {mode:?}: {stderr}");
            assert_eq!(String::from_utf8_lossy(&output.stdout), EXPECTED, "{optimize} {mode:?}: {stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
            assert!(!stderr.contains("Warning:"), "{stderr}");
        }
    }
}

/// Metadata cannot erase the profile requirement, throwing effects or ordinary-read restriction.
#[test]
fn indexed_native_reads_validate_policy() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Indexing, true);
    let original = super::semantic_sources::lower_capturing_arguments(SOURCE, Target::detect_host(), profile, false, SourceMode::Php);
    for case in 0..3 {
        let mut module = original.clone();
        let inst = module.functions.iter_mut().flat_map(|function| &mut function.instructions)
            .find(|inst| inst.immediate == Some(Immediate::CheckedElementRead)).unwrap();
        match case {
            0 => inst.effects.remove(crate::ir::Effects::MAY_THROW),
            1 => inst.op = Op::ArrayGetSilent,
            _ => module.semantic_profile = SemanticProfile::PERMISSIVE,
        }
        assert!(crate::ir::validate_module(&module).is_err(), "case={case}");
    }
}

/// Trusted generated code and the permissive profile retain their existing lookup policy.
#[test]
fn indexed_native_reads_keep_source_policy() {
    for (profile, mode) in [(SemanticProfile::PERMISSIVE, SourceMode::Php),
        (SemanticProfile::PERMISSIVE.with(StrictFeature::Indexing, true), SourceMode::Internal)] {
        let module = super::semantic_sources::lower_capturing_arguments(SOURCE, Target::detect_host(), profile, false, mode);
        assert!(!module.functions.iter().flat_map(|function| &function.instructions)
            .any(|inst| inst.immediate == Some(Immediate::CheckedElementRead)));
    }
}

/// Runtime-created exceptions retain their class even without an explicit source class reference.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn indexed_native_reads_without_named_exception() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Indexing, true);
    let caught = "<?php $a=[1]; try { echo $a[$argc+8]; } catch(Throwable $error) { echo get_class($error), ':', $error->getMessage(); }";
    let module = super::semantic_sources::lower_capturing_arguments(caught, Target::detect_host(), profile, false, SourceMode::Php);
    let output = super::native_module::run(&module);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stdout={}; {stderr}", String::from_utf8_lossy(&output.stdout));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "OutOfBoundsException:--strict-indexing: missing array key", "{stderr}");
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    let module = super::semantic_sources::lower_capturing_arguments("<?php $a=[1]; echo $a[$argc+8]; echo 'bad';", Target::detect_host(), profile, true, SourceMode::Php);
    let output = super::native_module::run(&module);
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("bad"));
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(report.contains("OutOfBoundsException") && report.contains(elephc_semantics::indexing::MISSING_ARRAY_KEY),
        "status={:?}; stdout={report}; stderr={}", output.status, String::from_utf8_lossy(&output.stderr));
}

/// Eval presence callbacks report absence without throwing across Rust on every supported ABI.
#[test]
fn indexed_eval_presence_callbacks_emit_on_every_target() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Indexing, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let mut features = crate::codegen_support::RuntimeFeatures::none();
        features.eval_bridge = true;
        let runtime = crate::codegen_support::generate_runtime_with_semantics(8_388_608, target, features, false, false, profile);
        assert!(runtime.contains("__elephc_eval_value_array_get_checked:"), "{name}");
        assert!(runtime.contains("__rt_mixed_array_get_absent:"), "{name}");
        assert!(runtime.contains("__rt_array_get_mixed_key_empty_slot:"), "{name}");
        let bridge = runtime.split("__elephc_eval_value_array_get_checked:").nth(1).unwrap()
            .split("__elephc_eval_value_array_key_exists:").next().unwrap();
        assert!(bridge.contains("__rt_mixed_array_get"));
        assert_eq!(bridge.matches("__elephc_eval_key_normalize").count(), 1,
            "{name}: convert once before the reader consumes the completed tuple");
        assert!(!bridge.contains("__rt_throw_missing"), "{name}: Rust receives a presence result");
    }
}

/// Public checking admits the completed profile and preserves it through every target's EIR.
#[test]
fn indexed_public_checker_emits_on_every_target() {
    let source = "<?php $a=[1];try{echo $a[$argc+9];}catch(Throwable $error){echo 'X';}";
    let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Indexing, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::check_with_target_and_options(&program, target,
            crate::types::CheckOptions { semantic_profile: profile, ..Default::default() },
            &crate::generics::GenericContext::default()).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        assert_eq!(module.semantic_profile, profile);
        assert!(module.functions.iter().flat_map(|function| &function.instructions)
            .any(|inst| inst.immediate == Some(Immediate::CheckedElementRead)));
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
