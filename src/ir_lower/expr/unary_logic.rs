//! Purpose:
//! Unary, truthiness, throw, print, and logical-expression lowering.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers a numeric unary operation.
pub(super) fn lower_numeric_unary(
    ctx: &mut LoweringContext<'_, '_>,
    inner: &Expr,
    int_op: Op,
    float_op: Op,
    expr: &Expr,
) -> LoweredValue {
    let value = lower_expr(ctx, inner);
    match value.ir_type {
        IrType::F64 => ctx.emit_value(float_op, vec![value.value], None, PhpType::Float, float_op.default_effects(), Some(expr.span)),
        IrType::I64 => {
            // Check if the type checker promoted this to Mixed (non-constant int negate
            // can overflow PHP_INT_MIN to float).
            let result_php_type = fallback_expr_type(expr);
            if result_php_type == PhpType::Mixed && int_op == Op::INeg {
                // Emit a checked negate via the mixed numeric sub helper: 0 - value
                let zero = lower_int_literal(ctx, 0, expr);
                return lower_mixed_numeric_binary(ctx, zero, value, MixedNumericOp::Sub, expr);
            }
            ctx.emit_value(int_op, vec![value.value], None, PhpType::Int, int_op.default_effects(), Some(expr.span))
        }
        IrType::TaggedScalar => {
            let narrowed = lower_tagged_scalar_to_int(ctx, value, Some(expr.span));
            ctx.emit_value(int_op, vec![narrowed.value], None, PhpType::Int, int_op.default_effects(), Some(expr.span))
        }
        _ if int_op == Op::INeg => {
            let zero = lower_int_literal(ctx, 0, expr);
            let result = lower_mixed_numeric_binary(ctx, zero, value, MixedNumericOp::Sub, expr);
            // Mirror the binary mixed-op path: an owning boxed operand (e.g.
            // `-($i * 7 + 1)`, issue #500) must be released once consumed.
            release_binary_operand_temporary(ctx, value, expr.span);
            result
        }
        _ => ctx.emit_value(Op::RuntimeCall, vec![value.value], None, PhpType::Mixed, Effects::all(), Some(expr.span)),
    }
}

/// Lowers an integer unary operation.
pub(super) fn lower_int_unary(ctx: &mut LoweringContext<'_, '_>, inner: &Expr, op: Op, expr: &Expr) -> LoweredValue {
    let value = lower_expr(ctx, inner);
    if value.ir_type == IrType::I64 {
        ctx.emit_value(op, vec![value.value], None, PhpType::Int, op.default_effects(), Some(expr.span))
    } else if value.ir_type == IrType::TaggedScalar {
        let narrowed = lower_tagged_scalar_to_int(ctx, value, Some(expr.span));
        ctx.emit_value(op, vec![narrowed.value], None, PhpType::Int, op.default_effects(), Some(expr.span))
    } else {
        ctx.emit_value(Op::RuntimeCall, vec![value.value], None, PhpType::Mixed, Effects::all(), Some(expr.span))
    }
}

/// Lowers a tagged scalar into PHP int semantics, coercing null to zero.
pub(super) fn lower_tagged_scalar_to_int(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    ctx.emit_value(
        Op::Cast,
        vec![value.value],
        Some(Immediate::CastTarget(IrType::I64)),
        PhpType::Int,
        Op::Cast.default_effects(),
        span,
    )
}

/// Lowers logical negation.
pub(super) fn lower_not(ctx: &mut LoweringContext<'_, '_>, inner: &Expr, expr: &Expr) -> LoweredValue {
    let value = lower_expr(ctx, inner);
    let value = ctx.truthy_consuming(value, Some(expr.span));
    let zero = lower_int_literal(ctx, 0, expr);
    ctx.emit_value(
        Op::ICmp,
        vec![value.value, zero.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Eq)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(expr.span),
    )
}

/// Lowers throw used as an expression and returns a placeholder null value.
pub(super) fn lower_throw_expr(ctx: &mut LoweringContext<'_, '_>, inner: &Expr, expr: &Expr) -> LoweredValue {
    let value = lower_expr(ctx, inner);
    // Match statement-form `throw`: transfer owning temps, but retain loads that
    // leave a local slot as owner (e.g. `true ? throw $e : 0` after a catch bind).
    let transferable = ctx.value_is_owning_temporary(value)
        && !ctx.value_is_owned_unboxed_local_load(value.value);
    let value = if transferable {
        value
    } else {
        crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(inner.span))
    };
    ctx.emit_void(
        Op::ThrowException,
        vec![value.value],
        None,
        Op::ThrowException.default_effects(),
        Some(expr.span),
    );
    lower_null(ctx, expr)
}

