//! Purpose:
//! Named spread bounds, associative reads, and variadic-tail setup.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Returns a static prefix length only for indexed array literals without nested spreads.
pub(super) fn static_indexed_variadic_prefix_len(prefix_expr: &Expr) -> Option<usize> {
    let ExprKind::ArrayLiteral(items) = &prefix_expr.kind else {
        return None;
    };
    if items.iter().any(|item| matches!(item.kind, ExprKind::Spread(_))) {
        return None;
    }
    Some(items.len())
}

/// Builds a variadic tail hash from static spread overflow plus later named variadics.
#[allow(clippy::too_many_arguments)]
pub(super) fn lower_named_spread_static_variadic_tail_hash(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    prefix_temp: &Expr,
    prefix_len: usize,
    regular_param_count: usize,
    plan: &crate::types::call_args::CallArgPlan,
    source_values: &[Option<crate::ir::ValueId>],
    first_named_pos: usize,
    span: crate::span::Span,
) -> LoweredValue {
    let value_ty = variadic_tail_value_type(sig);
    let prefix_tail_len = prefix_len.saturating_sub(regular_param_count);
    let named_tail_len = plan
        .source_values
        .iter()
        .filter(|source| source.source_index() >= first_named_pos && source.param_idx().is_none())
        .count();
    let hash_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(value_ty.clone()),
    };
    let hash = ctx.emit_value(
        Op::HashNew,
        Vec::new(),
        Some(Immediate::Capacity((prefix_tail_len + named_tail_len) as u32)),
        hash_ty,
        Op::HashNew.default_effects(),
        Some(span),
    );
    let array_ty = PhpType::Array(Box::new(value_ty.clone()));
    let mut next_positional_key = 0usize;
    for prefix_idx in regular_param_count..prefix_len {
        let key = emit_i64_at_span(ctx, next_positional_key as i64, span);
        next_positional_key += 1;
        let expr = spread_element_expr_for_ir(prefix_temp, prefix_idx, None, false, span);
        let value = lower_expr(ctx, &expr);
        let value = coerce_variadic_tail_value(ctx, value, &array_ty, span);
        ctx.emit_void(
            Op::HashSet,
            vec![hash.value, key.value, value.value],
            None,
            Op::HashSet.default_effects(),
            Some(span),
        );
    }
    for source in &plan.source_values {
        if source.source_index() < first_named_pos || source.param_idx().is_some() {
            continue;
        }
        let key = if let Some(key) = source.key() {
            lower_string_literal(ctx, key, source.expr())
        } else {
            let key = emit_i64_at_span(ctx, next_positional_key as i64, source.expr().span);
            next_positional_key += 1;
            key
        };
        let value = source_values[source.source_index()]
            .expect("named spread variadic source was not evaluated");
        let value = lowered_value_from_id(ctx, value);
        let value = coerce_variadic_tail_value(ctx, value, &array_ty, source.expr().span);
        ctx.emit_void(
            Op::HashSet,
            vec![hash.value, key.value, value.value],
            None,
            Op::HashSet.default_effects(),
            Some(source.expr().span),
        );
    }
    hash
}

