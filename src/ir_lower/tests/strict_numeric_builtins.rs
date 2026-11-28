//! Purpose:
//! Verifies integer division retains typed catchable failure handling across all targets.
//!
//! Called from:
//! - Focused staged numeric lowering tests.
//!
//! Key details:
//! - PHP already rejects INT_MIN divided by minus one; strict overflow shares that kernel.
//! - Both source and generated callable paths use the common inline exception machinery.

use crate::source::SourceMode;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Integer division preserves its integer ABI and target-aware shared exception emission.
#[test]
fn strict_numeric_builtins_intdiv_shared_failure_emits_on_all_targets() {
    let source = "<?php function integer_division_probe(int $left, int $right): int { return intdiv($left, $right); } $divide = intdiv(...); $name = $argc > 0 ? 'intdiv' : 'fdiv'; try { echo integer_division_probe($argc, -1); echo $name($argc, -1); echo $divide($argc, 0); } catch (ArithmeticError $error) { echo 'caught'; }";
    let tokens = crate::lexer::tokenize(source).unwrap();
    for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
        let program = crate::parser::parse_with_mode(&tokens, mode).unwrap();
        for checked in [false, true] {
            for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
                let target = crate::codegen::platform::Target::parse(name).unwrap();
                let mut types = crate::types::check_with_target(&program, target).unwrap();
                types.semantic_profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, checked);
                for optimize in [false, true] {
                    let mut module = crate::ir_lower::lower_program(&program, &types, target, false).unwrap();
                    if optimize { crate::ir_passes::optimize_module(&mut module); }
                    crate::ir::validate_module(&module).unwrap();
                    let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
                    assert!(asm.contains("_spl_arithmetic_error_class_id"), "{name} {mode:?} {optimize}");
                    assert!(asm.contains("__rt_throw_current"), "{name}");
                }
            }
        }
    }
}


/// Numeric builtin guards retain original argument kinds and public callable signatures on every ABI.
#[test]
fn strict_numeric_parameters_frontend_callables_and_all_target_guards() {
    use crate::ir::Op;
    use crate::types::PhpType;
    let source = "<?php function numericAbs(mixed $value): mixed { return abs($value); } function numericIntdiv(mixed $left, mixed $right): int { return intdiv($left, $right); } $absolute = abs(...); $divide = intdiv(...); echo $absolute($argc); echo $divide($argc, 2);";
    for feature in [StrictFeature::Arithmetic, StrictFeature::Coercions] {
        let profile = SemanticProfile::PERMISSIVE.with(feature, true);
        for builtin in ["abs", "intdiv"] {
            let public = crate::builtins::registry::first_class_callable_sig_with_semantics(builtin, profile).unwrap();
            assert!(public.params.iter().all(|(_, ty)| *ty == PhpType::Mixed));
            let internal = crate::builtins::registry::function_sig_with_semantics(builtin, profile, SourceMode::Internal).unwrap();
            let legacy = crate::builtins::registry::function_sig(builtin).unwrap();
            assert_eq!(internal.params, legacy.params);
        }
        for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
            let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
            for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
                let target = crate::codegen::platform::Target::parse(name).unwrap();
                let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
                let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
                for optimize in [false, true] {
                    let mut module = module.clone(); if optimize { crate::ir_passes::optimize_module(&mut module); }
                    for function in ["numericAbs", "numericIntdiv"] {
                        let function = module.functions.iter().find(|body| body.name == function).unwrap();
                        assert_eq!(function.instructions.iter().any(|inst| inst.op == Op::ThrowTypeError),
                            mode != SourceMode::Internal, "{name}, {mode:?}, {feature:?}, opt={optimize}");
                    }
                    crate::ir::validate_module(&module).unwrap();
                    crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
                }
            }
        }
    }
}

/// Proven invalid scalar parameters fail before folding, including dead branches and named spreads.
#[test]
fn strict_numeric_parameters_source_kinds_and_explicit_cast_boundaries() {
    let target = crate::codegen::platform::Target::detect_host();
    for feature in [StrictFeature::Arithmetic, StrictFeature::Coercions] {
        let profile = SemanticProfile::PERMISSIVE.with(feature, true);
        for source in ["<?php if (false) { abs('2'); }", "<?php abs(num: true);", "<?php abs(null);",
            "<?php abs([1]);", "<?php const BAD_ABS = '2.5'; abs(BAD_ABS);",
            "<?php function badAbs(bool $value): mixed { return abs($value); }",
            "<?php if (false) { intdiv(2.0, 1); }", "<?php intdiv(num2: '2', num1: 8);",
            "<?php intdiv(...[true, 2]);", "<?php intdiv(null, 2);", "<?php intdiv([1], 2);",
            "<?php function badDivision(float $value): int { return intdiv($value, 2); }",
            "<?php function badDivision(mixed $value): int { return intdiv($value, 2.0); }",
            "<?php if (false) { intdiv($argc, 2.0); }"] {
            let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
            let error = crate::types::checker::check_staged_semantics(&program, target, profile).expect_err(source);
            assert!(error.message.contains(feature.flag()), "{source}: {error:?}");
        }
        let source = "<?php echo abs((int) '2'); echo abs(-2.5); echo intdiv((int) 2.5, 1); echo intdiv(...[8, 2]);";
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
    }
}
