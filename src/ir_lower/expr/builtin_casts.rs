//! Purpose:
//! Shares rooted explicit numeric cast graphs between syntax and builtin invocations.
//!
//! Called from:
//! - BuiltinLoweringContext after shared argument planning and source-order evaluation.
//!
//! Key details:
//! - Caller-owned arguments remain borrowed, including across native object adapters.
//! - A selected string/base branch uses the exact radix kernel without changing other kinds.

use super::*;
use elephc_semantics::numeric_cast::NumericCastTarget;

/// Applies the invocation's contract to evaluated operands without consuming caller leases.
pub(in crate::ir_lower) fn lower(ctx: &mut LoweringContext<'_, '_>, target: NumericCastTarget,
    value: ValueId, base: Option<ValueId>, source_mode: crate::source::SourceMode, span: Span) -> LoweredValue {
    let previous = ctx.source_mode; ctx.source_mode = source_mode;
    let result = ctx.with_independent_write_receiver(|ctx| {
        let input = super::builtin_numeric::borrowed_operand(ctx, value, span);
        let expression = Expr::new(ExprKind::Variable("__builtin_cast_value".into()), span);
        if let (NumericCastTarget::Int, Some(base)) = (target, base) {
            integer_base(ctx, input, base, &expression)
        } else {
            let target = if target == NumericCastTarget::Int { CastType::Int } else { CastType::Float };
            super::ternary_cast::lower_scalar_cast_value(ctx, &target, input, &expression)
        }
    });
    ctx.source_mode = previous; result
}

/// Selects string radix parsing while preserving explicit scalar and native object cast contracts.
fn integer_base(ctx: &mut LoweringContext<'_, '_>, input: LoweredValue,
    base: ValueId, expression: &Expr) -> LoweredValue {
    let span = expression.span;
    let is_string = ctx.emit_value(Op::TypePredicate, vec![input.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::String)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(span));
    let initial = ctx.initialized_slots_snapshot();
    let string = ctx.builder.create_named_block("cast.radix_string", Vec::new());
    let other = ctx.builder.create_named_block("cast.scalar_value", Vec::new());
    let merge = ctx.builder.create_named_block("cast.integer_result", vec![(IrType::I64, PhpType::Int)]);
    ctx.builder.terminate(Terminator::CondBr { cond: is_string.value, then_target: string,
        then_args: Vec::new(), else_target: other, else_args: Vec::new() });
    ctx.builder.position_at_end(string);
    let text = super::ternary_cast::lower_scalar_cast_value(ctx, &CastType::String, input, expression);
    let pins = pin_in_flight_owners(ctx, &[text.value], span);
    let result = ctx.emit_value(Op::RuntimeCall, vec![text.value, base],
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::CheckedRadixInt)), PhpType::Int,
        Op::RuntimeCall.default_effects(), Some(span));
    unpin_in_flight_owners(ctx, pins, span); release_binary_operand_temporary(ctx, text, span);
    let string_initialized = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::Br { target: merge, args: vec![result.value] });
    ctx.restore_initialized_slots(initial.clone()); ctx.builder.position_at_end(other);
    let result = super::ternary_cast::lower_scalar_cast_value(ctx, &CastType::Int, input, expression);
    let other_initialized = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::Br { target: merge, args: vec![result.value] });
    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(merge_initialized_slots_for_expr(&initial, string_initialized, true, other_initialized, true));
    LoweredValue { value: ctx.builder.block_param(merge, 0), ir_type: IrType::I64 }
}
