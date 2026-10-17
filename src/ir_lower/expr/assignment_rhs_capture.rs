//! Purpose:
//! Gives delayed static-write RHS captures an expression-scoped lifetime.
//!
//! Called from:
//! - `super::assignments::lower_assignment_expr()` after the parser's RHS prelude.
//!
//! Key details:
//! - The actual generated PHP slot is rooted, not a second copy that leaves it alive.
//! - Secure the result before retiring the slot; owner records are strictly LIFO.

use super::*;

/// Publishes the final generated RHS slot before a delayed non-local write can throw.
pub(super) fn publish_static_assignment_rhs(
    ctx: &mut LoweringContext<'_, '_>, target: &Expr, value: &Expr,
    result: &Expr, prelude: &[Stmt], span: Span,
) -> Option<crate::ir::LocalSlotId> {
    let ExprKind::Variable(name) = &value.kind else { return None; };
    if !name.starts_with("__elephc_assign_expr_") || !crate::names::is_generated_local_name(name)
        || !matches!(&result.kind, ExprKind::Variable(result) if result == name)
        || !matches!(prelude.last().map(|stmt| &stmt.kind),
            Some(StmtKind::Assign { name: captured, .. }) if captured == name)
    { return None; }
    let mut root = target;
    while let ExprKind::ArrayAccess { array, .. } = &root.kind { root = array; }
    let delayed = match &root.kind {
        ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::PropertyAccess { object, .. } | ExprKind::DynamicPropertyAccess { object, .. } =>
            is_static_property_write_chain(object),
        _ => false,
    };
    if !delayed || !Ownership::php_type_needs_lifetime_tracking(&ctx.local_type(name)) {
        return None;
    }
    let slot = ctx.local_slots.get(name).copied()?;
    register_owned_call_operand(ctx, slot, span);
    Some(slot)
}

/// Transfers the result's lifetime out of its capture and retires that real slot safely.
pub(super) fn finish_static_assignment_rhs(
    ctx: &mut LoweringContext<'_, '_>, slot: Option<crate::ir::LocalSlotId>,
    result: LoweredValue, span: Span,
) -> LoweredValue {
    let Some(slot) = slot else { return result; };
    ctx.with_independent_write_receiver(|ctx| {
        let copied = crate::ir_lower::ownership::copy_assignment_value(ctx, result, Some(span));
        let owned = if copied.value == result.value {
            let owned = crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, result, Some(span));
            crate::ir_lower::ownership::release_if_owned(ctx, result, Some(span));
            owned
        } else { copied };
        // POP detaches the top record, not the named slot. Publish the result only afterwards.
        unregister_owned_call_operand(ctx, slot, span);
        let pins = pin_in_flight_owners(ctx, &[owned.value], span);
        ctx.emit_void(Op::ReleaseLocalSlot, Vec::new(), Some(Immediate::LocalSlot(slot)),
            Op::ReleaseLocalSlot.default_effects(), Some(span));
        unpin_in_flight_owners(ctx, pins, span);
        owned
    })
}
