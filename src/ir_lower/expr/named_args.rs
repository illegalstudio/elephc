//! Purpose:
//! Named argument planning and dynamic spread duplicate guards.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers named arguments with caller-selected trailing-default preservation.
pub(super) fn lower_named_args_with_signature_options(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    args: &[Expr],
    trim_trailing_defaults: bool,
    capture_values: bool,
) -> Vec<crate::ir::ValueId> {
    let call_span = args
        .first()
        .map(|arg| arg.span)
        .unwrap_or_else(crate::span::Span::dummy);
    let assoc_spread_sources = assoc_spread_sources(ctx, args);
    let regular_param_count = crate::types::call_args::regular_param_count(sig);
    let Ok(plan) = crate::types::call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
        sig,
        args,
        call_span,
        regular_param_count,
        trim_trailing_defaults,
        true,
        &assoc_spread_sources,
    ) else {
        return lower_args(ctx, args);
    };
    if plan.has_spread_args() {
        if let Some(operands) = lower_named_args_with_spread_plan(
            ctx,
            sig,
            &plan,
            &assoc_spread_sources,
            capture_values,
        ) {
            return operands;
        }
        if let Some(operands) = lower_dynamic_named_spread_variadic_args(ctx, sig, &plan, capture_values) {
            return operands;
        }
        let normalized = plan.normalized_args();
        return lower_args(ctx, &normalized);
    }
    let mut source_values = Vec::with_capacity(plan.source_args.len());
    for (index, source_arg) in plan.source_args.iter().enumerate() {
        source_values.push(lower_planned_source_arg(ctx, sig, &plan, index, source_arg, capture_values));
    }

    let mut operands = Vec::with_capacity(plan.regular_args.len() + usize::from(sig.variadic.is_some()));
    for arg in &plan.regular_args {
        match arg {
            crate::types::call_args::PlannedRegularArg::Source { source_index, .. } => {
                operands.push(source_values[*source_index]);
            }
            crate::types::call_args::PlannedRegularArg::Default(default) => {
                operands.push(lower_expr(ctx, default).value);
            }
            crate::types::call_args::PlannedRegularArg::SpreadElement { .. } => {
                return lower_args(ctx, args);
            }
        }
    }
    if sig.variadic.is_some() {
        operands.push(lower_named_variadic_tail_array(ctx, sig, &plan.source_values, &source_values).value);
    }
    operands
}

/// Lowers dynamic associative prefix spreads for variadic calls far enough to preserve duplicate fatals.
pub(super) fn lower_dynamic_named_spread_variadic_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    plan: &crate::types::call_args::CallArgPlan,
    capture_values: bool,
) -> Option<Vec<crate::ir::ValueId>> {
    if sig.variadic.is_none() || !plan.prefix_has_dynamic_named_spread {
        return None;
    }
    let call_span = plan
        .source_args
        .first()
        .map(|arg| arg.span)
        .unwrap_or_else(crate::span::Span::dummy);
    let first_named_pos = plan.first_named_pos?;
    let prefix_expr = plan.positional_prefix_expr(call_span)?;
    let prefix = lower_expr(ctx, &prefix_expr);
    if !matches!(ctx.builder.value_php_type(prefix.value).codegen_repr(), PhpType::AssocArray { .. }) {
        return None;
    }
    let prefix_type = ctx.builder.value_php_type(prefix.value);
    let prefix_temp_name = ctx.declare_hidden_temp(prefix_type.clone());
    store_value_into_temp(ctx, &prefix_temp_name, prefix_type, prefix, prefix_expr.span);
    let prefix_temp = Expr::new(ExprKind::Variable(prefix_temp_name), prefix_expr.span);

    let mut source_values = vec![None; plan.source_args.len()];
    for (source_index, source_arg) in plan.source_args.iter().enumerate().skip(first_named_pos) {
        if matches!(source_arg.kind, ExprKind::Spread(_)) {
            return None;
        }
        source_values[source_index] = Some(lower_planned_source_arg(ctx, sig, plan, source_index, source_arg, capture_values));
    }
    emit_dynamic_named_prefix_duplicate_guards(ctx, sig, plan, &prefix_temp, first_named_pos);

    let mut operands = Vec::with_capacity(plan.regular_args.len() + 1);
    for arg in &plan.regular_args {
        match arg {
            crate::types::call_args::PlannedRegularArg::Source { source_index, .. } => {
                operands.push(source_values.get(*source_index).copied().flatten()?);
            }
            crate::types::call_args::PlannedRegularArg::Default(default) => {
                operands.push(lower_expr(ctx, default).value);
            }
            crate::types::call_args::PlannedRegularArg::SpreadElement {
                prefix_element_idx,
                param_name,
                prefer_named_key,
                default,
                guaranteed_present,
                spread_span,
                ..
            } => {
                let expr = if let Some(default) = default {
                    if *guaranteed_present {
                        spread_element_expr_for_ir(
                            &prefix_temp,
                            *prefix_element_idx,
                            param_name.as_deref(),
                            *prefer_named_key,
                            *spread_span,
                        )
                    } else {
                        spread_element_or_default_expr_for_ir(
                            &prefix_temp,
                            *prefix_element_idx,
                            param_name.as_deref(),
                            *prefer_named_key,
                            default.clone(),
                            *spread_span,
                        )
                    }
                } else {
                    spread_element_expr_for_ir(
                        &prefix_temp,
                        *prefix_element_idx,
                        param_name.as_deref(),
                        *prefer_named_key,
                        *spread_span,
                    )
                };
                operands.push(lower_expr(ctx, &expr).value);
            }
        }
    }
    operands.push(lower_variadic_tail_array(ctx, sig, &[]).value);
    Some(operands)
}

