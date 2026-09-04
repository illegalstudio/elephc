//! Purpose:
//! Positional spread lowering and arity guards.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;
use crate::names::NameKind;

/// Lowers one trailing indexed spread in a fixed-arity positional call.
pub(super) fn lower_positional_spread_args_with_signature(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    args: &[Expr],
) -> Option<Vec<crate::ir::ValueId>> {
    let spread_idx = single_trailing_indexed_spread_arg(ctx, args)?;
    let regular_param_count = crate::types::call_args::regular_param_count(sig);
    if spread_idx > regular_param_count {
        return None;
    }
    let first_spread_param_idx = spread_idx;
    let required_len = required_positional_spread_len(sig, first_spread_param_idx, regular_param_count);
    let ExprKind::Spread(inner) = &args[spread_idx].kind else {
        return None;
    };
    if static_indexed_spread_len(inner).is_some_and(|len| len >= required_len) {
        return None;
    }

    let mut operands = Vec::with_capacity(regular_param_count);
    for (index, arg) in args[..spread_idx].iter().enumerate() {
        operands.push(lower_arg_with_signature(ctx, sig, index, arg));
    }

    let spread_type = indexed_spread_source_type(ctx, inner)?;
    let spread = lower_expr(ctx, inner);
    let temp_name = ctx.declare_hidden_temp(spread_type.clone());
    store_value_into_temp(ctx, &temp_name, spread_type, spread, args[spread_idx].span);
    let spread_expr = Expr::new(ExprKind::Variable(temp_name), inner.span);
    let spread_value = lower_expr(ctx, &spread_expr);
    emit_positional_spread_min_len_guard(
        ctx,
        spread_value.value,
        required_len,
        args[spread_idx].span,
    );

    for param_idx in first_spread_param_idx..regular_param_count {
        let element_idx = param_idx - first_spread_param_idx;
        let default = sig.defaults.get(param_idx).and_then(|default| default.as_ref());
        let expr = if let Some(default) = default {
            if element_idx < required_len {
                spread_element_expr_for_ir(
                    &spread_expr,
                    element_idx,
                    None,
                    false,
                    args[spread_idx].span,
                )
            } else {
                spread_element_or_default_expr_for_ir(
                    &spread_expr,
                    element_idx,
                    None,
                    false,
                    default.clone(),
                    args[spread_idx].span,
                )
            }
        } else {
            spread_element_expr_for_ir(
                &spread_expr,
                element_idx,
                None,
                false,
                args[spread_idx].span,
            )
        };
        operands.push(lower_expr(ctx, &expr).value);
    }

    if sig.variadic.is_some() {
        let tail_offset = regular_param_count.saturating_sub(spread_idx);
        let tail = positional_spread_tail_expr(&spread_expr, tail_offset, args[spread_idx].span);
        let actual_count = positional_spread_actual_count_expr(
            &spread_expr,
            spread_idx,
            args[spread_idx].span,
        );
        if crate::func_args::sig_has_hidden_argc_param(sig) {
            operands.push(lower_expr(ctx, &actual_count).value);
        }
        let tail = if crate::func_args::sig_collects_optional_arg_count(sig) {
            Expr::new(
                ExprKind::ArrayLiteral(vec![
                    actual_count,
                    Expr::new(ExprKind::Spread(Box::new(tail)), args[spread_idx].span),
                ]),
                args[spread_idx].span,
            )
        } else {
            tail
        };
        let tail = lower_expr(ctx, &tail);
        let tail = coerce_spread_variadic_array(ctx, sig, tail, args[spread_idx].span);
        operands.push(tail.value);
    }

    Some(operands)
}

/// Builds `array_slice($spread, $offset)` for the values beyond regular parameters.
fn positional_spread_tail_expr(spread: &Expr, offset: usize, span: crate::span::Span) -> Expr {
    Expr::new(
        ExprKind::FunctionCall {
            name: Name::from_parts(NameKind::FullyQualified, vec!["array_slice".to_string()]),
            args: vec![
                spread.clone(),
                Expr::new(ExprKind::IntLiteral(offset as i64), span),
            ],
        },
        span,
    )
}

