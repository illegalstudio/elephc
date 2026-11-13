//! Purpose:
//! Verifies builtin powers reuse explicit checked operator policies on every target.
//!
//! Called from:
//! - Focused AST-to-EIR tests.
//!
//! Key details:
//! - Staged profiles retain permissive and Internal controls until the full guarantee is enabled.
//! - Generated callable graphs preserve the public builtin invocation contract.

use crate::ir::{Immediate, MixedNumericOp, Op};
use crate::source::SourceMode;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Checked builtin power shares the operator graph and survives both optimizer settings.
#[test]
fn strict_pow_builtin_lowering_reuses_operator_policies_on_all_targets() {
    let source = "<?php function strict_pow_builtin_probe(mixed $left, mixed $right): mixed { return pow($left, $right); } $f = pow(...); echo strict_pow_builtin_probe($argc, $argc); echo $f($argc, $argc);";
    for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
        for checked in [false, true] {
            let source = if checked && mode != SourceMode::Internal {
                format!("{source} function strict_pow_string_float_probe(string $left, float $right): float {{ return pow($left, $right); }} echo strict_pow_string_float_probe('2', 3.0);")
            } else { source.to_string() };
            let tokens = crate::lexer::tokenize(&source).unwrap();
            let program = crate::parser::parse_with_mode(&tokens, mode).unwrap();
            for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
                let target = crate::codegen::platform::Target::parse(name).unwrap();
                let mut types = crate::types::check_with_target(&program, target).unwrap();
                types.semantic_profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, checked);
                for optimize in [false, true] {
                    let mut module = crate::ir_lower::lower_program(&program, &types, target, false).unwrap();
                    if optimize { crate::ir_passes::optimize_module(&mut module); }
                    crate::ir::validate_module(&module).unwrap();
                    let function = module.functions.iter().find(|function| function.name == "strict_pow_builtin_probe").unwrap();
                    assert_eq!(function.instructions.iter().any(|instruction| instruction.op == Op::MixedNumericBinop
                        && matches!(instruction.immediate, Some(Immediate::NumericOperation { op: MixedNumericOp::Pow, .. }))),
                        checked && mode != SourceMode::Internal, "{name} {mode:?} {optimize}");
                    crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
                }
            }
        }
    }
}

/// Numeric guarantees keep caller operand kinds while default and Internal signatures retain PHP ABI.
#[test]
fn strict_pow_builtin_callable_signatures_preserve_numeric_kinds() {
    use crate::types::PhpType;
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
    let legacy = crate::builtins::registry::first_class_callable_sig("pow").unwrap();
    assert!(legacy.params.iter().all(|(_, ty)| *ty == PhpType::Mixed));
    let public = crate::builtins::registry::first_class_callable_sig_with_semantics("pow", profile).unwrap();
    assert!(public.params.iter().all(|(_, ty)| *ty == PhpType::Mixed));
    assert_eq!(public.return_type, PhpType::Mixed);
    let internal = crate::builtins::registry::function_sig_with_semantics("pow", profile, SourceMode::Internal).unwrap();
    assert!(internal.params.iter().all(|(_, ty)| *ty == PhpType::Mixed));
}


/// Each independent numeric profile selects the same guarded power graph on every supported target.
#[test]
fn strict_numeric_builtin_pow_independent_profiles_and_target_graphs() {
    let source = "<?php function profiledPow(mixed $base, mixed $exponent): mixed { return pow($base, $exponent); } $f = pow(...); echo $f($argc, 3);";
    for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
        let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
        for feature in [StrictFeature::Arithmetic, StrictFeature::Coercions] {
            for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
                let target = crate::codegen::platform::Target::parse(name).unwrap();
                let checked = crate::types::checker::check_staged_semantics(&program, target,
                    SemanticProfile::PERMISSIVE.with(feature, true)).unwrap();
                let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
                for optimize in [false, true] {
                    let mut module = module.clone(); if optimize { crate::ir_passes::optimize_module(&mut module); }
                    crate::ir::validate_module(&module).unwrap();
                    let function = module.functions.iter().find(|function| function.name == "profiledPow").unwrap();
                    assert_eq!(function.instructions.iter().any(|instruction| matches!(instruction.immediate,
                        Some(Immediate::NumericOperation { op: MixedNumericOp::Pow, policy }) if policy.requirements().contains(feature))),
                        mode != SourceMode::Internal, "{name}, {mode:?}, {feature:?}, opt={optimize}");
                    crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
                }
            }
        }
    }
}

/// Literal proofs and typed parameters reject implicit power operands before destructive folding.
#[test]
fn strict_numeric_builtin_pow_source_domains_named_spread_and_parameter_types() {
    for feature in [StrictFeature::Arithmetic, StrictFeature::Coercions] {
        let profile = SemanticProfile::PERMISSIVE.with(feature, true);
        for source in ["<?php if (false) { pow('2', 3); }", "<?php pow(exponent: 3, num: true);",
            "<?php const POW_VALUE = 'bad'; pow(POW_VALUE, 3);", "<?php pow(...[null, 3]);",
            "<?php if (false) { pow([1], 2); }",
            "<?php function parameterPow(bool $value): mixed { return pow($value, 2); }"] {
            let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
            let error = crate::types::checker::check_staged_semantics(&program, crate::codegen::platform::Target::detect_host(), profile)
                .expect_err(source);
            assert!(error.message.contains(feature.flag()), "{source}: {error:?}");
        }
        let arrays = crate::parser::parse(&crate::lexer::tokenize("<?php $values = [1] + [2];").unwrap()).unwrap();
        assert!(crate::types::checker::check_staged_semantics(&arrays, crate::codegen::platform::Target::detect_host(), profile).is_ok());
        let negative = crate::parser::parse(&crate::lexer::tokenize("<?php if (false) { pow(2, -1); }").unwrap()).unwrap();
        assert_eq!(crate::types::checker::check_staged_semantics(&negative,
            crate::codegen::platform::Target::detect_host(), profile).is_err(), feature == StrictFeature::Arithmetic);
    }
}


/// Known decimal results use the native operator's object contract even with a floating exponent.
#[test]
fn strict_numeric_builtin_pow_native_class_results_follow_operator_contracts() {
    let source = r"<?php function typedNumberPow(): BcMath\Number { return pow(new BcMath\Number('2'), 3.0); }";
    let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
    let mut inventory = crate::optimize::reachability::PreludeInventory::new();
    let program = crate::core_prelude::inject_if_used(program, &mut inventory);
    let program = crate::bcmath_prelude::inject_if_used(program, false, &mut inventory);
    let program = crate::name_resolver::resolve(program).unwrap();
    for feature in [StrictFeature::Arithmetic, StrictFeature::Coercions] {
        let profile = SemanticProfile::PERMISSIVE.with(feature, true);
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = crate::codegen::platform::Target::parse(name).unwrap();
            let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
            let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
            for optimize in [false, true] {
                let mut module = module.clone(); if optimize { crate::ir_passes::optimize_module(&mut module); }
                crate::ir::validate_module(&module).unwrap();
                crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            }
        }
    }
}