/// Emits duplicate checks for numeric prefix keys overwritten by later named parameters.
pub(super) fn emit_dynamic_named_prefix_duplicate_guards(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    plan: &crate::types::call_args::CallArgPlan,
    prefix_temp: &Expr,
    first_named_pos: usize,
) {
    for source in &plan.source_values {
        if source.source_index() < first_named_pos {
            continue;
        }
        let Some(param_idx) = source.param_idx() else {
            continue;
        };
        let Some((param_name, _)) = sig.params.get(param_idx) else {
            continue;
        };
        emit_dynamic_named_prefix_duplicate_guard(
            ctx,
            prefix_temp,
            param_idx,
            param_name,
            source.expr().span,
        );
    }
}

/// Emits one duplicate guard for a numeric key in a dynamic associative prefix.
pub(super) fn emit_dynamic_named_prefix_duplicate_guard(
    ctx: &mut LoweringContext<'_, '_>,
    prefix_temp: &Expr,
    param_idx: usize,
    param_name: &str,
    span: crate::span::Span,
) {
    let exists_expr = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("array_key_exists"),
            args: vec![
                Expr::new(ExprKind::IntLiteral(param_idx as i64), span),
                prefix_temp.clone(),
            ],
        },
        span,
    );
    let exists = lower_expr(ctx, &exists_expr);
    let ok = ctx.builder.create_named_block("call.dynamic_named_prefix.ok", Vec::new());
    let fatal = ctx.builder.create_named_block("call.dynamic_named_prefix.fatal", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: exists.value,
        then_target: fatal,
        then_args: Vec::new(),
        else_target: ok,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(fatal);
    let message = format!(
        "Fatal error: Named parameter ${} overwrites previous argument\n",
        param_name
    );
    let message = ctx.intern_string(&message);
    ctx.builder.terminate(Terminator::Fatal { message });

    ctx.builder.position_at_end(ok);
}

/// Lowers named/spread argument plans without re-evaluating dynamic spread expressions.
pub(super) fn lower_named_args_with_spread_plan(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    plan: &crate::types::call_args::CallArgPlan,
    assoc_spread_sources: &[bool],
    capture_values: bool,
) -> Option<Vec<crate::ir::ValueId>> {
    lower_named_args_with_spread_plan_impl(
        ctx,
        sig,
        plan,
        assoc_spread_sources,
        capture_values,
        &mut |_, _, _| None,
    )
}

/// `lower_named_args_with_spread_plan` with a per-source override: `hinted` sees each
/// named source's parameter index and value expression before the default lowering and
/// may lower it itself (the xml handler setters type a closure literal from its slot's
/// SAX event this way). Sources still lower once each, in source order, after the
/// positional prefix (spreads included) was evaluated once into a temp.
pub(super) fn lower_named_args_with_spread_plan_hinted(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    plan: &crate::types::call_args::CallArgPlan,
    assoc_spread_sources: &[bool],
    hinted: &mut dyn FnMut(&mut LoweringContext<'_, '_>, usize, &Expr) -> Option<crate::ir::ValueId>,
) -> Option<Vec<crate::ir::ValueId>> {
    lower_named_args_with_spread_plan_impl(ctx, sig, plan, assoc_spread_sources, false, hinted)
}

