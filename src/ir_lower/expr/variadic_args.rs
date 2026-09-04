//! Purpose:
//! Variadic containers and static call-spread expansion.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers one source call argument, unwrapping named syntax while preserving source position.
pub(super) fn lower_call_source_arg(ctx: &mut LoweringContext<'_, '_>, arg: &Expr) -> crate::ir::ValueId {
    match &arg.kind {
        ExprKind::NamedArg { value, .. } => lower_expr(ctx, value).value,
        _ => lower_expr(ctx, arg).value,
    }
}

/// Builds the variadic tail array for a named-argument call plan.
pub(super) fn lower_named_variadic_tail_array(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    tail: &[crate::types::call_args::PlannedSourceValue],
    source_values: &[crate::ir::ValueId],
) -> LoweredValue {
    if tail.iter().any(|source| source.key().is_some()) {
        return lower_named_variadic_tail_hash(ctx, sig, tail, source_values);
    }
    let span = tail
        .first()
        .map(|arg| arg.expr().span)
        .unwrap_or_else(crate::span::Span::dummy);
    let count_metadata = crate::func_args::sig_collects_optional_arg_count(sig);
    let variadic_count = tail.iter().filter(|source| source.param_idx().is_none()).count()
        + usize::from(count_metadata);
    let array_ty = variadic_array_type(sig);
    let array = ctx.emit_value(
        Op::ArrayNew,
        Vec::new(),
        Some(Immediate::Capacity(variadic_count as u32)),
        array_ty.clone(),
        Op::ArrayNew.default_effects(),
        Some(span),
    );
    let elem_ty = indexed_array_literal_element_type(&array_ty);
    let by_ref_variadic = variadic_param_is_by_ref(sig);
    if count_metadata {
        let actual_count = named_plan_actual_arg_count(tail);
        push_func_args_count(ctx, array.value, &array_ty, &elem_ty, actual_count, span);
    }
    for source in tail {
        if source.param_idx().is_some() {
            continue;
        }
        let value = lower_variadic_tail_source_value(
            ctx,
            source.expr(),
            by_ref_variadic,
            Some(source_values[source.source_index()]),
            &array_ty,
        );
        ctx.emit_void(
            Op::ArrayPush,
            vec![array.value, value.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(source.expr().span),
        );
        crate::ir_lower::stmt::release_indexed_array_write_operand(
            ctx,
            elem_ty.as_ref(),
            value,
            source.expr().span,
        );
    }
    array
}

/// Builds an associative variadic tail when unknown named args must keep string keys.
pub(super) fn lower_named_variadic_tail_hash(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    tail: &[crate::types::call_args::PlannedSourceValue],
    source_values: &[crate::ir::ValueId],
) -> LoweredValue {
    let span = tail
        .first()
        .map(|arg| arg.expr().span)
        .unwrap_or_else(crate::span::Span::dummy);
    let value_ty = variadic_tail_value_type(sig);
    let variadic_count = tail.iter().filter(|source| source.param_idx().is_none()).count();
    let hash_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(value_ty.clone()),
    };
    let hash = ctx.emit_value(
        Op::HashNew,
        Vec::new(),
        Some(Immediate::Capacity(variadic_count as u32)),
        hash_ty,
        Op::HashNew.default_effects(),
        Some(span),
    );
    let mut next_positional_key = 0usize;
    let by_ref_variadic = variadic_param_is_by_ref(sig);
    for source in tail {
        if source.param_idx().is_some() {
            continue;
        }
        let key = if let Some(key) = source.key() {
            lower_string_literal(ctx, key, source.expr())
        } else {
            let key = emit_i64_at_span(ctx, next_positional_key as i64, source.expr().span);
            next_positional_key += 1;
            key
        };
        let value = lower_variadic_tail_source_value(
            ctx,
            source.expr(),
            by_ref_variadic,
            Some(source_values[source.source_index()]),
            &PhpType::Array(Box::new(value_ty.clone())),
        );
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

/// Rebuilds lowering metadata for an already emitted value.
pub(super) fn lowered_value_from_id(
    ctx: &LoweringContext<'_, '_>,
    value: crate::ir::ValueId,
) -> LoweredValue {
    LoweredValue {
        value,
        ir_type: ctx.builder.value_type(value),
    }
}

/// Lowers the synthetic variadic tail array using the variadic parameter's storage type.
pub(super) fn lower_variadic_tail_array(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    tail: &[Expr],
    actual_count: usize,
) -> LoweredValue {
    let span = tail
        .first()
        .map(|arg| arg.span)
        .unwrap_or_else(crate::span::Span::dummy);
    let array_ty = variadic_array_type(sig);
    let array = ctx.emit_value(
        Op::ArrayNew,
        Vec::new(),
        Some(Immediate::Capacity(
            (tail.len()
                + usize::from(crate::func_args::sig_collects_optional_arg_count(sig)))
                as u32,
        )),
        array_ty.clone(),
        Op::ArrayNew.default_effects(),
        Some(span),
    );
    let elem_ty = indexed_array_literal_element_type(&array_ty);
    let by_ref_variadic = variadic_param_is_by_ref(sig);
    if crate::func_args::sig_collects_optional_arg_count(sig) {
        push_func_args_count(
            ctx,
            array.value,
            &array_ty,
            &elem_ty,
            actual_count,
            span,
        );
    }
    for item in tail {
        let value = lower_variadic_tail_source_value(ctx, item, by_ref_variadic, None, &array_ty);
        ctx.emit_void(
            Op::ArrayPush,
            vec![array.value, value.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(item.span),
        );
        crate::ir_lower::stmt::release_indexed_array_write_operand(ctx, elem_ty.as_ref(), value, item.span);
    }
    array
}

/// Computes the PHP positional count represented by a named call plan.
pub(super) fn named_plan_actual_arg_count(
    sources: &[crate::types::call_args::PlannedSourceValue],
) -> usize {
    let regular = sources
        .iter()
        .filter_map(|source| source.param_idx().map(|index| index + 1))
        .max()
        .unwrap_or(0);
    let surplus = sources
        .iter()
        .filter(|source| source.param_idx().is_none() && source.key().is_none())
        .count();
    regular + surplus
}

/// Appends the actual argument count as the first boxed Mixed collector element.
fn push_func_args_count(
    ctx: &mut LoweringContext<'_, '_>,
    array: crate::ir::ValueId,
    array_ty: &PhpType,
    elem_ty: &Option<PhpType>,
    actual_count: usize,
    span: crate::span::Span,
) {
    let count = emit_i64_at_span(ctx, actual_count as i64, span);
    let count = coerce_variadic_tail_value(ctx, count, array_ty, span);
    ctx.emit_void(
        Op::ArrayPush,
        vec![array, count.value],
        None,
        Op::ArrayPush.default_effects(),
        Some(span),
    );
    crate::ir_lower::stmt::release_indexed_array_write_operand(
        ctx,
        elem_ty.as_ref(),
        count,
        span,
    );
}

/// Lowers one value stored into a variadic tail container.
pub(super) fn lower_variadic_tail_source_value(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
    by_ref_variadic: bool,
    prelowered: Option<crate::ir::ValueId>,
    array_ty: &PhpType,
) -> LoweredValue {
    if by_ref_variadic {
        if let ExprKind::Variable(name) = &expr.kind {
            return lower_invoker_ref_arg_marker(ctx, name, expr.span);
        }
    }
    let value = prelowered
        .map(|value| lowered_value_from_id(ctx, value))
        .unwrap_or_else(|| lower_expr(ctx, expr));
    coerce_variadic_tail_value(ctx, value, array_ty, expr.span)
}

/// Returns whether the synthetic variadic parameter slot is by-reference.
pub(super) fn variadic_param_is_by_ref(sig: &FunctionSig) -> bool {
    let Some(variadic_name) = sig.variadic.as_ref() else {
        return false;
    };
    sig.params
        .iter()
        .position(|(name, _)| name == variadic_name)
        .and_then(|index| sig.ref_params.get(index))
        .copied()
        .unwrap_or(false)
}

/// Returns the element type expected inside a variadic tail container.
pub(super) fn variadic_tail_value_type(sig: &FunctionSig) -> PhpType {
    if variadic_param_is_by_ref(sig) {
        return PhpType::Mixed;
    }
    let Some(variadic_name) = sig.variadic.as_ref() else {
        return PhpType::Mixed;
    };
    sig.params
        .iter()
        .find(|(name, _)| name == variadic_name)
        .map(|(_, ty)| match ty.codegen_repr() {
            PhpType::Array(elem_ty) => variadic_container_element_type(*elem_ty),
            other => variadic_container_element_type(other),
        })
        .unwrap_or(PhpType::Mixed)
}

/// Returns the runtime array type used for a variadic parameter slot.
pub(super) fn variadic_array_type(sig: &FunctionSig) -> PhpType {
    if variadic_param_is_by_ref(sig) {
        return PhpType::Array(Box::new(PhpType::Mixed));
    }
    let Some(variadic_name) = sig.variadic.as_ref() else {
        return PhpType::Array(Box::new(PhpType::Mixed));
    };
    sig.params
        .iter()
        .find(|(name, _)| name == variadic_name)
        .map(|(_, ty)| match ty.codegen_repr() {
            PhpType::Array(elem_ty) => {
                PhpType::Array(Box::new(variadic_container_element_type(*elem_ty)))
            }
            other => PhpType::Array(Box::new(variadic_container_element_type(other))),
        })
        .unwrap_or_else(|| PhpType::Array(Box::new(PhpType::Mixed)))
}

/// Maps checker-only variadic container markers to their stored element type.
pub(super) fn variadic_container_element_type(ty: PhpType) -> PhpType {
    if matches!(ty, PhpType::Iterable) {
        PhpType::Mixed
    } else {
        ty
    }
}

/// Boxes variadic tail values when the callee expects an `array<mixed>` slot.
pub(super) fn coerce_variadic_tail_value(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    array_ty: &PhpType,
    span: crate::span::Span,
) -> LoweredValue {
    let PhpType::Array(elem_ty) = array_ty.codegen_repr() else {
        return value;
    };
    if elem_ty.codegen_repr() != PhpType::Mixed {
        return value;
    }
    if ctx.builder.value_php_type(value.value).codegen_repr() == PhpType::Mixed {
        return value;
    }
    ctx.box_value_as_mixed(value, PhpType::Mixed, Some(span))
}

/// Returns true when a call argument uses unpacking syntax.
pub(super) fn is_spread_arg(arg: &Expr) -> bool {
    matches!(arg.kind, ExprKind::Spread(_))
}

/// Returns true when a call contains any static spread that EIR can flatten before lowering.
pub(super) fn has_static_call_spread_args(args: &[Expr]) -> bool {
    has_static_indexed_spread_args(args) || has_static_assoc_spread_args(args)
}

/// Returns true when a call contains an indexed-array spread that EIR can flatten statically.
pub(super) fn has_static_indexed_spread_args(args: &[Expr]) -> bool {
    args.iter().any(|arg| match &arg.kind {
        ExprKind::Spread(inner) => matches!(inner.kind, ExprKind::ArrayLiteral(_)),
        _ => false,
    })
}

/// Returns true when a call contains an associative-array spread literal that can be flattened.
pub(super) fn has_static_assoc_spread_args(args: &[Expr]) -> bool {
    args.iter().any(|arg| match &arg.kind {
        ExprKind::Spread(inner) => matches!(inner.kind, ExprKind::ArrayLiteralAssoc(_)),
        _ => false,
    })
}

/// Flattens every statically-known call spread before EIR operand materialization.
pub(super) fn expand_static_call_spread_args(args: &[Expr]) -> Vec<Expr> {
    let assoc_expanded = crate::types::call_args::expand_static_assoc_spread_args(args);
    expand_static_indexed_spread_args(&assoc_expanded)
}

/// Flattens static indexed array spreads into positional call arguments.
pub(super) fn expand_static_indexed_spread_args(args: &[Expr]) -> Vec<Expr> {
    let mut expanded = Vec::new();
    for arg in args {
        match &arg.kind {
            ExprKind::Spread(inner) => {
                if let ExprKind::ArrayLiteral(items) = &inner.kind {
                    expanded.extend(items.iter().map(|value| {
                        Expr::new(value.kind.clone(), arg.span)
                    }));
                } else {
                    expanded.push(arg.clone());
                }
            }
            _ => expanded.push(arg.clone()),
        }
    }
    expanded
}