/// Emits named-after-spread min/max checks against the already materialized prefix temp.
pub(super) fn emit_named_spread_bounds_guard(
    ctx: &mut LoweringContext<'_, '_>,
    spread: crate::ir::ValueId,
    check: &crate::types::call_args::SpreadBoundsCheck,
    span: crate::span::Span,
) {
    if check.min_len == 0 && check.max_len.is_none() {
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
    emit_named_spread_min_len_guard(ctx, len.value, check.min_len, span);
    emit_named_spread_max_len_guard(
        ctx,
        len.value,
        check.max_len,
        check.max_len_param_name.as_deref(),
        span,
    );
}

/// Emits the underflow branch for a named-after-spread bounds check.
pub(super) fn emit_named_spread_min_len_guard(
    ctx: &mut LoweringContext<'_, '_>,
    len: crate::ir::ValueId,
    min_len: usize,
    span: crate::span::Span,
) {
    if min_len == 0 {
        return;
    }
    let min = emit_i64_at_span(ctx, min_len as i64, span);
    let has_required_args = ctx.emit_value(
        Op::ICmp,
        vec![len, min.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Sge)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(span),
    );
    let ok = ctx.builder.create_named_block("call.named_spread.min.ok", Vec::new());
    let fatal = ctx.builder.create_named_block("call.named_spread.min.fatal", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: has_required_args.value,
        then_target: ok,
        then_args: Vec::new(),
        else_target: fatal,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(fatal);
    let message = ctx.intern_string("Fatal error: named argument spread length mismatch\n");
    ctx.builder.terminate(Terminator::Fatal { message });

    ctx.builder.position_at_end(ok);
}

/// Emits the overflow branch for a named-after-spread bounds check.
pub(super) fn emit_named_spread_max_len_guard(
    ctx: &mut LoweringContext<'_, '_>,
    len: crate::ir::ValueId,
    max_len: Option<usize>,
    param_name: Option<&str>,
    span: crate::span::Span,
) {
    let Some(max_len) = max_len else {
        return;
    };
    let max = emit_i64_at_span(ctx, max_len as i64, span);
    let within_bound = ctx.emit_value(
        Op::ICmp,
        vec![len, max.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Sle)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(span),
    );
    let ok = ctx.builder.create_named_block("call.named_spread.max.ok", Vec::new());
    let fatal = ctx.builder.create_named_block("call.named_spread.max.fatal", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: within_bound.value,
        then_target: ok,
        then_args: Vec::new(),
        else_target: fatal,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(fatal);
    let message = if let Some(param_name) = param_name {
        format!(
            "Fatal error: Named parameter ${} overwrites previous argument\n",
            param_name
        )
    } else {
        "Fatal error: named argument spread length mismatch\n".to_string()
    };
    let message = ctx.intern_string(&message);
    ctx.builder.terminate(Terminator::Fatal { message });

    ctx.builder.position_at_end(ok);
}

/// Lowers a single associative spread as named parameter reads by key.
pub(super) fn lower_assoc_spread_only_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    args: &[Expr],
) -> Option<Vec<crate::ir::ValueId>> {
    let [arg] = args else {
        return None;
    };
    let ExprKind::Spread(inner) = &arg.kind else {
        return None;
    };
    if !is_assoc_spread_source(ctx, inner) || sig.variadic.is_some() {
        return None;
    }
    let (spread_expr, cleanup_temp) = if matches!(&inner.kind, ExprKind::Variable(_)) {
        (inner.as_ref().clone(), None)
    } else {
        let spread = lower_expr(ctx, inner);
        let spread_type = ctx.builder.value_php_type(spread.value);
        let temp_name = ctx.declare_hidden_temp(spread_type.clone());
        store_value_into_temp(ctx, &temp_name, spread_type, spread, arg.span);
        (
            Expr::new(ExprKind::Variable(temp_name.clone()), inner.span),
            Some(temp_name),
        )
    };
    let mut operands = Vec::with_capacity(sig.params.len());
    for (idx, (param_name, _)) in sig.params.iter().enumerate() {
        let default = sig.defaults.get(idx).and_then(|default| default.as_ref());
        let param_expr = assoc_spread_param_expr(&spread_expr, param_name, default, arg.span);
        operands.push(lower_expr(ctx, &param_expr).value);
    }
    if let Some(cleanup_temp) = cleanup_temp {
        if let Some(anchor) = operands.first().copied() {
            ctx.register_call_arg_temp_cleanup(anchor, cleanup_temp);
        } else {
            ctx.clear_hidden_temp(&cleanup_temp, Some(arg.span));
        }
    }
    Some(operands)
}

/// Builds an expression that reads one named parameter from an associative spread.
pub(super) fn assoc_spread_param_expr(
    spread_expr: &Expr,
    param_name: &str,
    default: Option<&Expr>,
    span: crate::span::Span,
) -> Expr {
    let key = Expr::new(ExprKind::StringLiteral(param_name.to_string()), span);
    let access = Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(spread_expr.clone()),
            index: Box::new(key.clone()),
        },
        span,
    );
    let Some(default) = default else {
        return access;
    };
    Expr::new(
        ExprKind::Ternary {
            condition: Box::new(Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("array_key_exists"),
                    args: vec![key, spread_expr.clone()],
                },
                span,
            )),
            then_expr: Box::new(access),
            else_expr: Box::new(default.clone()),
        },
        span,
    )
}

