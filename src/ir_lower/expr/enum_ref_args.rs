//! Purpose:
//! Captures selected enum reference places and validates their current referents.
//!
//! Called from:
//! - `enum_static_args` before source evaluation and during parameter-order binding.
//!
//! Key details:
//! - An alias freezes the place once, not the value it held before later arguments ran.
//! - Ordinary reference assignment owns promotion, interior-cell leases and reference returns.
//! - Guards run after all source effects; accepted scalar bindings write through the alias.

use super::*;

/// Recovers a source contract when the physical parameter was normalized to boxed storage.
fn reference_contract(ctx: &LoweringContext<'_, '_>, signature: &FunctionSig, index: usize) -> Option<PhpType> {
    if !signature.declared_params.get(index).copied().unwrap_or(false) { return None; }
    let ty = &signature.params.get(index)?.1;
    let ty = if *ty == PhpType::Mixed {
        ctx.type_expr_to_php_type_for_value(signature.param_type_exprs.get(index)?.as_ref()?)
    } else { ty.clone() };
    (ty != PhpType::Mixed).then_some(ty)
}

/// Selects boxed contracts that can differ while sharing a reference payload ABI.
fn needs_captured_guard(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Object(_) | PhpType::False)
        || matches!(ty, PhpType::Union(_)) && ty.codegen_repr() == PhpType::Mixed
}

/// Retains a reference place once, without validating before later source effects.
pub(super) fn stage_argument(
    ctx: &mut LoweringContext<'_, '_>, signature: &FunctionSig, index: usize,
    source: &Expr, variadic: bool, span: Span,
) -> Option<(ValueId, String)> {
    if !variadic && !reference_contract(ctx, signature, index).is_some_and(|ty| needs_captured_guard(&ty)) {
        return None;
    }
    if let ExprKind::Variable(name) = &source.kind {
        // A branch can restore a concrete hint after another branch widened the cell.
        // Inherit its real payload layout before capturing the alias.
        let payload = ctx.ref_cell_owner_slot(name)
            .or_else(|| ctx.local_slots.get(name).copied())
            .map(|slot| ctx.builder.local_php_type(slot));
        if payload.is_some_and(|ty| ty.codegen_repr() == PhpType::Mixed) {
            ctx.set_local_type(name, PhpType::Mixed);
        }
        if variadic { ctx.promote_local_mixed_ref_cell(name, Some(source.span)); }
    }
    // Reference assignment chooses the actual source payload, including raw interior cells.
    let alias = ctx.next_synthetic_place_local_name();
    crate::ir_lower::stmt::lower_ref_assign(ctx, &alias, source, span);
    if let Some(slot) = ctx.ref_cell_owner_slot(&alias) {
        register_owned_call_operand(ctx, slot, span);
        let payload = ctx.builder.local_php_type(slot);
        ctx.call_argument_evaluation_scopes.last_mut().expect("selected reference argument scope")
            .owners.push(crate::ir_lower::context::CallArgumentEvaluationOwner {
                value: None, borrow: None, temp_name: alias.clone(), slot, span,
                ref_cell_payload: Some(payload), scoped_receiver_alias: true, pins_borrowed_source: false,
            });
    }
    let value = if variadic { lower_invoker_ref_arg_marker(ctx, &alias, span) }
        else { ctx.load_local(&alias, Some(span)) };
    let value = if variadic { root_evaluated_call_argument(ctx, value, span) } else { value };
    Some((value.value, alias))
}

/// Checks the live captured referent after every caller expression has completed.
pub(super) fn guard_argument(
    ctx: &mut LoweringContext<'_, '_>, signature: &FunctionSig, index: usize,
    alias: &str, position: Option<usize>, class: &str, method: &str, span: Span,
) {
    let Some(contract) = reference_contract(ctx, signature, index) else { return; };
    let mut logical = signature.clone();
    logical.params[index].1 = contract.clone();
    let value = ctx.load_local(alias, Some(span));
    let value = if matches!(value.ir_type, IrType::Heap(IrHeapKind::Mixed | IrHeapKind::Union)) {
        ctx.emit_owned_value(Op::MixedClone, vec![value.value], None, PhpType::Mixed,
            Op::MixedClone.default_effects(), Some(span))
    } else { ctx.box_value_as_mixed(value, PhpType::Mixed, Some(span)) };
    let slot = enum_method_args::call_intermediate(ctx, value, span);
    let value = Expr::new(ExprKind::Variable(slot.clone()), span);
    if let Some(position) = position {
        let position = Expr::new(ExprKind::IntLiteral(position as i64), span);
        enum_argument_guards::guard_variadic_element(ctx, &logical, index, &value,
            &position, class, method, span);
    } else {
        enum_argument_guards::guard_parameter(ctx, &logical, index, &value, class, method, span);
    }
    // Work on the independent snapshot across diagnostic callbacks, then publish only an
    // accepted binding through the captured place. Invalid values leave the caller untouched.
    if let Some(binding) = crate::types::param_binding::runtime_scalar_param_binding(&contract, &value) {
        let bound = lower_expr(ctx, &binding);
        store_value_into_temp(ctx, alias, ctx.local_type(alias), bound, span);
    }
    enum_method_args::release_snapshot_payload(ctx, &slot, span);
}
