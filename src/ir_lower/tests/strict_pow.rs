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
