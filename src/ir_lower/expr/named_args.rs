//! Purpose:
//! Named argument planning and dynamic spread duplicate guards.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers named builtin arguments without inventing a user-function variadic collector.
///
/// Without spreads, valid named builtin calls have no variadic entries: positional values precede
/// named regular parameters, and the checker rejects unknown names. The shared planner still owns
/// source evaluation and parameter ordering, using a storage signature with no empty collector.
pub(super) fn lower_named_builtin_args_with_signature(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    let Some(sig) = sig else { return lower_args(ctx, args); };
    let mut storage = sig.clone();
    if storage.variadic.is_some() {
        let regular = crate::types::call_args::regular_param_count(sig);
        storage.params.truncate(regular);
        storage.param_type_exprs.truncate(regular);
        storage.param_attributes.truncate(regular);
        storage.ref_params.truncate(regular);
        storage.declared_params.truncate(regular);
        storage.defaults.truncate(regular);
        storage.variadic = None;
    }
    lower_args_with_signature(ctx, Some(&storage), args)
}

/// Records the class-like parameter check of one evaluated source, through the planner's
/// source-to-parameter mapping (which also covers statically unpacked named keys).
fn record_planned_object_param_source(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    plan: &crate::types::call_args::CallArgPlan,
    source_index: usize,
    value: crate::ir::ValueId,
) -> crate::ir::ValueId {
    let Some(source) = plan.source_values.iter().find(|source| source.source_index() == source_index)
    else {
        return value;
    };
    match source.param_idx() {
        Some(param_index) => super::call_arg_coercion::record_object_param_source(
            ctx, sig, param_index, value, source.expr(),
        ),
        None => value,
    }
}

/// Lowers named arguments with caller-selected trailing-default preservation.
pub(super) fn lower_named_args_with_signature_options(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    args: &[Expr],
    trim_trailing_defaults: bool,
    capture_values: bool,
    capture_output_index: Option<(usize, PhpType)>,
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
            capture_output_index.as_ref(),
        ) {
            return operands;
        }
        if let Some(operands) = lower_dynamic_named_spread_variadic_args(ctx, sig, &plan, capture_values) {
            return operands;
        }
        let normalized = plan.normalized_args();
        return lower_normalized_args(ctx, sig, &plan, &normalized);
    }
    let mut source_values = Vec::with_capacity(plan.source_args.len());
    for source_index in 0..plan.source_args.len() {
        let previous = capture_output_index.as_ref().and_then(|(output_index, ty)| {
            plan.source_values.iter().find(|source| {
                source.source_index() == source_index && source.param_idx() == Some(*output_index)
            }).and_then(|source| if capture_values {
                promote_captured_reference_argument(ctx, source.expr()); None
            } else { prepare_captured_output_argument(ctx, source.expr(), ty) })
        });
        let value = lower_planned_source_arg(ctx, sig, &plan, source_index, capture_values);
        source_values.push(value);
        if let Some((name, ty)) = previous { ctx.set_local_logical_type(&name, ty); }
    }

    if plan.regular_args.iter()
        .any(|arg| matches!(arg, crate::types::call_args::PlannedRegularArg::SpreadElement { .. }))
    {
        return lower_args(ctx, args);
    }
    // PHP fills an omitted parameter that precedes a supplied one before the callee checks any
    // parameter, and evaluates the trailing omitted ones after the checks. A refused argument
    // therefore throws its `TypeError` before a throwing trailing default (`int $n = 1 % 0`)
    // runs, but after a throwing default declared before the last supplied parameter.
    let throwing_default = !capture_values && plan.regular_args.iter().any(|arg| matches!(arg,
        crate::types::call_args::PlannedRegularArg::Default(default)
            if !super::call_arg_coercion::default_is_inert(default)));
    let mut leading_defaults = vec![None; plan.regular_args.len()];
    if throwing_default {
        check_supplied_scalar_params_before_defaults(
            ctx, sig, &plan, &mut source_values, &mut leading_defaults,
        );
    }
    let mut operands = Vec::with_capacity(plan.regular_args.len() + usize::from(sig.variadic.is_some()));
    for (param_index, arg) in plan.regular_args.iter().enumerate() {
        match arg {
            crate::types::call_args::PlannedRegularArg::Source { source_index, .. } => {
                operands.push(source_values[*source_index]);
            }
            crate::types::call_args::PlannedRegularArg::Default(_) if leading_defaults[param_index].is_some() => {
                operands.push(match leading_defaults[param_index].take().expect("leading default lowered") {
                    LeadingDefault::Rooted(value) => value,
                    // The checks are done: retire the pin without releasing, and pass the
                    // default as the ordinary pass would have, owned by the call as before.
                    LeadingDefault::Pinned(value, pins) => {
                        let span = ctx.call_site_span.unwrap_or_else(Span::dummy);
                        super::call_operand_owners::unpin_in_flight_owners(ctx, pins, span);
                        value
                    }
                });
            }
            crate::types::call_args::PlannedRegularArg::Default(default) => {
                operands.push(lower_arg_with_signature(ctx, sig, param_index, default));
            }
            crate::types::call_args::PlannedRegularArg::SpreadElement { .. } => unreachable!(),
        }
    }
    if sig.variadic.is_some() {
        if crate::func_args::sig_has_hidden_argc_param(sig) {
            let actual_count = named_plan_actual_arg_count(&plan.source_values);
            operands.push(emit_i64_at_span(ctx, actual_count as i64, call_span).value);
        }
        operands.push(lower_named_variadic_tail_array(ctx, sig, &plan.source_values, &source_values).value);
    }
    // Named arguments are lowered in source order and reordered into parameter order here.
    // Apply the same object-union ABI guard the positional path uses before the callee sees
    // the operand, using the original source expression for the TypeError text.
    for (param_index, arg) in plan.regular_args.iter().enumerate() {
        let crate::types::call_args::PlannedRegularArg::Source { source_index, .. } = arg else {
            continue;
        };
        let source = &plan.source_args[*source_index];
        let value = lowered_value_from_id(ctx, operands[param_index]);
        if let Some(guarded) =
            super::call_arg_coercion::guard_object_param_argument(ctx, sig, param_index, value, source)
        {
            operands[param_index] = guarded.value;
        }
    }
    operands
}

