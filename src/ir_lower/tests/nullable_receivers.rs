//! Purpose:
//! Verifies independent null receiver guards and precise quiet access spines.
//!
//! Called from:
//! - Focused staged strict-nullability compiler tests.
//!
//! Key details:
//! - Staged helpers isolate receiver obligations from independently selected guarantees.

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
        assert_eq!(stdout, "LP|R|S:kept|NULL:7:8:NULL:NULL", "strict={strict} optimize={optimize}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
      }
    }
}

/// Deferred original-null checks and declared returns lower through the same target-neutral EIR.
#[test]
fn nullable_declared_boundaries_emit_on_every_target() {
    let source = include_str!("../../../tests/fixtures/strict/nullable_boundaries.php");
    for strict in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, strict);
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let module = super::semantic_sources::lower_capturing_arguments(source, target, profile, false, SourceMode::Php);
            crate::ir::validate_module(&module).unwrap();
            let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            assert_eq!(assembly.contains(elephc_semantics::nullability::NON_NULL_MESSAGE), strict, "{name}");
        }
    }
}

/// Original neutral builtin contracts reject known null before folding, including named/static unpacking.
#[test]
fn nullable_builtin_source_contracts_survive_folding() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for source in ["<?php echo strlen(null);", "<?php echo strlen(string: null);",
        "<?php echo strlen(...[null]);", "<?php if (false) { echo strlen(null); }",
        "<?php const ABSENT = null; echo strlen(ABSENT);", "<?php eval('echo strlen(null);');"] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        let error = crate::semantics::validate_source(&program, profile).unwrap_err();
        assert!(error.message.contains("strict-nullability"), "{source}: {error:?}");
        crate::semantics::validate_source(&program, SemanticProfile::PERMISSIVE).unwrap();
    }
    for source in ["<?php echo substr('abc', 1, null);", "<?php echo strlen(7);",
        "<?php echo gettype(null);", "<?php echo strlen((string)null);",
        "<?php if(false){echo 1/0;} echo PHP_INT_MAX+1;",
        "<?php if(false){echo 1+'bad';} echo 1=='1';",
        "<?php namespace App; function strlen($value) { return 0; } echo strlen(null);"] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        crate::semantics::validate_source(&program, profile).unwrap();
    }
}

/// Dynamic builtin inputs retain original null through direct, planned and callable argument paths.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn nullable_builtin_native_contracts_and_owners() {
    let source = include_str!("../../../tests/fixtures/strict/nullable_builtins.php");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for optimize in [false, true] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::detect_host(), profile, optimize, SourceMode::Php);
        let output = super::native_module::run(&module);
        let stdout = String::from_utf8_lossy(&output.stdout); let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "optimize={optimize}: {stdout}; {stderr}");
        assert_eq!(stdout, "DNSCAFIRLTJ|bc:1:NULL:0", "{stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        assert!(!stderr.contains("Warning:"), "{stderr}");
    }
}

/// The original builtin contract is available to every target's EIR lowering.
#[test]
fn nullable_builtin_contracts_emit_on_every_target() {
    let source = include_str!("../../../tests/fixtures/strict/nullable_builtins.php");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let module = super::semantic_sources::lower_capturing_arguments(source, target, profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Unknown Reflection targets cannot bypass null contracts through their legacy stub bodies.
#[test]
fn nullable_builtin_reflection_unknown_targets_are_explicit() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for source in [
        "<?php if($argc>0){$r=new ReflectionFunction('strlen');}else{$r=new ReflectionFunction('trim');}$r->invokeArgs(['x']);",
        "<?php function nullableReflectionSource():mixed{return null;}$r=new ReflectionFunction('strlen');try{$r->invoke(nullableReflectionSource());}catch(TypeError $e){}$r->invokeArgs(['x']);",
    ] {
        let error = super::semantic_sources::try_lower_capturing_arguments(source, Target::detect_host(), profile, false, SourceMode::Php).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("strict-nullability") && message.contains("known ReflectionFunction"), "{message}");
        super::semantic_sources::try_lower_capturing_arguments(source, Target::detect_host(), SemanticProfile::PERMISSIVE, false, SourceMode::Php).unwrap();
    }
}

/// Alias writes enforce surviving property declarations and release the contract after detach.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn nullable_property_aliases_preserve_contracts_and_owners() {
    let source = include_str!("../../../tests/fixtures/strict/nullable_aliases.php");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for optimize in [false, true] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::detect_host(), profile, optimize, SourceMode::Php);
        let output = super::native_module::run(&module);
        let stdout = String::from_utf8_lossy(&output.stdout); let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "optimize={optimize}: {stdout}; {stderr}");
        assert_eq!(stdout, "R:kept|S:saved|P:kept|string:7|NULL", "{stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    }
}

/// Managed aliases and implicit Mixed reference parameters use the same ABI on every target.
#[test]
fn nullable_property_aliases_emit_on_every_target() {
    let source = include_str!("../../../tests/fixtures/strict/nullable_aliases.php");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let module = super::semantic_sources::lower_capturing_arguments(source, target, profile, false, SourceMode::Php);
        assert!(module.required_runtime_features.native_reference_validation, "{name}");
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Typed stores and copied scalar misses retain original null until the declared consumer.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn nullable_local_stores_and_copies_preserve_original_null() {
    let source = include_str!("../../../tests/fixtures/strict/nullable_locals.lfc");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for optimize in [false, true] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::detect_host(), profile, optimize, SourceMode::Lfc);
        let output = super::native_module::run(&module);
        let stdout = String::from_utf8_lossy(&output.stdout); let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "optimize={optimize}: {stdout}; {stderr}");
        assert_eq!(stdout, "I|S:4|C|A|NULL:NULL:NULL", "{stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    }
}

