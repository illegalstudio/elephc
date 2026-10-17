//! Purpose:
//! Lowers compiler-generated append updates through captured ordinary Mixed cells.
//! Keeps the selected place stable across diagnostic callbacks without PHP reference flags.
//!
//! Called from:
//! - `crate::ir_lower::expr::assignments::lower_assignment_expr()`.
//! - `LoweringContext::store_local_impl()` for stores through an active cell alias.
//!
//! Key details:
//! - RefAssign marker names are an unforgeable parser/EIR protocol, not reference-cell ABI pointers.
//! - Alias slots own stored Mixed boxes and publish cleanup records on normal/exceptional paths.

use super::*;
use crate::ir::RuntimeCallTarget;
use super::call_operand_owners::{register_owned_call_operand, retire_owned_call_operand};

/// Processes an append prelude while every captured stored cell stays rooted for unwinding.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, result: &Expr, prelude: &[Stmt],
) -> Option<LoweredValue> {
    if !prelude.iter().any(|stmt| matches!(&stmt.kind,
        StmtKind::RefAssign { target, .. } if crate::names::append_update_alias_kind(target).is_some()
    )) { return None; }
    let mut aliases: Vec<(String, LocalSlotId, crate::span::Span)> = Vec::new();
    let cancellable = prelude.iter().any(|stmt| read_parent_alias(stmt).is_some());
    let prefix_increment = matches!(
        &result.kind,
        ExprKind::Variable(name) if crate::names::is_append_prefix_increment_result(name)
    );
    let selected = cancellable.then(|| ctx.declare_owned_hidden_temp(PhpType::Mixed));
    let merge = cancellable.then(|| ctx.builder.create_named_block("append.update.result", Vec::new()));
    let mut cancelled_initialized = None;
    for stmt in prelude {
        if let StmtKind::RefAssign { target, source } = &stmt.kind {
            if let Some(leaf) = crate::names::append_update_alias_kind(target) {
                let cell = crate::ir_lower::stmt::lower_nested_assign_parent(ctx, source, source.span);
                debug_assert_eq!(ctx.builder.value_php_type(cell.value).codegen_repr(), PhpType::Mixed);
                debug_assert!(!ctx.append_update_cell_aliases.contains(target));
                // A normal Mixed pointer slot owns the box itself. PHP references instead
                // contain an address of a boxed-pointer slot and use a different ABI.
                ctx.store_local(target, cell, PhpType::Mixed, Some(source.span));
                let slot = ctx.local_slots[target];
                let parent_alias = match &source.kind {
                    ExprKind::ArrayAccess { array, .. } => match &array.kind {
                        ExprKind::Variable(name) => Some(name.as_str()),
                        _ => None,
                    },
                    _ => None,
                };
                if aliases.last().is_some_and(|(name, _, _)| Some(name.as_str()) == parent_alias) {
                    let (name, parent_slot, _) = aliases.pop().unwrap();
                    // Remove the old LIFO record, publish the child, then retire its parent.
                    // A dead compiler ancestor must not masquerade as a live PHP array owner.
                    ctx.emit_void(Op::PopCallOperandOwner, Vec::new(), Some(Immediate::LocalSlot(parent_slot)),
                        Op::PopCallOperandOwner.default_effects(), Some(source.span));
                    register_owned_call_operand(ctx, slot, source.span);
                    ctx.emit_void(Op::ReleaseLocalSlot, Vec::new(), Some(Immediate::LocalSlot(parent_slot)),
                        Op::ReleaseLocalSlot.default_effects(), Some(source.span));
                    ctx.append_update_cell_aliases.remove(&name);
                } else {
                    register_owned_call_operand(ctx, slot, source.span);
                }
                ctx.append_update_cell_aliases.insert(target.clone());
                aliases.push((target.clone(), slot, source.span));
                if !leaf { initialize_pending_parent(ctx, target, source.span); }
                continue;
            }
        }
        crate::ir_lower::stmt::lower_stmt(ctx, stmt);
        if let Some(parent) = read_parent_alias(stmt) {
            let selected = selected.as_ref().unwrap();
            let merge = merge.unwrap();
            let parent = ctx.load_local(parent, Some(stmt.span));
            let live = ctx.emit_value(Op::RuntimeCall, vec![parent.value],
                Some(Immediate::RuntimeCall(RuntimeCallTarget::MixedArrayCellHasLiveOwner)),
                PhpType::Bool, Effects::READS_HEAP, Some(stmt.span));
            let initialized = ctx.initialized_slots_snapshot();
            let active = ctx.builder.create_named_block("append.update.active", Vec::new());
            let cancel = ctx.builder.create_named_block("append.update.cancel", Vec::new());
            ctx.builder.terminate(Terminator::CondBr {
                cond: live.value, then_target: active, then_args: Vec::new(),
                else_target: cancel, else_args: Vec::new(),
            });
            ctx.builder.position_at_end(cancel);
            for (_, slot, span) in aliases.iter().rev() {
                retire_owned_call_operand(ctx, *slot, *span);
            }
            let cancelled = if prefix_increment {
                lower_expr(ctx, &Expr::new(ExprKind::IntLiteral(1), stmt.span))
            } else {
                lower_expr(ctx, &Expr::new(ExprKind::Null, stmt.span))
            };
            store_value_into_temp(ctx, selected, PhpType::Mixed, cancelled, stmt.span);
            cancelled_initialized = Some(ctx.initialized_slots_snapshot());
            branch_to(ctx, merge);
            ctx.builder.position_at_end(active);
            ctx.restore_initialized_slots(initialized);
        }
    }
    for (name, slot, span) in aliases.into_iter().rev() {
        retire_owned_call_operand(ctx, slot, span);
        ctx.append_update_cell_aliases.remove(&name);
    }
    let value = lower_expr(ctx, result);
    if let Some(selected) = selected {
        store_value_into_temp(ctx, &selected, PhpType::Mixed, value, result.span);
        let initialized = ctx.initialized_slots_snapshot();
        branch_to(ctx, merge.unwrap());
        ctx.builder.position_at_end(merge.unwrap());
        ctx.restore_initialized_slots(initialized.intersection(&cancelled_initialized.unwrap())
            .copied().collect());
        Some(take_owned_temp(ctx, &selected, result.span))
    } else {
        Some(value)
    }
}

