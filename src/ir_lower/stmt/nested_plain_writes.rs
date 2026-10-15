//! Purpose:
//! Lowers plain nested stores and deep appends through attached writable parent cells.
//!
//! Called from:
//! - Nested array assignment dispatch and recognized parser-generated deep append groups.
//!
//! Key details:
//! - Computed inputs run before the RHS; write-context traversal runs after it.
//! - False conversion is published before its deprecation callback.
//! - Parent, key and RHS owners remain rooted across callbacks and retirement.

use super::*;

/// Writes a leaf only after evaluating its computed dimensions and RHS.
pub(super) fn lower_store(
    ctx: &mut LoweringContext<'_, '_>, target: &Expr, value: &Expr, span: Span,
) {
    let mut sources = Vec::new();
    let target = super::nested_write_sources::capture_target(ctx, target, &mut sources);
    let value = lower_expr(ctx, value);
    let (value, value_owner) = super::nested_array_writes::root_static_update_receiver(ctx, value, span);
    let target = super::nested_array_writes::bind_concrete_static_update_root(ctx, &target, &target)
        .map_or(target, |(_, write)| write);
    let ExprKind::ArrayAccess { array, index } = &target.kind else { unreachable!() };
    let parent = super::nested_array_writes::lower_nested_assign_parent_for_store(ctx, array, span);
    let (parent, parent_owner) = super::nested_array_writes::root_static_update_receiver(ctx, parent, span);
    let key = lower_expr(ctx, index);
    let (key, key_owner) = super::nested_array_writes::root_static_update_receiver(ctx, key, span);
    let pending = super::nested_false_errors::capture(ctx, parent, span);
    super::nested_false_errors::handle_pending(ctx, pending, Some(key), span, |ctx| {
        ctx.emit_void(Op::RuntimeCall, vec![parent.value, key.value, value.value], None,
            effects_lookup::runtime_effects(), Some(span));
    });
    guard_scalar_parent(ctx, parent, span);
    ctx.emit_void(Op::RuntimeCall, vec![parent.value, key.value, value.value], None,
        effects_lookup::runtime_effects(), Some(span));
    retire_owner(ctx, key_owner, span);
    retire_owner(ctx, parent_owner, span);
    retire_owner(ctx, value_owner, span);
    super::nested_write_sources::retire(ctx, sources, span);
}

/// Appends directly into a stored parent instead of cloning a detached read and writing it back.
pub(super) fn lower_append(
    ctx: &mut LoweringContext<'_, '_>, target: &Expr, value: &Expr, span: Span,
) {
    let mut sources = Vec::new();
    let target = super::nested_write_sources::capture_target(ctx, target, &mut sources);
    let value = lower_expr(ctx, value);
    let (value, value_owner) = super::nested_array_writes::root_static_update_receiver(ctx, value, span);
    let target = super::nested_array_writes::bind_concrete_static_update_root(ctx, &target, &target)
        .map_or(target, |(_, write)| write);
    let parent = super::nested_array_writes::lower_nested_assign_parent_for_store(ctx, &target, span);
    let (parent, parent_owner) = super::nested_array_writes::root_static_update_receiver(ctx, parent, span);
    let pending = super::nested_false_errors::capture(ctx, parent, span);
    super::nested_false_errors::handle_pending(ctx, pending, None, span, |ctx| {
        ctx.emit_void(Op::MixedArrayAppend, vec![parent.value, value.value], None,
            Op::MixedArrayAppend.default_effects(), Some(span));
    });
    guard_scalar_parent(ctx, parent, span);
    ctx.emit_void(Op::MixedArrayAppend, vec![parent.value, value.value], None,
        Op::MixedArrayAppend.default_effects(), Some(span));
    retire_owner(ctx, parent_owner, span);
    retire_owner(ctx, value_owner, span);
    super::nested_write_sources::retire(ctx, sources, span);
}