/// Source checks retain invalid typed declarations even when control-flow pruning removes them.
#[test]
fn nullable_local_declarations_are_checked_before_folding() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for source in ["<?php if(false){int $value=null;}",
        "<?php if(false){array<int> $value=null;}"] {
        let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), SourceMode::Lfc).unwrap();
        let error = crate::semantics::validate_source(&program, profile).unwrap_err();
        assert!(error.message.contains("strict-nullability"), "{error:?}");
        crate::semantics::validate_source(&program, SemanticProfile::PERMISSIVE).unwrap();
    }
    for source in ["<?php ?int $value=null;", "<?php mixed $value=null;",
        "<?php int|mixed $value=null;"] {
        let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), SourceMode::Lfc).unwrap();
        crate::semantics::validate_source(&program, profile).unwrap();
    }
}

/// Declared null-only guards share the existing scalar storage adapters on all supported targets.
#[test]
fn nullable_local_stores_emit_on_every_target() {
    let source = include_str!("../../../tests/fixtures/strict/nullable_locals.lfc");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::parse(name).unwrap(), profile, false, SourceMode::Lfc);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Indirect Reflection cannot bypass unsupported target checks through synthetic null bodies.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn nullable_indirect_reflection_reports_explicit_unsupported_targets() {
    let source = include_str!("../../../tests/fixtures/strict/nullable_indirect_reflection.php");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for optimize in [false, true] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::detect_host(), profile, optimize, SourceMode::Php);
        let output = super::native_module::run(&module);
        let stdout = String::from_utf8_lossy(&output.stdout); let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "optimize={optimize}: {stdout}; {stderr}");
        assert_eq!(stdout, "VD|VC|VF|VAV|MV|R", "{stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    }
}

/// Unresolved invocation shells fail through target-neutral EIR and retain permissive fallbacks.
#[test]
fn nullable_indirect_reflection_shells_emit_on_every_target() {
    let source = include_str!("../../../tests/fixtures/strict/nullable_indirect_reflection.php");
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for feature in [Some(StrictFeature::Nullability), Some(StrictFeature::ArrayKeys), None] {
            let profile = feature.map_or(SemanticProfile::PERMISSIVE, |feature| SemanticProfile::PERMISSIVE.with(feature, true));
            let module = super::semantic_sources::lower_capturing_arguments(source, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
            for class in ["ReflectionFunction", "ReflectionMethod"] {
                for method in ["invoke", "invokeargs"] {
                    let function = module.class_methods.iter().find(|function|
                        function.name.eq_ignore_ascii_case(&format!("{class}::{method}"))).unwrap();
                    let throws = function.blocks.iter().any(|block|
                        matches!(block.terminator, Some(crate::ir::Terminator::Throw { .. })));
                    assert_eq!(throws, feature.is_some(), "{name}: {class}::{method} {feature:?}");
                }
            }
            crate::ir::validate_module(&module).unwrap();
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        }
    }
}

/// Non-scalar parameters reject original null only after every source argument has run.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn nullable_container_arguments_preserve_source_order_and_owners() {
    let source = include_str!("../../../tests/fixtures/strict/nullable_container_arguments.php");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for optimize in [false, true] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::detect_host(), profile, optimize, SourceMode::Php);
        let output = super::native_module::run(&module);
        let stdout = String::from_utf8_lossy(&output.stdout); let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "optimize={optimize}: {stdout}; {stderr}");
        assert_eq!(stdout, "LA|LI|LC|LO|1:1:object|1:1:object", "{stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    }
}

/// Opaque container inputs retain their distinct runtime declaration policies.
#[test]
fn nullable_container_arguments_preserve_opaque_contract_policies_on_every_target() {
    for declaration in ["array", "iterable", "callable"] {
        let source = format!("<?php function opaqueNullableInput():mixed{{return null;}} \
            function checkedNullableInput({declaration} $value):void{{}}checkedNullableInput(opaqueNullableInput());");
        let program = crate::parser::parse(&crate::lexer::tokenize(&source).unwrap()).unwrap();
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for enabled in [false, true] {
                let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, enabled);
                let result = crate::types::checker::check_staged_semantics(
                    &program,
                    Target::parse(target).unwrap(),
                    profile,
                );
                if declaration == "callable" {
                    result.unwrap_or_else(|error| {
                        panic!("{target}, callable, nullability={enabled}: {error:?}")
                    });
                } else {
                    let error = result
                        .expect_err("opaque structural contract must remain unsupported");
                    assert!(
                        error.message.contains("got mixed")
                            && error.message.contains(declaration),
                        "{target}, {declaration}, nullability={enabled}: {error:?}",
                    );
                }
            }
        }
    }
}

/// Non-scalar argument proofs and callable extraction preserve every supported target ABI.
#[test]
fn nullable_container_arguments_emit_on_every_target() {
    let source = include_str!("../../../tests/fixtures/strict/nullable_container_arguments.php");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