/// An omitted parameter's default lowered ahead of the supplied arguments' checks.
#[derive(Clone)]
enum LeadingDefault {
    /// A by-value default, rooted across the checks and passed as is.
    Rooted(crate::ir::ValueId),
    /// A by-reference default that can throw, kept as the call's operand and pinned (without a
    /// retain) in the unwind chain while the checks run.
    Pinned(crate::ir::ValueId, Vec<super::call_operand_owners::PinnedInFlightOwner>),
}

/// Runs the runtime scalar-parameter checks of the supplied named arguments before an omitted
/// trailing default that can throw is evaluated.
///
/// Every supplied value is rooted first, so a refusal (or the throwing default afterwards)
/// releases them through the unwind chain. PHP fills the omitted parameters declared before the
/// last supplied one before it checks any parameter (`f(b: $null)` for
/// `f(int $a = 1 % 0, string $b)` throws `DivisionByZeroError`), so those defaults are lowered
/// (and rooted) into `leading_defaults` ahead of the checks. An `int`/`float` check leaves the
/// converted scalar in place and a `string` is converted right after its check, in parameter
/// order as PHP binds them; a `bool` operand stays boxed and the call's ordinary coercion
/// converts it.
fn check_supplied_scalar_params_before_defaults(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    plan: &crate::types::call_args::CallArgPlan,
    source_values: &mut [crate::ir::ValueId],
    leading_defaults: &mut [Option<LeadingDefault>],
) {
    use super::scalar_param_guards::GuardSource;
    let supplied = plan.regular_args.iter().enumerate().filter_map(|(param_index, arg)| match arg {
        crate::types::call_args::PlannedRegularArg::Source { source_index, .. } => Some((param_index, *source_index)),
        _ => None,
    }).collect::<Vec<_>>();
    if !supplied.iter().any(|(param_index, source_index)| super::scalar_param_guards::guarded_scalar_param(
        ctx, sig, *param_index, source_values[*source_index], GuardSource::Unrecorded,
    ).is_some()) {
        return;
    }
    // Every owned source is rooted before the first check can throw: the regular parameters'
    // values and the extra named arguments a variadic collects alike (`extra: make()` would
    // otherwise leak its fresh array when the check for `$s` refuses).
    let regular_param_count = crate::types::call_args::regular_param_count(sig);
    for source in &plan.source_values {
        let param = source.param_idx().unwrap_or(regular_param_count);
        if sig.ref_params.get(param).copied().unwrap_or(false) {
            continue;
        }
        let source_index = source.source_index();
        let lowered = lowered_value_from_id(ctx, source_values[source_index]);
        source_values[source_index] =
            super::call_operand_owners::root_evaluated_call_argument(ctx, lowered, Span::dummy()).value;
    }
    let last_supplied = supplied.last().map_or(0, |(param_index, _)| *param_index);
    for (param_index, arg) in plan.regular_args.iter().enumerate().take(last_supplied) {
        let crate::types::call_args::PlannedRegularArg::Default(default) = arg else { continue };
        if sig.ref_params.get(param_index).copied().unwrap_or(false) {
            // A by-reference parameter's default that cannot throw stays in the ordinary
            // operand pass: its evaluation order is unobservable. One that can throw
            // (`int &$a = 1 % 0`) runs now, as PHP fills it before any check. It stays the
            // call's own operand and is only pinned over the checks, so a refusal releases it
            // and the call then binds and releases it as before. Rooting it in a temporary
            // instead made the call bind that temporary, whose root did not survive into the
            // call (a throwing callee leaked it).
            if super::call_arg_coercion::default_is_inert(default) {
                continue;
            }
            let value = lower_arg_with_signature(ctx, sig, param_index, default);
            let pins = super::call_operand_owners::pin_in_flight_owners(ctx, &[value], Span::dummy());
            leading_defaults[param_index] = Some(LeadingDefault::Pinned(value, pins));
            continue;
        }
        let value = lower_arg_with_signature(ctx, sig, param_index, default);
        let lowered = lowered_value_from_id(ctx, value);
        leading_defaults[param_index] = Some(LeadingDefault::Rooted(
            super::call_operand_owners::root_evaluated_call_argument(ctx, lowered, Span::dummy()).value,
        ));
    }
    let guard_span = ctx.call_site_span.unwrap_or_else(Span::dummy);
    for (param_index, source_index) in supplied {
        let value = source_values[source_index];
        if super::scalar_param_guards::guarded_scalar_param(ctx, sig, param_index, value, GuardSource::Unrecorded).is_none() {
            continue;
        }
        // An earlier by-reference property argument's check comes first, as in the final
        // operand pass (`f(a: $o->s, s: $bad)` for `f(int &$a, string $s)` refuses `$a`
        // without converting `$s`).
        super::call_arg_coercion::emit_property_type_guards_before(ctx, param_index);
        let lowered = lowered_value_from_id(ctx, value);
        if let Some(converted) = super::scalar_param_guards::guard_scalar_param(
            ctx, sig, param_index, lowered, GuardSource::Unrecorded, guard_span, false, &[],
        ) {
            source_values[source_index] = converted.value;
        } else if sig.params[param_index].1.codegen_repr() == PhpType::Str {
            // PHP binds (converts) each parameter before it checks the next: an accepted
            // `Stringable` runs `__toString()` now, so its side effects and its own exception
            // come before a later parameter's `TypeError`. The string stays rooted while the
            // defaults run.
            let string = coerce_to_string_at_span(ctx, lowered, Some(guard_span));
            source_values[source_index] =
                super::call_operand_owners::root_evaluated_call_argument(ctx, string, Span::dummy()).value;
        }
    }
}

