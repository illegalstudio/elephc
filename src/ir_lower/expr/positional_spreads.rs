//! Purpose:
//! Positional spread lowering and arity guards.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Selects the PHP exception for an internal spread argument overflow.
#[derive(Clone, Copy)]
pub(super) enum SpreadOverflowError<'a> {
    /// A legacy internal overload reports a TypeError with its specific message.
    Overload(&'a str),
    /// A fixed-arity builtin reports an ArgumentCountError.
    Builtin(&'a str),
}

/// Lowers one trailing indexed spread in a fixed-arity positional call.
pub(super) fn lower_positional_spread_args_with_signature(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    args: &[Expr],
    spread_overflow_error: Option<SpreadOverflowError<'_>>,
) -> Option<Vec<crate::ir::ValueId>> {
    if sig.variadic.is_some() {
        return None;
    }
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
    let (spread_expr, cleanup_temp) = if matches!(&inner.kind, ExprKind::Variable(_)) {
        (inner.as_ref().clone(), None)
    } else {
        let spread = lower_expr(ctx, inner);
        let temp_name = ctx.declare_hidden_temp(spread_type.clone());
        store_value_into_temp(ctx, &temp_name, spread_type, spread, args[spread_idx].span);
        (
            Expr::new(ExprKind::Variable(temp_name.clone()), inner.span),
            Some(temp_name),
        )
    };
    let spread_value = lower_expr(ctx, &spread_expr);
    emit_positional_spread_min_len_guard_with_context(
        ctx,
        spread_value.value,
        required_len,
        match spread_overflow_error {
            Some(SpreadOverflowError::Builtin(name)) => Some(name),
            _ => None,
        },
        spread_idx,
        cleanup_temp.as_deref(),
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

    if let Some(cleanup_temp) = cleanup_temp {
        if let Some(anchor) = operands.first().copied() {
            ctx.register_call_arg_temp_cleanup(anchor, cleanup_temp.clone());
        } else {
            ctx.clear_hidden_temp(&cleanup_temp, Some(args[spread_idx].span));
        }
        if let Some(message) = spread_overflow_error {
            emit_positional_spread_max_len_error_guard(
                ctx,
                spread_value.value,
                regular_param_count - spread_idx,
                spread_idx,
                message,
                Some(&cleanup_temp),
                args[spread_idx].span,
            );
        }
    } else if let Some(message) = spread_overflow_error {
        emit_positional_spread_max_len_error_guard(
            ctx,
            spread_value.value,
            regular_param_count - spread_idx,
            spread_idx,
            message,
            None,
            args[spread_idx].span,
        );
    }
    Some(operands)
}

/// Throws the caller's PHP arity/overload exception when a runtime spread has excess entries.
pub(super) fn emit_positional_spread_max_len_error_guard(
    ctx: &mut LoweringContext<'_, '_>,
    spread: crate::ir::ValueId,
    max_len: usize,
    supplied_prefix: usize,
    error: SpreadOverflowError<'_>,
    cleanup_temp: Option<&str>,
    span: Span,
) {
    let len = ctx.emit_value(
        Op::ArrayLen,
        vec![spread],
        None,
        PhpType::Int,
        Op::ArrayLen.default_effects(),
        Some(span),
    );
    let max = emit_i64_at_span(ctx, max_len as i64, span);
    let within_bound = ctx.emit_value(
        Op::ICmp,
        vec![len.value, max.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Sle)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(span),
    );
    let valid = ctx
        .builder
        .create_named_block("call.spread.max.valid", Vec::new());
    let invalid = ctx
        .builder
        .create_named_block("call.spread.max.invalid", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: within_bound.value,
        then_target: valid,
        then_args: Vec::new(),
        else_target: invalid,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(invalid);
    if let Some(cleanup_temp) = cleanup_temp {
        ctx.clear_hidden_temp(cleanup_temp, Some(span));
    }
    match error {
        SpreadOverflowError::Overload(message) => {
            emit_exception_and_terminate(ctx, "TypeError", message, span);
        }
        SpreadOverflowError::Builtin(name) => {
            emit_builtin_spread_arity_error(ctx, name, len, supplied_prefix, false, span);
        }
    }
    ctx.builder.position_at_end(valid);
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
        ExprKind::FunctionCall { name, .. } => ctx
            .functions
            .get(name.as_str())
            .map(eir_user_function_return_type)
            .unwrap_or_else(|| infer_expr_type_syntactic(expr)),
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

/// Checks required entries while retaining builtin identity and caller-owned temp cleanup.
fn emit_positional_spread_min_len_guard_with_context(
    ctx: &mut LoweringContext<'_, '_>,
    spread: crate::ir::ValueId,
    min_len: usize,
    builtin: Option<&str>,
    supplied_prefix: usize,
    cleanup_temp: Option<&str>,
    span: Span,
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
    if let Some(cleanup_temp) = cleanup_temp {
        ctx.clear_hidden_temp(cleanup_temp, Some(span));
    }
    if let Some(name) = builtin {
        emit_builtin_spread_arity_error(ctx, name, len, supplied_prefix, true, span);
    } else {
        emit_exception_and_terminate(ctx, "ArgumentCountError", "Too few arguments for spread call", span);
    }

    ctx.builder.position_at_end(ok);
}

/// Emits PHP's exact internal arity wording with the actual runtime argument count.
fn emit_builtin_spread_arity_error(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    spread_len: LoweredValue,
    supplied_prefix: usize,
    too_few: bool,
    span: Span,
) {
    use crate::synthetic_class::{e_binop, e_int, e_str, e_var};
    let contract = elephc_builtin_contract::lookup(name)
        .expect("a registry-backed builtin has a shared contract");
    let minimum = contract.min_args.unwrap_or_else(|| {
        contract.params.iter().take_while(|param| param.default.is_none()).count()
    });
    let maximum = contract.max_args.unwrap_or(contract.params.len());
    let (qualifier, expected) = if minimum == maximum {
        ("exactly", minimum)
    } else if too_few {
        ("at least", minimum)
    } else {
        ("at most", maximum)
    };
    let plural = if expected == 1 { "" } else { "s" };
    let count_temp = ctx.declare_hidden_temp(PhpType::Int);
    store_value_into_temp(ctx, &count_temp, PhpType::Int, spread_len, span);
    let count = if supplied_prefix == 0 {
        e_var(&count_temp)
    } else {
        e_binop(e_var(&count_temp), BinOp::Add, e_int(supplied_prefix as i64))
    };
    let message = e_binop(
        e_binop(
            e_str(&format!("{}() expects {qualifier} {expected} argument{plural}, ", contract.name)),
            BinOp::Concat,
            count,
        ),
        BinOp::Concat,
        e_str(" given"),
    );
    emit_exception_from_expr(ctx, "ArgumentCountError", message, span);
    ctx.builder.terminate(Terminator::Unreachable);
}