/// Lowers an error-suppressed expression.
pub(super) fn lower_error_suppress(ctx: &mut LoweringContext<'_, '_>, inner: &Expr, expr: &Expr) -> LoweredValue {
    ctx.emit_void(Op::ErrorSuppressBegin, Vec::new(), None, Op::ErrorSuppressBegin.default_effects(), Some(expr.span));
    let value = lower_expr(ctx, inner);
    ctx.emit_void(Op::ErrorSuppressEnd, Vec::new(), None, Op::ErrorSuppressEnd.default_effects(), Some(expr.span));
    value
}

/// Lowers `print`.
pub(super) fn lower_print(ctx: &mut LoweringContext<'_, '_>, inner: &Expr, expr: &Expr) -> LoweredValue {
    let value = lower_expr(ctx, inner);
    // `print` renders through the same output path as `echo`, so it raises the same warnings —
    // an array converted to `Array`, a NaN coerced to string — and, like `echo`, only publishes
    // the ` in FILE on line N` tail when the instruction admits it may warn. Without the
    // admission `print $a;` reported whatever line the PREVIOUS diagnostic had published.
    let mut effects = Op::PrintValue.default_effects();
    if crate::ir_lower::stmt::output_value_can_warn(value.ir_type) {
        effects |= crate::ir::Effects::MAY_WARN;
    }
    ctx.emit_void(Op::PrintValue, vec![value.value], None, effects, Some(expr.span));
    lower_int_literal(ctx, 1, expr)
}

/// Lowers short-circuiting logical `&&` and `||`.
pub(super) fn lower_logical_binary(
    ctx: &mut LoweringContext<'_, '_>,
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let lhs = lower_expr(ctx, left);
    let lhs = ctx.truthy_consuming(lhs, Some(left.span));
    let temp_name = ctx.declare_hidden_temp(PhpType::Bool);
    let rhs_block = ctx.builder.create_named_block("logical.rhs", Vec::new());
    let const_block = ctx.builder.create_named_block("logical.const", Vec::new());
    let merge = ctx.builder.create_named_block("logical.merge", Vec::new());
    let (then_target, else_target) = match op {
        BinOp::And => (rhs_block, const_block),
        BinOp::Or => (const_block, rhs_block),
        _ => unreachable!("only short-circuit logical operators reach this lowering"),
    };
    ctx.builder.terminate(Terminator::CondBr {
        cond: lhs.value,
        then_target,
        then_args: Vec::new(),
        else_target,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(rhs_block);
    let rhs = lower_expr(ctx, right);
    let rhs = ctx.truthy_consuming(rhs, Some(right.span));
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, rhs, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(const_block);
    let const_value = emit_bool_literal(ctx, matches!(op, BinOp::Or), Some(expr.span));
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, const_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Lowers non-short-circuiting PHP logical `xor`.
pub(super) fn lower_logical_xor(
    ctx: &mut LoweringContext<'_, '_>,
    left: &Expr,
    right: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let lhs = lower_expr(ctx, left);
    let lhs = lower_truthy_bool(ctx, lhs, Some(left.span));
    let rhs = lower_expr(ctx, right);
    let rhs = lower_truthy_bool(ctx, rhs, Some(right.span));
    ctx.emit_value(
        Op::ICmp,
        vec![lhs.value, rhs.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Ne)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(expr.span),
    )
}

/// Converts a lowered PHP value into a canonical boolean and releases an owned input.
pub(super) fn lower_truthy_bool(
    ctx: &mut LoweringContext<'_, '_>,
    input: LoweredValue,
    span: Option<crate::span::Span>,
) -> LoweredValue {
    let owns_input = ctx.value_is_owning_temporary(input);
    let result = match ctx.builder.value_php_type(input.value).codegen_repr() {
        PhpType::Bool => input,
        PhpType::Int => {
            let zero = ctx
                .builder
                .emit_with_effects(
                    Op::ConstI64,
                    Vec::new(),
                    Some(Immediate::I64(0)),
                    IrType::I64,
                    PhpType::Int,
                    Ownership::NonHeap,
                    Op::ConstI64.default_effects(),
                    span,
                )
                .expect("const_i64 produces a value");
            ctx.emit_value(
                Op::ICmp,
                vec![input.value, zero],
                Some(Immediate::CmpPredicate(CmpPredicate::Ne)),
                PhpType::Bool,
                Op::ICmp.default_effects(),
                span,
            )
        }
        PhpType::Void | PhpType::Never => emit_bool_literal(ctx, false, span),
        _ => ctx.emit_value(
            Op::IsTruthy,
            vec![input.value],
            None,
            PhpType::Bool,
            Op::IsTruthy.default_effects(),
            span,
        ),
    };
    if owns_input && result.value != input.value {
        crate::ir_lower::ownership::release_if_owned(ctx, input, span);
    }
    result
}

