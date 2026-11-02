//! Purpose:
//! Checks statically planned arguments against the selected concrete enum method.
//!
//! Called from:
//! - `nullable_method_calls` inside the direct call's argument and reference-return scope.
//!
//! Key details:
//! - Shared plans own named matching, static spreads, defaults and variadic membership.
//! - Caller sources finish before parameter-order guards, casts and omitted defaults.
//! - Value snapshots retain their original tags; reference operands retain their places.

use super::*;
use crate::types::call_args::{PlannedRegularArg, PlannedSourceValue};
use super::enum_method_args::call_intermediate;

/// Keeps a by-reference place distinct from independently owned by-value snapshots.
enum StagedArgument {
    Reference { value: ValueId, referent: Option<String> },
    Value { original: String, checked: String },
}

/// Binds a static selected call without using the value-only descriptor return ABI.
pub(super) fn lower_arguments(
    ctx: &mut LoweringContext<'_, '_>, signature: &FunctionSig, args: &[Expr],
    class: &str, method: &str, span: Span,
) -> Option<Vec<ValueId>> {
    let expanded = crate::types::call_args::expand_static_assoc_spread_args(args);
    let flat = crate::types::call_args::expand_planned_positional_spreads(&expanded)
        .unwrap_or(expanded);
    if flat.iter().any(is_spread_arg) { return None; }
    let regular = crate::types::call_args::regular_param_count(signature);
    let plan = crate::types::call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
        signature, &flat, span, regular, false,
        signature.variadic.is_some() && !crate::func_args::sig_collects_surplus_args(signature),
        &assoc_spread_sources(ctx, &flat),
    ).ok()?;
    let named = plan.first_named_pos.is_some();
    let sources = if named { plan.source_values.clone() } else {
        plan.source_args.iter().enumerate().map(|(source_index, expr)| {
            if source_index < regular {
                PlannedSourceValue::Regular { source_index, param_idx: source_index, expr: expr.clone() }
            } else {
                PlannedSourceValue::Variadic { source_index, key: None, expr: expr.clone() }
            }
        }).collect()
    };
    let mut staged = Vec::with_capacity(plan.source_args.len());
    for (source_index, arg) in plan.source_args.iter().enumerate() {
        let parameter = sources.iter().find(|source| source.source_index() == source_index)
            .and_then(PlannedSourceValue::param_idx);
        let variadic_reference = parameter.is_none() && variadic_param_is_by_ref(signature);
        let reference_index = parameter.filter(|index|
            signature.ref_params.get(*index).copied().unwrap_or(false))
            .or_else(|| variadic_reference.then(|| signature.params.len() - 1));
        if let Some(index) = reference_index {
            let source = sources.iter().find(|source| source.source_index() == source_index).unwrap();
            let reference = enum_ref_args::stage_argument(ctx, signature, index, source.expr(),
                variadic_reference, span);
            let (value, referent) = reference.map_or_else(|| {
                (lower_arg_with_signature(ctx, signature, index, source.expr()), None)
            }, |(value, alias)| (value, Some(alias)));
            staged.push(StagedArgument::Reference { value, referent });
        } else {
            let value = parameter.and_then(|index| {
                let source = sources.iter().find(|source| source.source_index() == source_index)?;
                lower_tracked_callable_array_param(ctx, signature, index, source.expr())
            }).unwrap_or_else(|| {
                let value = lower_call_source_arg(ctx, arg);
                lowered_value_from_id(ctx, value)
            });
            let value = snapshot_mixed(ctx, value, span);
            let original = call_intermediate(ctx, value, span);
            let value = ctx.load_local(&original, Some(span));
            let value = if matches!(value.ir_type, IrType::Heap(IrHeapKind::Mixed | IrHeapKind::Union)) {
                snapshot_mixed(ctx, value, span)
            } else { ctx.box_value_as_mixed(value, PhpType::Mixed, Some(span)) };
            let checked = call_intermediate(ctx, value, span);
            staged.push(StagedArgument::Value { original, checked });
        }
    }
    let mut values = Vec::new();
    for index in 0..regular {
        let source = if named {
            match &plan.regular_args[index] {
                PlannedRegularArg::Source { source_index, .. } => Some(*source_index),
                PlannedRegularArg::Default(_) => None,
                PlannedRegularArg::SpreadElement { .. } => unreachable!("static enum plan has no spread"),
            }
        } else { (index < staged.len()).then_some(index) };
        let value = if let Some(source) = source {
            bind_argument(ctx, signature, index, &staged[source], None, class, method, span)
        } else {
            let default = signature.defaults[index].as_ref()
                .expect("selected static call already passed the required-argument plan");
            lower_arg_with_signature(ctx, signature, index, default)
        };
        values.push(value);
    }
    if signature.variadic.is_some() {
        if crate::func_args::sig_has_hidden_argc_param(signature) {
            let count = named_plan_actual_arg_count(&sources);
            values.push(emit_i64_at_span(ctx, count as i64, span).value);
        }
        let mut element_signature = signature.clone();
        let index = signature.params.len() - 1;
        if let Some(element) = crate::types::signatures::variadic_source_element_type_expr(signature) {
            element_signature.params[index].1 = ctx.type_expr_to_php_type_for_value(element);
        } else { element_signature.declared_params[index] = false; }
        let mut bound = vec![ValueId::from_raw(0); staged.len()];
        let mut tail_sources = sources.clone();
        let mut position = regular;
        let named_position = named_plan_actual_arg_count(&sources).max(regular) + 1;
        for source in &mut tail_sources {
            if source.param_idx().is_some() { continue; }
            let argument_position = if source.key().is_some() { named_position } else {
                position += 1;
                position
            };
            let value = bind_argument(ctx, &element_signature, index,
                &staged[source.source_index()], Some(argument_position), class, method, span);
            bound[source.source_index()] = value;
            if !variadic_param_is_by_ref(signature) {
                // Packing must use checked values, never reevaluate caller expressions.
                let slot = call_intermediate(ctx, lowered_value_from_id(ctx, value), span);
                let value = Expr::new(ExprKind::Variable(slot), span);
                if let PlannedSourceValue::Variadic { expr, .. } = source { *expr = value; }
            }
        }
        let tail = lower_named_variadic_tail_array(ctx, signature, &tail_sources, &bound);
        values.push(root_evaluated_call_argument(ctx, tail, span).value);
    }
    Some(coerce_operands_to_params(ctx, signature, values))
}