/// Applies optional per-parameter hints while preserving caller-selected value capture.
fn lower_named_args_with_spread_plan_impl(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    plan: &crate::types::call_args::CallArgPlan,
    assoc_spread_sources: &[bool],
    capture_values: bool,
    hinted: &mut dyn FnMut(&mut LoweringContext<'_, '_>, usize, &Expr) -> Option<crate::ir::ValueId>,
) -> Option<Vec<crate::ir::ValueId>> {
    if assoc_spread_sources.iter().any(|is_assoc| *is_assoc) {
        return None;
    }
    let call_span = plan
        .source_args
        .first()
        .map(|arg| arg.span)
        .unwrap_or_else(crate::span::Span::dummy);
    let first_named_pos = plan.first_named_pos?;
    let prefix_expr = plan.positional_prefix_expr(call_span)?;
    let static_variadic_prefix_len = static_indexed_variadic_prefix_len(&prefix_expr);
    if sig.variadic.is_some() && static_variadic_prefix_len.is_none() {
        return None;
    }
    let prefix = lower_expr(ctx, &prefix_expr);
    let prefix_type = ctx.builder.value_php_type(prefix.value);
    let prefix_temp_name = ctx.declare_hidden_temp(prefix_type.clone());
    store_value_into_temp(ctx, &prefix_temp_name, prefix_type, prefix, prefix_expr.span);
    let single_prefix_spread = !matches!(prefix_expr.kind, ExprKind::ArrayLiteral(_));
    let prefix_temp = Expr::new(ExprKind::Variable(prefix_temp_name), prefix_expr.span);

    let mut source_values = vec![None; plan.source_args.len()];
    for (source_index, source_arg) in plan.source_args.iter().enumerate().skip(first_named_pos) {
        if matches!(source_arg.kind, ExprKind::Spread(_)) {
            return None;
        }
        // `Regular.expr` is the named argument's value, already unwrapped by the planner.
        let planned = plan
            .source_values
            .iter()
            .find(|source| source.source_index() == source_index)
            .and_then(|source| Some((source.param_idx()?, source.expr())));
        let value = match planned {
            Some((param_idx, expr)) => {
                if let Some(value) = hinted(ctx, param_idx, expr) {
                    value
                } else {
                    lower_planned_source_arg(
                        ctx,
                        sig,
                        plan,
                        source_index,
                        source_arg,
                        capture_values,
                    )
                }
            }
            None => lower_planned_source_arg(
                ctx,
                sig,
                plan,
                source_index,
                source_arg,
                capture_values,
            ),
        };
        source_values[source_index] = Some(value);
    }
    if single_prefix_spread {
        if let [check] = plan.spread_bounds_checks.as_slice() {
            let prefix_value = lower_expr(ctx, &prefix_temp);
            emit_named_spread_bounds_guard(ctx, prefix_value.value, check, call_span);
        }
    }

    let mut operands = Vec::with_capacity(plan.regular_args.len());
    for (param_idx, arg) in plan.regular_args.iter().enumerate() {
        match arg {
            crate::types::call_args::PlannedRegularArg::Source { source_index, .. } => {
                if *source_index < first_named_pos {
                    let expr = spread_element_expr_for_ir(
                        &prefix_temp,
                        param_idx,
                        None,
                        false,
                        plan.source_args.get(*source_index).map(|arg| arg.span).unwrap_or(call_span),
                    );
                    operands.push(lower_expr(ctx, &expr).value);
                } else {
                    operands.push(source_values.get(*source_index).copied().flatten()?);
                }
            }
            crate::types::call_args::PlannedRegularArg::Default(default) => {
                operands.push(lower_expr(ctx, default).value);
            }
            crate::types::call_args::PlannedRegularArg::SpreadElement {
                element_idx: _,
                prefix_element_idx,
                param_name,
                prefer_named_key,
                default,
                guaranteed_present,
                spread_span,
                ..
            } => {
                let element_idx = *prefix_element_idx;
                let expr = if let Some(default) = default {
                    if *guaranteed_present {
                        spread_element_expr_for_ir(
                            &prefix_temp,
                            element_idx,
                            param_name.as_deref(),
                            *prefer_named_key,
                            *spread_span,
                        )
                    } else {
                        spread_element_or_default_expr_for_ir(
                            &prefix_temp,
                            element_idx,
                            param_name.as_deref(),
                            *prefer_named_key,
                            default.clone(),
                            *spread_span,
                        )
                    }
                } else {
                    spread_element_expr_for_ir(
                        &prefix_temp,
                        element_idx,
                        param_name.as_deref(),
                        *prefer_named_key,
                        *spread_span,
                    )
                };
                operands.push(lower_expr(ctx, &expr).value);
            }
        }
    }
    if sig.variadic.is_some() {
        let regular_param_count = crate::types::call_args::regular_param_count(sig);
        let tail = lower_named_spread_static_variadic_tail_hash(
            ctx,
            sig,
            &prefix_temp,
            static_variadic_prefix_len.unwrap_or(regular_param_count),
            regular_param_count,
            plan,
            &source_values,
            first_named_pos,
            call_span,
        );
        operands.push(tail.value);
    }
    Some(operands)
}

/// Captures a planned source value while preserving the original lvalue for reference parameters.
fn lower_planned_source_arg(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    plan: &crate::types::call_args::CallArgPlan,
    source_index: usize,
    arg: &Expr,
    capture_values: bool,
) -> ValueId {
    let parameter = plan.source_values.iter().find(|source| source.source_index() == source_index)
        .and_then(|source| source.param_idx());
    let by_ref = parameter.is_some_and(|index| sig.ref_params.get(index).copied().unwrap_or(false));
    if capture_values && by_ref {
        promote_captured_reference_argument(ctx, arg);
    }
    let value = lower_call_source_arg(ctx, arg);
    if capture_values && !by_ref {
        let lowered = lowered_value_from_id(ctx, value);
        capture_call_argument_value(ctx, lowered, parameter.unwrap_or(sig.params.len() + source_index), arg.span).value
    } else { value }
}
