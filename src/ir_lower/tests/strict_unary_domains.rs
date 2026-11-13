//! Purpose:
//! Verifies numeric unary and update contracts before folding and across target lowering.
//!
//! Called from:
//! - Focused compiler library tests.
//!
//! Key details:
//! - Staged checking runs the real frontend while public admission remains closed.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Known bad kinds fail before optimization while explicit numeric and string-complement cases survive.
#[test]
fn strict_unary_domain_frontend_preserves_source_kinds_and_typed_update_contracts() {
    for feature in [StrictFeature::Arithmetic, StrictFeature::Coercions] {
        let profile = SemanticProfile::PERMISSIVE.with(feature, true);
        for source in [
            "<?php if (false) { echo -true; }", "<?php const BAD = 'bad'; if (false) { echo -BAD; }",
            "<?php echo -null;", "<?php echo -[1];", "<?php echo ~7.0;",
            "<?php const BAD = false; echo ~BAD;",
            "<?php function typedUnary(string $value): mixed { return -$value; }",
            "<?php function typedUpdate(string $value): void { $value++; }",
            "<?php $value = true; $value++;", "<?php $value = null; --$value;",
        ] {
            let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
            let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).expect_err(source);
            assert!(error.message.contains(feature.flag()), "{source}: {error:?}");
        }
        for source in ["<?php echo -1.5; echo ~7;", "<?php $value = 7; ++$value; $value--;",
            "<?php function dynamicUpdate(mixed &$value): mixed { return $value++; }"] {
            let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
            assert!(crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).is_ok(), "{source}");
        }
        let string = crate::parser::parse(&crate::lexer::tokenize("<?php echo ~'A';").unwrap()).unwrap();
        assert_eq!(crate::types::checker::check_staged_semantics(&string, Target::detect_host(), profile).is_ok(),
            feature == StrictFeature::Coercions);
    }
    let unknown = crate::parser::parse(&crate::lexer::tokenize("<?php if (false) { echo -'bad'; }").unwrap()).unwrap();
    assert!(crate::semantics::validate_source(&unknown, SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true)).is_ok());
}

/// Guards and typed scalar bit-not lower identically on every target with trusted source exemptions.
#[test]
fn strict_unary_domain_eir_targets_optimizers_and_internal_source() {
    let source = "<?php function domainNegate(mixed $value): mixed { return -$value; } function domainBitNot(mixed $value): mixed { return ~$value; } function domainUpdate(mixed &$value): mixed { return $value++; }";
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
            for feature in [StrictFeature::Arithmetic, StrictFeature::Coercions] {
                let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
                let checked = crate::types::checker::check_staged_semantics(&program, target,
                    SemanticProfile::PERMISSIVE.with(feature, true)).unwrap();
                let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
                for optimize in [false, true] {
                    let mut module = module.clone();
                    if optimize { crate::ir_passes::optimize_module(&mut module); }
                    crate::ir::validate_module(&module).unwrap();
                    let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
                    assert!(asm.contains("__rt_mixed_bit_not"), "{name}, {mode:?}, {feature:?}");
                    assert_eq!(asm.contains(&format!("--{} requires", feature.flag())), mode != SourceMode::Internal,
                        "{name}, {mode:?}, {feature:?}, opt={optimize}");
                }
            }
        }
    }
}


/// Every target negates floats by sign, retaining NaN payloads and both zero encodings.
#[test]
fn strict_unary_ieee_scalar_negation_uses_sign_preserving_target_instructions() {
    let source = "<?php function scalarIeeeNegate(float $value): float { return -$value; }";
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        let checked = crate::types::check_with_target(&program, target).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        for optimize in [false, true] {
            let mut module = module.clone();
            if optimize { crate::ir_passes::optimize_module(&mut module); }
            let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            if name.ends_with("x86_64") {
                for instruction in ["pcmpeqd xmm1, xmm1", "psllq xmm1, 63", "xorpd xmm0, xmm1"] {
                    assert!(asm.contains(instruction), "{name}, opt={optimize}: {instruction}");
                }
                assert!(!asm.contains("subsd xmm1, xmm0"));
            } else { assert!(asm.contains("fneg d0, d0"), "{name}, opt={optimize}"); }
        }
    }
}


/// Source proofs and standalone folding retain unary-plus domain violations before optimization.
#[test]
fn strict_unary_ieee_plus_frontend_retains_original_kinds_and_fold_guards() {
    for feature in [StrictFeature::Arithmetic, StrictFeature::Coercions] {
        let profile = SemanticProfile::PERMISSIVE.with(feature, true);
        for source in ["<?php if (false) { echo +true; }", "<?php const BAD_PLUS = '2'; echo +BAD_PLUS;",
            "<?php function numericPlus(string $value): mixed { return +$value; }"] {
            let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
            let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).expect_err(source);
            assert!(error.message.contains(feature.flag()), "{source}: {error:?}");
        }
        let program = crate::parser::parse(&crate::lexer::tokenize("<?php echo +true;").unwrap()).unwrap();
        let _policy = crate::optimize::scoped_semantic_profile(profile);
        let folded = crate::optimize::fold_constants(program);
        assert!(format!("{folded:?}").contains("UnaryPlus"), "{feature:?}: a fold erased its domain guard");
    }
}

/// Typed unary calls retain their explicit policy through every target and optimizer configuration.
#[test]
fn strict_unary_ieee_eir_metadata_profile_validation_and_native_lowering() {
    use crate::ir::{Immediate, RuntimeCallTarget};
    let source = "<?php function mixedIeeePlus(mixed $value): mixed { return +$value; } function mixedIeeeNegate(mixed $value): mixed { return -$value; }";
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        for feature in [None, Some(StrictFeature::Overflow), Some(StrictFeature::Arithmetic), Some(StrictFeature::Coercions)] {
            let profile = feature.map_or(SemanticProfile::PERMISSIVE, |feature| SemanticProfile::PERMISSIVE.with(feature, true));
            for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
                let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
                let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
                let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
                for optimize in [false, true] {
                    let mut module = module.clone();
                    if optimize { crate::ir_passes::optimize_module(&mut module); }
                    crate::ir::validate_module(&module).unwrap();
                    let calls = module.functions.iter().flat_map(|function| function.instructions.iter()).filter_map(|inst| {
                        if let Some(Immediate::RuntimeCall(RuntimeCallTarget::NumericUnary { policy, .. })) = inst.immediate { Some(policy) } else { None }
                    }).collect::<Vec<_>>();
                    assert_eq!(calls.len(), 2, "{name}, {mode:?}, {feature:?}, opt={optimize}");
                    let expected = if mode == SourceMode::Internal { SemanticProfile::PERMISSIVE } else { profile };
                    assert!(calls.iter().all(|policy| policy.requirements() == expected));
                    let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
                    assert!(asm.contains("__rt_numeric_unary_plus"), "{name}");
                    let negate = if expected.contains(StrictFeature::Overflow) { "__rt_strict_numeric_unary_negate" } else { "__rt_numeric_unary_negate" };
                    assert!(asm.contains(negate), "{name}");
                    if mode != SourceMode::Internal && feature.is_some() {
                        module.semantic_profile = SemanticProfile::PERMISSIVE;
                        assert!(matches!(crate::ir::validate_module(&module), Err(crate::ir::ValidationError::SemanticProfileMismatch { .. })));
                    }
                }
            }
        }
    }
}
