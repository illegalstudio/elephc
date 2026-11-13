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
        elephc_semantics::numeric::INTEGER_OVERFLOW_MESSAGE, Some(span));
    result
}

/// Negates an integer with the same signed-minimum check as subtraction from zero.
pub(super) fn negate(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span) -> LoweredValue {
    let zero = ctx.emit_value(Op::ConstI64, Vec::new(), Some(Immediate::I64(0)), PhpType::Int,
        Op::ConstI64.default_effects(), Some(span));
    binary(ctx, zero, MixedNumericOp::Sub, value, span)
}

/// Borrows boxed operands with explicit unwind roots, avoiding hidden codegen allocations.
pub(super) fn boxed(
    ctx: &mut LoweringContext<'_, '_>, left: LoweredValue, operation: MixedNumericOp,
    right: LoweredValue, policy: elephc_semantics::arithmetic_policy::ArithmeticPolicy, span: Span,
) -> LoweredValue {
    use super::expr::call_operand_owners::{pin_in_flight_owners, unpin_in_flight_owners};
    let originals = pin_in_flight_owners(ctx, &[left.value, right.value], span);
    let boxed_left = boxed_operand(ctx, left, span);
    let left_root = if boxed_left.value != left.value { pin_in_flight_owners(ctx, &[boxed_left.value], span) } else { Vec::new() };
    let boxed_right = boxed_operand(ctx, right, span);
    let right_root = if boxed_right.value != right.value { pin_in_flight_owners(ctx, &[boxed_right.value], span) } else { Vec::new() };
    let result = ctx.emit_value(Op::MixedNumericBinop, vec![boxed_left.value, boxed_right.value],
        Some(Immediate::NumericOperation { op: operation, policy }), PhpType::Mixed,
        Op::MixedNumericBinop.arithmetic_effects(policy), Some(span));
    unpin_in_flight_owners(ctx, right_root, span);
    if boxed_right.value != right.value { super::ownership::release_if_owned(ctx, boxed_right, Some(span)); }
    unpin_in_flight_owners(ctx, left_root, span);
    if boxed_left.value != left.value { super::ownership::release_if_owned(ctx, boxed_left, Some(span)); }
    unpin_in_flight_owners(ctx, originals, span);
    result
}

/// Creates an independent box without consuming the source owner's normal-path release.
fn boxed_operand(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span) -> LoweredValue {
    use crate::ir::{IrHeapKind, IrType};
    if value.ir_type == IrType::Heap(IrHeapKind::Mixed) { return value; }
    let op = if value.ir_type == IrType::Heap(IrHeapKind::Union) { Op::MixedClone } else { Op::MixedBox };
    ctx.emit_value(op, vec![value.value], None, PhpType::Mixed, op.default_effects(), Some(span))
}

/// Roots a checked PHP update operand without hidden backend boxing or ownership transfers.
pub(super) fn update(
    ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, increment: bool,
    policy: elephc_semantics::arithmetic_policy::ArithmeticPolicy, span: Span,
) -> LoweredValue {
    use super::expr::call_operand_owners::{pin_in_flight_owners, unpin_in_flight_owners};
    let originals = pin_in_flight_owners(ctx, &[value.value], span);
    let operand = if value.ir_type == crate::ir::IrType::Str { value } else { boxed_operand(ctx, value, span) };
    let boxed_root = if operand.value != value.value { pin_in_flight_owners(ctx, &[operand.value], span) } else { Vec::new() };
    let result = ctx.emit_value(Op::StrIncDec, vec![operand.value], Some(Immediate::NumericUpdate { increment, policy }),
        PhpType::Mixed, Op::StrIncDec.arithmetic_effects(policy), Some(span));
    unpin_in_flight_owners(ctx, boxed_root, span);
    if operand.value != value.value { super::ownership::release_if_owned(ctx, operand, Some(span)); }
    unpin_in_flight_owners(ctx, originals, span);
    result
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

    /// Boxed arithmetic and powers retain policy, ownership roots and exceptions through optimization.
    #[test]
    fn strict_boxed_overflow_lowering_retains_operation_policy_on_all_targets() {
        let source = "<?php function strict_boxed(mixed $left, mixed $right): mixed { return ($left + $right) * $right - ($left ** $right); } function strict_power(int $left, int $right): mixed { return $left ** $right; } echo strict_boxed(2, 3); echo strict_power(2, 3);";
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut checked = crate::types::check_with_target(&program, target).unwrap();
            checked.semantic_profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
            let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
            for optimize in [false, true] {
                let mut module = module.clone();
                if optimize { crate::ir_passes::optimize_module(&mut module); }
                crate::ir::validate_module(&module).unwrap();
                let operations: Vec<_> = module.functions.iter().flat_map(|function| &function.instructions)
                    .filter(|inst| matches!(inst.immediate, Some(Immediate::NumericOperation { .. }))).collect();
                assert!(operations.len() >= 5, "{name} opt={optimize}");
                for inst in operations {
                    assert!(inst.arithmetic_policy().checked_overflow());
                    assert!(inst.effects.contains(crate::ir::Effects::MAY_THROW));
                }
                let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
                for helper in ["__rt_strict_mixed_numeric_add", "__rt_strict_mixed_numeric_sub",
                    "__rt_strict_mixed_numeric_mul", "__rt_strict_mixed_numeric_pow", "__rt_int_pow_or_throw"] {
                    assert!(asm.contains(helper), "{name} opt={optimize}: {helper}");
                }
                module.semantic_profile = SemanticProfile::PERMISSIVE;
                assert!(matches!(crate::ir::validate_module(&module),
                    Err(crate::ir::ValidationError::SemanticProfileMismatch { .. })), "{name} opt={optimize}");
            }
        }
    }

    /// Updates and typed buffer steps retain checked effects and policy through the EIR optimizer.
    #[test]
    fn strict_overflow_update_lowering_checks_storage_on_all_targets() {
        let source = "<?php function strict_update(int $index, mixed $value, buffer<int> $buffer): mixed { $index++; $value++; return ++$buffer[$index]; }";
        let tokens = crate::lexer::tokenize(source).unwrap();
        for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
            let program = crate::parser::parse_with_mode(&tokens, mode).unwrap();
            for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
                let target = Target::parse(name).unwrap();
                let mut checked = crate::types::check_with_target(&program, target).unwrap();
                checked.semantic_profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
                let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
                for optimize in [false, true] {
                    let mut module = module.clone();
                    if optimize { crate::ir_passes::optimize_module(&mut module); }
                    crate::ir::validate_module(&module).unwrap();
                    let function = module.functions.iter().find(|function| function.name == "strict_update").unwrap();
                    let user = mode != SourceMode::Internal;
                    assert_eq!(function.instructions.iter().any(|inst| matches!(inst.immediate,
                        Some(Immediate::NumericUpdate { .. })) && inst.effects.contains(crate::ir::Effects::MAY_THROW)), user, "{name} {optimize} {mode:?}");
                    assert_eq!(function.instructions.iter().any(|inst| inst.op == Op::IOverflows), user, "{name} {optimize} {mode:?}");
                    assert_eq!(function.instructions.iter().any(|inst| matches!(inst.op, Op::ICheckedAddToInt | Op::ICheckedSubToInt)), !user, "{name} {optimize} {mode:?}");
                    let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
                    assert_eq!(asm.contains("__rt_strict_mixed_inc_dec"), user, "{name} {optimize} {mode:?}");
                }
            }
        }
    }
}