/// Recognizes only the parser's missing-leaf read through an unforgeable parent lease.
fn read_parent_alias(stmt: &Stmt) -> Option<&str> {
    let StmtKind::Assign { value, .. } = &stmt.kind else { return None; };
    let ExprKind::ArrayAccess { array, .. } = &value.kind else { return None; };
    let ExprKind::Variable(name) = &array.kind else { return None; };
    (crate::names::append_update_alias_kind(name) == Some(false)).then_some(name.as_str())
}

/// Publishes the fresh append's preallocated parent before a missing-key callback can inspect it.
fn initialize_pending_parent(
    ctx: &mut LoweringContext<'_, '_>, name: &str, span: crate::span::Span,
) {
    let parent = ctx.load_local(name, Some(span));
    // Reserve one slot before any missing-key warning can invoke user code. A surviving
    // payload copy must observe the pending write without a later promotion or relocation.
    let container_ty = if crate::names::append_update_alias_prefers_hash(name) {
        PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        }
    } else {
        PhpType::Array(Box::new(PhpType::Mixed))
    };
    let op = if matches!(container_ty, PhpType::AssocArray { .. }) {
        Op::HashNew
    } else {
        Op::ArrayNew
    };
    let empty = ctx.emit_owned_value(
        op,
        Vec::new(),
        Some(Immediate::Capacity(1)),
        container_ty,
        op.default_effects(),
        Some(span),
    );
    ctx.emit_void(Op::RuntimeCall, vec![parent.value, empty.value], None,
        crate::ir_lower::effects_lookup::runtime_effects(), Some(span));
    crate::ir_lower::ownership::release_if_owned(ctx, empty, Some(span));
}

/// Publishes a value into the selected box before retiring its prior payload and source owner.
pub(crate) fn store_alias(
    ctx: &mut LoweringContext<'_, '_>, name: &str, value: LoweredValue,
    span: Option<crate::span::Span>,
) -> LoweredValue {
    let span = span.unwrap_or_else(crate::span::Span::synthetic);
    let target = ctx.load_local(name, Some(span));
    let source = if matches!(ctx.builder.value_php_type(value.value).codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)) {
        value
    } else {
        ctx.box_value_as_mixed(value, PhpType::Mixed, Some(span))
    };
    let pins = super::pin_in_flight_owners(ctx, &[source.value], span);
    ctx.emit_void(Op::RuntimeCall, vec![target.value, source.value], None,
        crate::ir_lower::effects_lookup::runtime_effects(), Some(span));
    super::unpin_in_flight_owners(ctx, pins, span);
    crate::ir_lower::ownership::release_if_owned(ctx, source, Some(span));
    ctx.load_local(name, Some(span))
}