/// Rejects non-array scalar parents after false conversion, leaving string-offset handling intact.
fn guard_scalar_parent(ctx: &mut LoweringContext<'_, '_>, parent: LoweredValue, span: Span) {
    if ctx.builder.value_php_type(parent.value).codegen_repr() != PhpType::Mixed { return; }
    let boolean = ctx.emit_value(Op::TypePredicate, vec![parent.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Bool)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(span));
    let integer = ctx.emit_value(Op::TypePredicate, vec![parent.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Int)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(span));
    let float = ctx.emit_value(Op::TypePredicate, vec![parent.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Float)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(span));
    let numeric = ctx.emit_value(Op::IBitOr, vec![integer.value, float.value], None, PhpType::Bool,
        Op::IBitOr.default_effects(), Some(span));
    let scalar = ctx.emit_value(Op::IBitOr, vec![boolean.value, numeric.value], None, PhpType::Bool,
        Op::IBitOr.default_effects(), Some(span));
    let invalid = ctx.builder.create_named_block("nested.store.scalar.error", Vec::new());
    let valid = ctx.builder.create_named_block("nested.store.parent", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: scalar.value, then_target: invalid, then_args: Vec::new(),
        else_target: valid, else_args: Vec::new(),
    });
    ctx.builder.position_at_end(invalid);
    lower_throw_access_error(ctx, "Cannot use a scalar value as an array", span);
    ctx.builder.position_at_end(valid);
}

/// Fetches one child with balanced, unwind-visible input and result owners.
pub(super) fn fetch_parent(
    ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue, index: &Expr, span: Span,
) -> LoweredValue {
    let (receiver, receiver_owner) =
        super::nested_array_writes::root_static_update_receiver(ctx, receiver, span);
    let key = lower_expr(ctx, index);
    let (key, key_owner) = super::nested_array_writes::root_static_update_receiver(ctx, key, span);
    let pending = super::nested_false_errors::capture(ctx, receiver, span);
    super::nested_false_errors::handle_pending(ctx, pending, Some(key), span, |ctx| {
        let null = lower_expr(ctx, &Expr::new(ExprKind::Null, span));
        ctx.emit_void(Op::RuntimeCall, vec![receiver.value, key.value, null.value], None,
            effects_lookup::runtime_effects(), Some(span));
    });
    guard_scalar_parent(ctx, receiver, span);
    let child = ctx.emit_value(Op::RuntimeCall, vec![receiver.value, key.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ArrayFetchForWrite)),
        PhpType::Mixed, effects_lookup::runtime_effects(), Some(span));
    let selected = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let slot = ctx.local_slots[&selected];
    ctx.store_call_argument_local(&selected, child, PhpType::Mixed, Some(span));
    // Publish the child before a retired input can throw, keeping every older owner reachable.
    if let Some(owner) = key_owner { pop_owner(ctx, owner, span); }
    push_owner(ctx, slot, span);
    if let Some(owner) = key_owner { release_owner(ctx, owner, span); }
    if let Some(owner) = receiver_owner {
        pop_owner(ctx, slot, span);
        pop_owner(ctx, owner, span);
        push_owner(ctx, slot, span);
        release_owner(ctx, owner, span);
    }
    let child = ctx.load_local(&selected, Some(span));
    ctx.clear_owned_hidden_temp(&selected, Some(span));
    pop_owner(ctx, slot, span);
    child
}

/// Removes one topmost guard without invoking its value's destructor.
fn pop_owner(ctx: &mut LoweringContext<'_, '_>, slot: LocalSlotId, span: Span) {
    ctx.emit_void(Op::PopCallOperandOwner, Vec::new(), Some(Immediate::LocalSlot(slot)),
        Op::PopCallOperandOwner.default_effects(), Some(span));
}

/// Publishes one initialized result or input owner.
fn push_owner(ctx: &mut LoweringContext<'_, '_>, slot: LocalSlotId, span: Span) {
    ctx.emit_void(Op::PushCallOperandOwner, Vec::new(), Some(Immediate::LocalSlot(slot)),
        Op::PushCallOperandOwner.default_effects(), Some(span));
}

/// Clears storage before releasing an owner whose destructor may throw.
fn release_owner(ctx: &mut LoweringContext<'_, '_>, slot: LocalSlotId, span: Span) {
    ctx.emit_void(Op::ReleaseLocalSlot, Vec::new(), Some(Immediate::LocalSlot(slot)),
        Op::ReleaseLocalSlot.default_effects(), Some(span));
}

/// Retires an optional topmost owner after its consumer has retained or copied the value.
fn retire_owner(ctx: &mut LoweringContext<'_, '_>, owner: Option<LocalSlotId>, span: Span) {
    if let Some(owner) = owner { crate::ir_lower::expr::retire_owned_call_operand(ctx, owner, span); }
}
