//! Purpose:
//! Separates persistent native static storage from its reached activation binding.
//!
//! Called from:
//! - Static declaration lowering after ordinary expression checking.
//!
//! Key details:
//! - Persistent cells hold canonical boxed values independently of their semantic origins.
//! - A displaced value remains pinned until the new binding is observable.

use super::*;

/// Reports whether publishing an initializer needs an owner independent of its source slot.
fn initializer_needs_independent_owner(
    ctx: &LoweringContext<'_, '_>,
    value: crate::ir::ValueId,
) -> bool {
    match ctx.builder.value_ownership(value) {
        Ownership::Borrowed => true,
        Ownership::MaybeOwned => initializer_is_refcounted_local_load(ctx, value),
        Ownership::Owned | Ownership::NonHeap | Ownership::Persistent | Ownership::Moved => false,
    }
}

/// Recognizes a refcounted value still owned by a concrete PHP local slot.
fn initializer_is_refcounted_local_load(
    ctx: &LoweringContext<'_, '_>,
    value: crate::ir::ValueId,
) -> bool {
    let Some(instruction) = ctx.builder.value_defining_instruction(value) else {
        return false;
    };
    if !matches!(instruction.op, Op::LoadLocal | Op::LoadStaticLocal) {
        return false;
    }
    let Some(Immediate::LocalSlot(slot)) = instruction.immediate else {
        return false;
    };
    if !matches!(ctx.builder.local_kind(slot), LocalKind::PhpLocal | LocalKind::StaticLocal) {
        return false;
    }
    let result = ctx.builder.value_php_type(value).codegen_repr();
    result.is_refcounted() || matches!(result, PhpType::Str | PhpType::Callable)
}

/// Initializes persistent storage before binding the activation name to its managed cell.
pub(super) fn lower(ctx: &mut LoweringContext<'_, '_>, name: &str, init: &Expr, span: Span) {
    let entry = ctx.builder.insertion_block().expect("static declaration block");
    let init_block = ctx.builder.create_named_block("static_local_init", Vec::new());
    let after = ctx.builder.create_named_block("static_local_after", Vec::new());
    ctx.builder.position_at_end(init_block);
    let value = lower_expr(ctx, init);
    let value = crate::ir_lower::ownership::copy_assignment_value(ctx, value, Some(span));
    let value = if initializer_needs_independent_owner(ctx, value.value) {
        crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span))
    } else {
        value
    };
    let existing = ctx.builder.function().locals.iter().find(|local|
        local.kind == LocalKind::StaticLocal && local.name.as_deref() == Some(name)).map(|local| local.id);
    let slot = existing.unwrap_or_else(|| ctx.builder.add_local(Some(name.to_string()),
        IrType::Heap(crate::ir::IrHeapKind::Mixed), PhpType::Mixed, LocalKind::StaticLocal));
    ctx.emit_void(Op::InitStaticLocal, vec![value.value], Some(Immediate::LocalSlot(slot)),
        Op::InitStaticLocal.default_effects(), Some(span));
    branch_to(ctx, after);
    ctx.builder.position_at_end(entry);
    let uninitialized = ctx.emit_value(Op::StaticLocalUninitialized, Vec::new(),
        Some(Immediate::LocalSlot(slot)), PhpType::Bool,
        Op::StaticLocalUninitialized.default_effects(), Some(span));
    ctx.builder.terminate(Terminator::CondBr { cond: uninitialized.value,
        then_target: init_block, then_args: Vec::new(), else_target: after, else_args: Vec::new() });
    ctx.builder.position_at_end(after);
    let displaced = if ctx.local_slots.contains_key(name) {
        let previous = ctx.load_local_for_retirement_pin(name, Some(span));
        crate::ir_lower::expr::root_value_for_later_use(ctx, previous, span).1
    } else { None };
    if ctx.local_kinds.get(name) == Some(&LocalKind::GlobalAlias) {
        // The process still owns the global; only this activation name changes storage.
        ctx.local_slots.remove(name);
        ctx.local_kinds.remove(name);
        ctx.unmark_ref_bound_local(name);
    }
    let pointer = ctx.emit_value(Op::LoadStaticLocalRefCell, Vec::new(),
        Some(Immediate::LocalSlot(slot)), PhpType::Pointer(None),
        Op::LoadStaticLocalRefCell.default_effects(), Some(span));
    let contract = ctx.local_store_contracts.get(&(ctx.loop_storage_scope.clone(), span, name.to_string()))
        .filter(|contract| contract.feature == elephc_semantics::StrictFeature::Locals)
        .cloned();
    if let Some(contract) = &contract {
        ctx.builder.emit_with_effects(Op::BindLocalRefContract, Vec::new(),
            Some(Immediate::LocalSlot(slot)), IrType::Void, contract.ty.clone(),
            Ownership::NonHeap, Op::BindLocalRefContract.default_effects(), Some(span));
    }
    crate::ir_lower::local_binding_origins::end(ctx, name, Some(span));
    ctx.bind_persistent_ref_cell_ptr(name, pointer, span);
    crate::ir_lower::local_binding_origins::start_reference_declaration(ctx, name,
        contract.map_or(PhpType::Mixed, |contract| contract.ty), span);
    if let Some(owner) = displaced { crate::ir_lower::expr::retire_owned_call_operand(ctx, owner, span); }
    ctx.clear_static_callable_locals();
}
