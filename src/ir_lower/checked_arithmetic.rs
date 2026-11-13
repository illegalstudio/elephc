//! Purpose:
//! Lowers checked scalar integer arithmetic using shared target-aware overflow detection.
//!
//! Called from:
//! - Numeric expression and update lowering under an explicit checked policy.
//!
//! Key details:
//! - Wrapping bits never escape their failure guard.
//! - The existing IOverflows/CondBr pattern allows each target to fuse its flag test.

use super::context::{LoweredValue, LoweringContext};
use crate::ir::{Immediate, MixedNumericOp, Op};
use crate::span::Span;
use crate::types::PhpType;

/// Computes an integer result or throws ArithmeticError before its value can be observed.
pub(super) fn binary(
    ctx: &mut LoweringContext<'_, '_>, left: LoweredValue, operation: MixedNumericOp,
    right: LoweredValue, span: Span,
) -> LoweredValue {
    let opcode = match operation {
        MixedNumericOp::Add => Op::IAdd, MixedNumericOp::Sub => Op::ISub, MixedNumericOp::Mul => Op::IMul,
        MixedNumericOp::Pow => unreachable!("power uses a checked multiplication loop"),
    };
    let operands = vec![left.value, right.value];
    let result = ctx.emit_value(opcode, operands.clone(), None, PhpType::Int, opcode.default_effects(), Some(span));
    let overflow = ctx.emit_value(Op::IOverflows, operands, Some(Immediate::MixedNumericOp(operation)),
        PhpType::Bool, Op::IOverflows.default_effects(), Some(span));
    super::semantic_guards::require_false(ctx, overflow, Op::ThrowArithmeticError,
        "--strict-overflow: integer arithmetic overflow", Some(span));
    result
}

/// Negates an integer with the same signed-minimum check as subtraction from zero.
pub(super) fn negate(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span) -> LoweredValue {
    let zero = ctx.emit_value(Op::ConstI64, Vec::new(), Some(Immediate::I64(0)), PhpType::Int,
        Op::ConstI64.default_effects(), Some(span));
    binary(ctx, zero, MixedNumericOp::Sub, value, span)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;
    use crate::source::SourceMode;
    use elephc_semantics::{SemanticProfile, StrictFeature};

    /// Builds staged lowering input without advertising an unfinished CLI guarantee.
    fn lower(target: Target, mode: SourceMode) -> crate::ir::Module {
        let source = "<?php function strict_scalar(int $left, int $right): int { echo $left + $right; echo $left - $right; echo $left * $right; return -$left; } strict_scalar(1, 2);";
        let tokens = crate::lexer::tokenize(source).unwrap();
        let program = crate::parser::parse_with_mode(&tokens, mode).unwrap();
        let mut checked = crate::types::check_with_target_and_options(&program, target, Default::default(),
            &crate::generics::GenericContext::default()).unwrap();
        checked.semantic_profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
        crate::ir_lower::lower_program(&program, &checked, target, false).unwrap()
    }

    /// User scalar arithmetic preserves typed failure graphs on every supported native target.
    #[test]
    fn strict_scalar_overflow_lowering_checks_user_arithmetic_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for mode in [SourceMode::Php, SourceMode::Lfc] {
                let module = lower(Target::parse(name).unwrap(), mode);
                let function = module.functions.iter().find(|function| function.name == "strict_scalar").unwrap();
                assert_eq!(function.instructions.iter().filter(|inst| inst.op == Op::IOverflows).count(), 4, "{name} {mode:?}");
                assert_eq!(function.instructions.iter().filter(|inst| inst.op == Op::ThrowArithmeticError).count(), 4, "{name} {mode:?}");
                assert!(!function.instructions.iter().any(|inst| matches!(inst.op, Op::ICheckedAdd | Op::ICheckedSub | Op::ICheckedMul)), "{name} {mode:?}");
                crate::ir::validate_module(&module).unwrap();
                let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
                assert!(asm.contains("_spl_arithmetic_error_class_id"), "{name} {mode:?}");
            }
        }
    }

    /// Trusted implementation arithmetic keeps the PHP-promoting path under a strict module profile.
    #[test]
    fn strict_scalar_overflow_lowering_keeps_internal_php_operations() {
        let module = lower(Target::detect_host(), SourceMode::Internal);
        let function = module.functions.iter().find(|function| function.name == "strict_scalar").unwrap();
        assert!(!function.instructions.iter().any(|inst| matches!(inst.op, Op::IOverflows | Op::ThrowArithmeticError)));
        assert!(function.instructions.iter().any(|inst| inst.op == Op::ICheckedAdd));
    }
}