/// Builds an expression that reads one materialized spread element from a hidden temp.
pub(super) fn spread_element_expr_for_ir(
    spread_expr: &Expr,
    element_idx: usize,
    param_name: Option<&str>,
    prefer_named_key: bool,
    span: crate::span::Span,
) -> Expr {
    let index = if prefer_named_key {
        param_name
            .map(|name| Expr::new(ExprKind::StringLiteral(name.to_string()), span))
            .unwrap_or_else(|| Expr::new(ExprKind::IntLiteral(element_idx as i64), span))
    } else {
        Expr::new(ExprKind::IntLiteral(element_idx as i64), span)
    };
    Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(spread_expr.clone()),
            index: Box::new(index),
        },
        span,
    )
}

/// Builds an expression that falls back to a default when a spread element is absent.
pub(super) fn spread_element_or_default_expr_for_ir(
    spread_expr: &Expr,
    element_idx: usize,
    param_name: Option<&str>,
    prefer_named_key: bool,
    default_expr: Expr,
    span: crate::span::Span,
) -> Expr {
    let condition = if prefer_named_key {
        if let Some(param_name) = param_name {
            Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("array_key_exists"),
                    args: vec![
                        Expr::new(ExprKind::StringLiteral(param_name.to_string()), span),
                        spread_expr.clone(),
                    ],
                },
                span,
            )
        } else {
            spread_len_gt_expr_for_ir(spread_expr, element_idx, span)
        }
    } else {
        spread_len_gt_expr_for_ir(spread_expr, element_idx, span)
    };
    Expr::new(
        ExprKind::Ternary {
            condition: Box::new(condition),
            then_expr: Box::new(spread_element_expr_for_ir(
                spread_expr,
                element_idx,
                param_name,
                prefer_named_key,
                span,
            )),
            else_expr: Box::new(default_expr),
        },
        span,
    )
}

/// Builds `count($spread) > element_idx` for optional spread-slot defaults.
pub(super) fn spread_len_gt_expr_for_ir(
    spread_expr: &Expr,
    element_idx: usize,
    span: crate::span::Span,
) -> Expr {
    Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("count"),
                    args: vec![spread_expr.clone()],
                },
                span,
            )),
            op: BinOp::Gt,
            right: Box::new(Expr::new(ExprKind::IntLiteral(element_idx as i64), span)),
        },
        span,
    )
}

/// Marks spread arguments whose source is known to be an associative array.
pub(super) fn assoc_spread_sources(ctx: &LoweringContext<'_, '_>, args: &[Expr]) -> Vec<bool> {
    crate::types::call_args::expand_static_assoc_spread_args(args)
        .iter()
        .map(|arg| match &arg.kind {
            ExprKind::Spread(inner) => is_assoc_spread_source(ctx, inner),
            _ => false,
        })
        .collect()
}

/// Returns true when a spread expression should feed named parameters by key.
pub(super) fn is_assoc_spread_source(ctx: &LoweringContext<'_, '_>, expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(name) => matches!(ctx.local_types.get(name), Some(PhpType::AssocArray { .. })),
        ExprKind::ArrayLiteralAssoc(_) => true,
        _ => matches!(infer_expr_type_syntactic(expr), PhpType::AssocArray { .. }),
    }
}
