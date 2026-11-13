//! Purpose:
//! Verifies checked abs policies survive EIR optimization and public callable construction.
//!
//! Called from:
//! - Focused AST-to-EIR unit tests.
//!
//! Key details:
//! - Staged module profiles exercise the kernel without publishing an incomplete flag.
//! - Trusted Internal operations retain their PHP-promoting runtime target.

use crate::ir::{Immediate, RuntimeCallTarget, RuntimeFnId};
use crate::source::SourceMode;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Explicit abs policies survive legacy sink passes, source modes and every target backend.
#[test]
fn strict_abs_lowering_retains_checked_policy_on_all_targets() {
    let source = "<?php function strict_abs_probe(mixed $n): mixed { return abs($n); } $value = (int) abs($argc); $f = abs(...); echo strict_abs_probe($argc); echo $f($argc);";
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
                    if checked && mode != SourceMode::Internal {
                        assert!(!module.functions.iter().flat_map(|function| &function.instructions).any(|instruction|
                            matches!(instruction.immediate, Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(RuntimeFnId::Abs | RuntimeFnId::AbsInt))))),
                            "{name} {optimize}: a legacy sink dropped the checked contract");
                        let mut mismatched = module.clone();
                        mismatched.semantic_profile = SemanticProfile::PERMISSIVE;
                        assert!(crate::ir::validate_module(&mismatched).is_err(), "{name}: operation profile must match its module");
                    }
                    let function = module.functions.iter().find(|function| function.name == "strict_abs_probe").unwrap();
                    assert_eq!(function.instructions.iter().any(|instruction| matches!(instruction.immediate,
                        Some(Immediate::RuntimeCall(RuntimeCallTarget::NumericFunction { target: RuntimeFnId::Abs, .. })))),
                        checked && mode != SourceMode::Internal, "{name} {mode:?} {optimize}");
                    crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
                }
            }
        }
    }
}
