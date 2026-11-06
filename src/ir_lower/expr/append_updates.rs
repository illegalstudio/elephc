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
    let guarded = prelude.iter().any(|stmt| guarded_parent_alias(stmt).is_some());
    let cancellable = prelude.iter().filter_map(guarded_parent_alias)
        .any(|(_, role)| role.is_final());
    let prefix_increment = matches!(
        &result.kind,
        ExprKind::Variable(name) if crate::names::is_append_prefix_increment_result(name)
    );
    let selected = cancellable.then(|| ctx.declare_owned_hidden_temp(PhpType::Mixed));
    let merge = cancellable.then(|| ctx.builder.create_named_block("append.update.result", Vec::new()));
    let mut cancelled_initialized = None;
    let detached = guarded.then(|| ctx.declare_hidden_temp(PhpType::Int));
    let keeper = guarded.then(|| ctx.declare_owned_hidden_temp(PhpType::Mixed));
    if let (Some(detached), Some(keeper)) = (&detached, &keeper) {
        store_expr_into_temp(ctx, detached, PhpType::Int,
            &Expr::new(ExprKind::IntLiteral(0), result.span), result.span);
        store_expr_into_temp(ctx, keeper, PhpType::Mixed,
            &Expr::new(ExprKind::Null, result.span), result.span);
        register_owned_call_operand(ctx, ctx.local_slots[keeper], result.span);
    }
    let mut position = 0;
    while let Some(stmt) = prelude.get(position) {
        position += 1;
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
        // A guard must never create a PHP owner or trigger an undefined-key read.
        let Some((parent_name, role)) = guarded_parent_alias(stmt) else {
            crate::ir_lower::stmt::lower_stmt(ctx, stmt);
            continue;
        };
        let initialized = ctx.initialized_slots_snapshot();
        let active = ctx.builder.create_named_block("append.update.active", Vec::new());
        let check = ctx.builder.create_named_block("append.update.check", Vec::new());
        let stale = ctx.builder.create_named_block(if role.is_final() {
            "append.update.cancel"
        } else { "append.update.detach" }, Vec::new());
        let detached_value = ctx.load_local(detached.as_ref().unwrap(), Some(stmt.span));
        ctx.builder.terminate(Terminator::CondBr {
            cond: detached_value.value, then_target: active, then_args: Vec::new(),
            else_target: check, else_args: Vec::new(),
        });
        ctx.builder.position_at_end(check);
        let parent = ctx.load_local(parent_name, Some(stmt.span));
        let live = ctx.emit_value(Op::RuntimeCall, vec![parent.value],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::MixedArrayCellHasLiveOwner)),
            PhpType::Bool, Effects::READS_HEAP, Some(stmt.span));
        ctx.builder.terminate(Terminator::CondBr {
            cond: live.value, then_target: active, then_args: Vec::new(),
            else_target: stale, else_args: Vec::new(),
        });
        ctx.builder.position_at_end(stale);
        ctx.restore_initialized_slots(initialized.clone());
        if role.is_final() {
            for (_, slot, span) in aliases.iter().rev() {
                retire_owned_call_operand(ctx, *slot, *span);
            }
            retire_owned_call_operand(ctx, ctx.local_slots[keeper.as_ref().unwrap()], stmt.span);
            let cancelled = if prefix_increment {
                lower_expr(ctx, &Expr::new(ExprKind::IntLiteral(1), stmt.span))
            } else {
                lower_expr(ctx, &Expr::new(ExprKind::Null, stmt.span))
            };
            store_value_into_temp(ctx, selected.as_ref().unwrap(), PhpType::Mixed, cancelled, stmt.span);
            let current = ctx.initialized_slots_snapshot();
            cancelled_initialized = Some(match cancelled_initialized {
                Some(previous) => current.intersection(&previous).copied().collect(),
                None => current,
            });
            branch_to(ctx, merge.unwrap());
            ctx.builder.position_at_end(active);
            ctx.restore_initialized_slots(initialized);
        } else {
            detach_parent(ctx, parent_name, keeper.as_ref().unwrap(), parent, stmt.span);
            store_expr_into_temp(ctx, detached.as_ref().unwrap(), PhpType::Int,
                &Expr::new(ExprKind::IntLiteral(1), stmt.span), stmt.span);
            let read = (role == crate::names::AppendLiveGuardRole::ParentPre)
                .then(|| prelude.get(position)).flatten()
                .filter(|read| read_parent_alias(read) == Some(parent_name));
            if let Some(Stmt { kind: StmtKind::Assign { name, .. }, .. }) = read {
                // Conversion lost this dimension before its read. Only that missing-key
                // warning is suppressed; subsequent dimensions run on the scratch normally.
                store_expr_into_temp(ctx, name, PhpType::Mixed,
                    &Expr::new(ExprKind::Null, stmt.span), stmt.span);
            }
            let detached_initialized = ctx.initialized_slots_snapshot();
            let continued = ctx.builder.create_named_block("append.update.continue", Vec::new());
            branch_to(ctx, continued);
            ctx.builder.position_at_end(active);
            ctx.restore_initialized_slots(initialized);
            if let Some(read) = read {
                crate::ir_lower::stmt::lower_stmt(ctx, read);
                position += 1;
            }
            let active_initialized = ctx.initialized_slots_snapshot();
            branch_to(ctx, continued);
            ctx.builder.position_at_end(continued);
            ctx.restore_initialized_slots(active_initialized.intersection(&detached_initialized)
                .copied().collect());
        }
    }
    for (name, slot, span) in aliases.into_iter().rev() {
        retire_owned_call_operand(ctx, slot, span);
        ctx.append_update_cell_aliases.remove(&name);
    }
    if let Some(keeper) = keeper {
        retire_owned_call_operand(ctx, ctx.local_slots[&keeper], result.span);
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

/// Selects the compiler-only parent probe without lowering its apparent assignment.
fn guarded_parent_alias(stmt: &Stmt) -> Option<(&str, crate::names::AppendLiveGuardRole)> {
    let StmtKind::Assign { name, value } = &stmt.kind else { return None; };
    let role = crate::names::append_live_guard_role(name)?;
    let ExprKind::Variable(parent) = &value.kind else { return None; };
    (crate::names::append_update_alias_kind(parent) == Some(false)).then_some((parent.as_str(), role))
}

/// Rebinds only the compiler lease, publishing fresh storage before retiring the old box.
fn detach_parent(
    ctx: &mut LoweringContext<'_, '_>, name: &str, keeper: &str,
    old: LoweredValue, span: crate::span::Span,
) {
    let empty = new_pending_container(ctx, name, span);
    let scratch = ctx.box_value_as_mixed(empty, PhpType::Mixed, Some(span));
    let pin = crate::ir_lower::ownership::acquire_if_refcounted(ctx, scratch, Some(span));
    // The keeper stays below the alias records and preserves the scratch when the
    // next child fetch pops and retires this parent lease.
    ctx.store_local(keeper, pin, PhpType::Mixed, Some(span));
    ctx.emit_void(Op::StoreLocal, vec![scratch.value], Some(Immediate::LocalSlot(ctx.local_slots[name])),
        Op::StoreLocal.default_effects(), Some(span));
    let orphan = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let orphan = ctx.local_slots[&orphan];
    // This non-retaining store transfers the old slot's reference, not a borrowed
    // SSA reference. Clear it before release so a throwing destructor cannot double-free it.
    ctx.emit_void(Op::StoreLocal, vec![old.value], Some(Immediate::LocalSlot(orphan)),
        Op::StoreLocal.default_effects(), Some(span));
    ctx.emit_void(Op::ReleaseLocalSlot, Vec::new(), Some(Immediate::LocalSlot(orphan)),
        Op::ReleaseLocalSlot.default_effects(), Some(span));
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
    let empty = new_pending_container(ctx, name, span);
    ctx.emit_void(Op::RuntimeCall, vec![parent.value, empty.value], None,
        crate::ir_lower::effects_lookup::runtime_effects(), Some(span));
    crate::ir_lower::ownership::release_if_owned(ctx, empty, Some(span));
}

/// Allocates the next dimension's indexed or associative scratch representation.
fn new_pending_container(
    ctx: &mut LoweringContext<'_, '_>, name: &str, span: crate::span::Span,
) -> LoweredValue {
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
    ctx.emit_owned_value(
        op,
        Vec::new(),
        Some(Immediate::Capacity(1)),
        container_ty,
        op.default_effects(),
        Some(span),
    )
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