/// Builds the runtime count of explicit prefix values plus one dynamic indexed spread.
fn positional_spread_actual_count_expr(
    spread: &Expr,
    prefix_len: usize,
    span: crate::span::Span,
) -> Expr {
    let spread_count = Expr::new(
        ExprKind::FunctionCall {
            name: Name::from_parts(NameKind::FullyQualified, vec!["count".to_string()]),
            args: vec![spread.clone()],
        },
        span,
    );
    if prefix_len == 0 {
        spread_count
    } else {
        Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(Expr::new(ExprKind::IntLiteral(prefix_len as i64), span)),
                op: BinOp::Add,
                right: Box::new(spread_count),
            },
            span,
        )
    }
}

/// Converts a sliced dynamic tail to the variadic slot's indexed storage representation.
fn coerce_spread_variadic_array(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    tail: LoweredValue,
    span: crate::span::Span,
) -> LoweredValue {
    let expected = variadic_array_type(sig);
    let actual = ctx.builder.value_php_type(tail.value).codegen_repr();
    let needs_mixed = matches!(expected.codegen_repr(), PhpType::Array(element) if element.codegen_repr() == PhpType::Mixed)
        && !matches!(actual, PhpType::Array(element) if element.codegen_repr() == PhpType::Mixed);
    if !needs_mixed {
        return tail;
    }
    ctx.emit_value(
        Op::ArrayToMixed,
        vec![tail.value],
        None,
        expected,
        Op::ArrayToMixed.default_effects(),
        Some(span),
    )
}

/// Returns the element count for a statically-known indexed spread source.
pub(super) fn static_indexed_spread_len(expr: &Expr) -> Option<usize> {
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => Some(items.len()),
        _ => None,
    }
}

/// Returns the index of a single trailing positional spread that EIR can materialize.
pub(super) fn single_trailing_indexed_spread_arg(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<usize> {
    let spread_indices = args
        .iter()
        .enumerate()
        .filter_map(|(idx, arg)| matches!(arg.kind, ExprKind::Spread(_)).then_some(idx))
        .collect::<Vec<_>>();
    let [spread_idx] = spread_indices.as_slice() else {
        return None;
    };
    if *spread_idx + 1 != args.len() {
        return None;
    }
    let ExprKind::Spread(inner) = &args[*spread_idx].kind else {
        return None;
    };
    indexed_spread_source_type(ctx, inner)?;
    Some(*spread_idx)
}

/// Returns the indexed-array source type for spread-only EIR lowering.
pub(super) fn indexed_spread_source_type(
    ctx: &LoweringContext<'_, '_>,
    expr: &Expr,
) -> Option<PhpType> {
    let ty = match &expr.kind {
        ExprKind::Variable(name) => ctx.local_type(name),
        ExprKind::ArrayLiteral(items) => array_literal_type_for_ir(ctx, items, expr),
        _ => infer_expr_type_syntactic(expr),
    }
    .codegen_repr();
    if matches!(ty, PhpType::Array(_)) {
        Some(ty)
    } else {
        None
    }
}

/// Returns how many spread elements must exist to satisfy required parameters.
pub(super) fn required_positional_spread_len(
    sig: &FunctionSig,
    start_param_idx: usize,
    regular_param_count: usize,
) -> usize {
    (start_param_idx..regular_param_count)
        .rfind(|idx| sig.defaults.get(*idx).and_then(|default| default.as_ref()).is_none())
        .map(|idx| idx - start_param_idx + 1)
        .unwrap_or(0)
}

/// Emits a fatal guard when a positional spread is shorter than required parameters.
pub(super) fn emit_positional_spread_min_len_guard(
    ctx: &mut LoweringContext<'_, '_>,
    spread: crate::ir::ValueId,
    min_len: usize,
    span: crate::span::Span,
) {
    if min_len == 0 {
        return;
    }
    let len = ctx.emit_value(
        Op::ArrayLen,
        vec![spread],
        None,
        PhpType::Int,
        Op::ArrayLen.default_effects(),
        Some(span),
    );
    let min = emit_i64_at_span(ctx, min_len as i64, span);
    let has_required_args = ctx.emit_value(
        Op::ICmp,
        vec![len.value, min.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Sge)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(span),
    );
    let ok = ctx.builder.create_named_block("call.spread.len.ok", Vec::new());
    let fatal = ctx.builder.create_named_block("call.spread.len.fatal", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: has_required_args.value,
        then_target: ok,
        then_args: Vec::new(),
        else_target: fatal,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(fatal);
    let message = ctx.intern_string("Fatal error: too few arguments for spread call\n");
    ctx.builder.terminate(Terminator::Fatal { message });

    ctx.builder.position_at_end(ok);
}
