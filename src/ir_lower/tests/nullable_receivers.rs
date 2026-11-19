//! Purpose:
//! Verifies independent null receiver guards and precise quiet access spines.
//!
//! Called from:
//! - Focused staged strict-nullability compiler tests.
//!
//! Key details:
//! - Public admission stays gated while remaining null consumers are integrated.

use crate::{codegen_support::platform::Target, ir::Op, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/nullable_receivers.php");
const PROPERTIES: &str = include_str!("../../../tests/fixtures/strict/nullable_properties.php");

/// Quiet receiver roots remain legal but ordinary reads in their selectors do not become probes.
#[test]
fn nullable_receiver_source_spines_survive_pre_fold_validation() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for source in ["<?php if (false) { echo null[0]; }",
        "<?php echo isset($a[null[0]]);", "<?php echo $a[null[0]] ?? 1;",
        "<?php eval('if (false) { echo null[0]; }');"] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        let error = crate::semantics::validate_source(&program, profile).unwrap_err();
        assert!(error.message.contains("strict-nullability"), "{source}: {error:?}");
        crate::semantics::validate_source(&program, SemanticProfile::PERMISSIVE).unwrap();
    }
    for source in ["<?php echo isset(null[0][1]);", "<?php echo null[0][1] ?? 1;",
        "<?php echo empty(null[0]);", "<?php echo null?->value;"] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        crate::semantics::validate_source(&program, profile).unwrap();
    }
    for source in ["<?php $value = null; echo $value[0];",
        "<?php $value = null; $outer = []; echo isset($outer[$value[0]]);",
        "<?php $value = null; $outer = []; echo $outer[$value[0]] ?? 1;"] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap_err();
        assert!(error.message.contains("strict-nullability"), "{source}: {error:?}");
    }
}

/// Dynamic reads gain non-null predicates and typed failures on every supported target.
#[test]
fn nullable_receiver_native_guards_emit_on_every_target() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let module = super::semantic_sources::lower_capturing_arguments(SOURCE, target, profile, false, SourceMode::Php);
        assert!(module.functions.iter().flat_map(|function| &function.instructions)
            .any(|inst| inst.op == Op::ThrowTypeError), "{name}");
        crate::ir::validate_module(&module).unwrap();
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains(elephc_semantics::nullability::NON_NULL_MESSAGE), "{name}");
        assert!(!module.semantic_profile.contains(StrictFeature::Indexing));
        assert!(!module.semantic_profile.contains(StrictFeature::Coercions));
    }
}

/// Nullable reads retain selector effects, quiet roots and all temporary owners on rejection.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn nullable_receiver_native_values_and_owners() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for optimize in [false, true] {
        for mode in [SourceMode::Php, SourceMode::Lfc] {
            let module = super::semantic_sources::lower_capturing_arguments(SOURCE, Target::detect_host(), profile, optimize, mode);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "{optimize} {mode:?}: {stderr}");
            assert_eq!(String::from_utf8_lossy(&output.stdout), "KN|K7|q:q|ON|KT", "{stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
            assert!(!stderr.contains("Warning:"), "{stderr}");
        }
    }
}

/// Trusted code and unselected profiles keep the established receiver behavior.
#[test]
fn nullable_receiver_guards_respect_source_policy() {
    let source = "<?php function nullableRead(mixed $value): mixed { return $value[0]; }";
    for (profile, mode) in [(SemanticProfile::PERMISSIVE, SourceMode::Php),
        (SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true), SourceMode::Internal)] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::detect_host(), profile, false, mode);
        assert!(!module.functions.iter().flat_map(|function| &function.instructions)
            .any(|inst| inst.op == Op::ThrowTypeError));
    }
}

/// Ordinary property receivers are guarded on every target while probes keep their own protocol.
#[test]
fn nullable_property_receivers_emit_on_every_target() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let module = super::semantic_sources::lower_capturing_arguments(PROPERTIES, target, profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains(elephc_semantics::nullability::NON_NULL_MESSAGE), "{name}");
    }
}

/// Named/dynamic reads, nullsafe continuation, nested probes and owned receivers stay balanced.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn nullable_property_receivers_native_values_and_owners() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for optimize in [false, true] {
        for mode in [SourceMode::Php, SourceMode::Lfc] {
            let module = super::semantic_sources::lower_capturing_arguments(PROPERTIES, Target::detect_host(), profile, optimize, mode);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "{optimize} {mode:?}: stdout={}; {stderr}", String::from_utf8_lossy(&output.stdout));
            assert_eq!(String::from_utf8_lossy(&output.stdout), "P|8|KN|q:0:1:q|q|CN|O|Q", "{stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
            assert!(!stderr.contains("Warning:"), "{stderr}");
        }
    }
}

/// Known null receivers are rejected even when nested in a probe's runtime name expression.
#[test]
fn nullable_property_receivers_preserve_precise_static_probes() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for source in ["<?php $value=null; echo $value->property;",
        "<?php $value=null;$outer=null;echo isset($outer->{$value->property});"] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap_err();
        assert!(error.message.contains("strict-nullability"), "{source}: {error:?}");
    }
}

/// Declared non-null boundaries reject original null without changing weak non-null conversions.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn nullable_declared_boundaries_native_preserve_contracts() {
    let source = include_str!("../../../tests/fixtures/strict/nullable_boundaries.php");
    for strict in [false, true] {
      for optimize in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, strict);
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::detect_host(), profile, optimize, SourceMode::Php);
        let output = super::native_module::run(&module);
        let stdout = String::from_utf8_lossy(&output.stdout); let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "strict={strict} optimize={optimize}: {stdout}; {stderr}");
        assert_eq!(stdout, if strict { "LP|R|S:kept|NULL:7:8:NULL:NULL" }
            else { "L||S:kept|NULL:7:8:NULL:NULL" }, "{stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
      }
    }
}

/// Deferred original-null checks and declared returns lower through the same target-neutral EIR.
#[test]
fn nullable_declared_boundaries_emit_on_every_target() {
    let source = include_str!("../../../tests/fixtures/strict/nullable_boundaries.php");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let module = super::semantic_sources::lower_capturing_arguments(source, target, profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