/// Lowers a plan's normalized arguments, giving each regular parameter's literal its declared type.
///
/// Like `lower_args`, every argument but the last is rooted across the later evaluation; the
/// regular entries additionally use their parameter destination for pointer literals and
/// checked integer arithmetic, as positional arguments do. Variadic tail entries lower as
/// plain values.
fn lower_normalized_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    plan: &crate::types::call_args::CallArgPlan,
    normalized: &[Expr],
) -> Vec<crate::ir::ValueId> {
    let regular_count = plan.normalized_regular_arg_count();
    normalized
        .iter()
        .enumerate()
        .map(|(index, arg)| {
            let value = match sig.params.get(index).filter(|_| index < regular_count) {
                Some((_, param_ty)) => lower_expr_for_parameter(
                    ctx, arg, param_ty, super::integer_destinations::parameter_is_source_typed(sig, index),
                ),
                None => lower_expr(ctx, arg),
            };
            if index + 1 < normalized.len() {
                root_evaluated_call_argument(ctx, value, arg.span).value
            } else {
                value.value
            }
        })
        .collect()
}

/// Preserves a regular reference argument's place while consuming the shared source-order plan.
///
/// Also records the class-like parameter check the evaluated source needs, so every named
/// lowering branch defers it to parameter-order normalization.
fn lower_planned_source_arg(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    plan: &crate::types::call_args::CallArgPlan,
    source_index: usize,
    capture_values: bool,
) -> crate::ir::ValueId {
    let value = lower_planned_source_arg_value(ctx, sig, plan, source_index, capture_values);
    record_planned_object_param_source(ctx, sig, plan, source_index, value)
}