/// Copies mutable Mixed cells without transferring a caller local's sole owner.
fn snapshot_mixed(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span) -> LoweredValue {
    if !matches!(value.ir_type, IrType::Heap(IrHeapKind::Mixed | IrHeapKind::Union)) { return value; }
    let snapshot = ctx.emit_owned_value(Op::MixedClone, vec![value.value], None,
        ctx.builder.value_php_type(value.value), Op::MixedClone.default_effects(), Some(span));
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
    snapshot
}

/// Validates original runtime tags, then materializes the selected scalar ABI or original place.
fn bind_argument(
    ctx: &mut LoweringContext<'_, '_>, signature: &FunctionSig, index: usize,
    argument: &StagedArgument, position: Option<usize>, class: &str, method: &str, span: Span,
) -> ValueId {
    let StagedArgument::Value { original, checked } = argument else {
        let StagedArgument::Reference { value, referent } = argument else { unreachable!() };
        if let Some(referent) = referent {
            enum_ref_args::guard_argument(ctx, signature, index, referent, position, class, method, span);
        }
        return *value;
    };
    let variable = Expr::new(ExprKind::Variable(checked.clone()), span);
    if let Some(position) = position {
        let position = Expr::new(ExprKind::IntLiteral(position as i64), span);
        enum_argument_guards::guard_variadic_element(ctx, signature, index, &variable,
            &position, class, method, span);
    } else {
        enum_argument_guards::guard_parameter(ctx, signature, index, &variable, class, method, span);
    }
    let binding = signature.declared_params.get(index).copied().unwrap_or(false)
        .then(|| crate::types::param_binding::runtime_scalar_param_binding(
            &signature.params[index].1, &variable,
        )).flatten();
    let value = if let Some(binding) = binding { lower_expr(ctx, &binding) } else {
        let value = ctx.load_local(original, Some(span));
        crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span))
    };
    let value = root_evaluated_call_argument(ctx, value, span);
    enum_method_args::release_snapshot_payload(ctx, original, span);
    enum_method_args::release_snapshot_payload(ctx, checked, span);
    value.value
}