/// Lowers one planned source argument without recording its class-like parameter check.
fn lower_planned_source_arg_value(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    plan: &crate::types::call_args::CallArgPlan,
    source_index: usize,
    capture_values: bool,
) -> crate::ir::ValueId {
    let root_before_defaults = plan.possible_default_argument_mask(sig).into_iter()
        .zip(&sig.defaults).any(|(possible, default)| possible && default.as_ref()
            .is_some_and(crate::types::deferred_defaults::contains_deferred_error));
    let root_before_later_evaluation = source_index + 1 < plan.source_args.len() || root_before_defaults;
    if let Some(source) = plan.source_values.iter()
        .find(|source| source.source_index() == source_index)
    {
        if let Some(param_index) = source.param_idx() {
            if sig.ref_params.get(param_index).copied().unwrap_or(false) {
                return lower_arg_with_signature(ctx, sig, param_index, source.expr());
            }
            if let Some(lowered) =
                lower_tracked_callable_array_param(ctx, sig, param_index, source.expr())
            {
                return if root_before_later_evaluation {
                    root_evaluated_call_argument(ctx, lowered, source.expr().span).value
                } else {
                    lowered.value
                };
            }
        }
    }
    let value = if plan.source_values.iter().any(|source| {
        source.source_index() == source_index
            && source.param_idx().is_none()
            && variadic_param_is_by_ref(sig)
    }) {
        let source = plan
            .source_values
            .iter()
            .find(|source| source.source_index() == source_index)
            .expect("planned variadic reference source");
        lower_variadic_tail_source_value(
            ctx,
            source.expr(),
            true,
            None,
            &variadic_array_type(sig),
        )
        .value
    } else {
        // A regular parameter shapes its literal argument as a positional one does: a null-only
        // literal bound to `array<ptr<T>|null>` keeps pointer storage.
        let param = plan
            .source_values
            .iter()
            .find(|source| source.source_index() == source_index)
            .and_then(|source| source.param_idx())
            .and_then(|param_index| {
                let (_, param_ty) = sig.params.get(param_index)?;
                Some((param_ty.clone(), super::integer_destinations::parameter_is_source_typed(sig, param_index)))
            });
        match param {
            Some((param_ty, source_typed)) => {
                lower_expr_for_parameter(
                    ctx, &plan.source_args[source_index], &param_ty, source_typed,
                ).value
            }
            None => lower_call_source_arg(ctx, &plan.source_args[source_index]),
        }
    };
    if capture_values {
        let parameter = plan.source_values.iter()
            .find(|source| source.source_index() == source_index)
            .and_then(|source| source.param_idx())
            .unwrap_or(sig.params.len() + source_index);
        let lowered = lowered_value_from_id(ctx, value);
        capture_call_argument_value(ctx, lowered, parameter, plan.source_args[source_index].span).value
    } else if root_before_later_evaluation {
        let lowered = lowered_value_from_id(ctx, value);
        root_evaluated_call_argument(ctx, lowered, plan.source_args[source_index].span).value
    } else {
        value
    }
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
    let prefix = root_evaluated_call_argument(ctx, prefix, prefix_expr.span);
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
        source_values[source_index] = Some(lower_planned_source_arg(ctx, sig, plan, source_index, capture_values));
    }
    emit_dynamic_named_prefix_duplicate_guards(ctx, sig, plan, &prefix_temp, first_named_pos);

    let mut operands = Vec::with_capacity(plan.regular_args.len() + 1);
    for (param_index, arg) in plan.regular_args.iter().enumerate() {
        match arg {
            crate::types::call_args::PlannedRegularArg::Source { source_index, .. } => {
                operands.push(source_values.get(*source_index).copied().flatten()?);
            }
            crate::types::call_args::PlannedRegularArg::Default(default) => {
                operands.push(lower_arg_with_signature(ctx, sig, param_index, default));
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
                operands.push(lower_arg_with_signature(ctx, sig, param_index, &expr));
            }
        }
    }
    operands.push(lower_variadic_tail_array(ctx, sig, &[], 0).value);
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
    capture_output: Option<&(usize, PhpType)>,
) -> Option<Vec<crate::ir::ValueId>> {
    lower_named_args_with_spread_plan_impl(
        ctx,
        sig,
        plan,
        assoc_spread_sources,
        capture_values,
        capture_output,
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
    lower_named_args_with_spread_plan_impl(ctx, sig, plan, assoc_spread_sources, false, None, hinted)
}

/// Applies optional per-parameter hints while preserving caller-selected value capture.
fn lower_named_args_with_spread_plan_impl(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    plan: &crate::types::call_args::CallArgPlan,
    assoc_spread_sources: &[bool],
    capture_values: bool,
    capture_output: Option<&(usize, PhpType)>,
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
    let prefix = root_evaluated_call_argument(ctx, prefix, prefix_expr.span);
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
        let previous = capture_output.and_then(|(output, ty)| {
            planned.filter(|(index, _)| index == output && !capture_values)
                .and_then(|(_, expr)| prepare_captured_output_argument(ctx, expr, ty))
        });
        let value = match planned {
            Some((param_idx, expr)) => hinted(ctx, param_idx, expr)
                .map(|value| {
                    let omitted_can_throw = plan.possible_default_argument_mask(sig).into_iter()
                        .zip(&sig.defaults).any(|(possible, default)| possible && default.as_ref()
                            .is_some_and(crate::types::deferred_defaults::contains_deferred_error));
                    if (source_index + 1 < plan.source_args.len() || omitted_can_throw)
                        && !sig.ref_params.get(param_idx).copied().unwrap_or(false)
                    {
                        let lowered = lowered_value_from_id(ctx, value);
                        root_evaluated_call_argument(ctx, lowered, expr.span).value
                    } else {
                        value
                    }
                })
                .unwrap_or_else(|| lower_planned_source_arg(ctx, sig, plan, source_index, capture_values)),
            None => lower_planned_source_arg(ctx, sig, plan, source_index, capture_values),
        };
        let value = record_planned_object_param_source(ctx, sig, plan, source_index, value);
        if let Some((name, ty)) = previous { ctx.set_local_logical_type(&name, ty); }
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
                    operands.push(lower_arg_with_signature(ctx, sig, param_idx, &expr));
                } else {
                    operands.push(source_values.get(*source_index).copied().flatten()?);
                }
            }
            crate::types::call_args::PlannedRegularArg::Default(default) => {
                operands.push(lower_arg_with_signature(ctx, sig, param_idx, default));
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
                operands.push(lower_arg_with_signature(ctx, sig, param_idx, &expr));
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
