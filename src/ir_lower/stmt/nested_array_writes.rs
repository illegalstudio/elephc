//! Purpose:
//! Nested write-context array autovivification.
//!
//! Called from:
//! - `crate::ir_lower::stmt`.
//!
//! Key details:
//! - Preserves statement ordering, CFG shape, EIR effects, and ownership contracts.
//! - String parents preserve reference markers and publish their stored child before leaf writes.

use super::*;

/// Writes an expression's captured nested update without repeating its key diagnoses.
pub(crate) fn lower_nested_array_assign_after_read(
    ctx: &mut LoweringContext<'_, '_>,
    target: &Expr,
    value: &Expr,
    span: Span,
) {
    lower_nested_array_assign_inner(ctx, target, value, span, true);
}

/// Lowers a nested array assignment that already carries an expression target.
pub(super) fn lower_nested_array_assign(
    ctx: &mut LoweringContext<'_, '_>,
    target: &Expr,
    value: &Expr,
    span: Span,
) {
    let update = desugared_element_update(value, span, |read| read == target);
    if let Some(ElementUpdate::NullCoalesce { read, default }) = update {
        let mut snapshots = Vec::new();
        let (read, write) = snapshot_nested_update_keys(ctx, read, &mut snapshots, false);
        crate::ir_lower::expr::lower_null_coalesce_update_stmt_to(ctx, &read, &write, default, span);
        retire_update_keys(ctx, snapshots, span);
        return;
    }
    if update.is_none() {
        super::nested_plain_writes::lower_store(ctx, target, value, span);
        return;
    }
    if matches!(&update, Some(ElementUpdate::Compound | ElementUpdate::IncDec { .. }))
        && crate::ir_lower::expr::lower_weak_map_incdec_stmt(ctx, value, span)
    {
        return;
    }
    let static_update = matches!(&update,
        Some(ElementUpdate::Compound | ElementUpdate::IncDec { .. }));
    if static_update
        && (nested_target_has_static_array_root(ctx, target)
            || nested_target_has_local_array_root(ctx, target))
    {
        let mut sources = Vec::new();
        let target = super::nested_write_sources::capture_target(ctx, target, &mut sources);
        let mut value = value.clone();
        if let ExprKind::BinaryOp { right, .. } = &mut value.kind {
            // PHP settles computed dimensions and the RHS before obtaining the writable parent.
            // Bare variable dimensions are still captured by the subsequent read, after the RHS.
            *right = Box::new(super::nested_write_sources::capture(ctx, right, &mut sources));
        }
        let mut snapshots = Vec::new();
        let (read, write) = snapshot_nested_update_keys(ctx, &target, &mut snapshots, true);
        if let Some((read, write)) = bind_concrete_static_update_root(ctx, &read, &write) {
            lower_boxed_static_nested_update(ctx, &read, &write, update.unwrap(), &value, value.span);
        } else if nested_static_root_is_boxed(ctx, &target)
            || nested_target_has_local_array_root(ctx, &target) {
            lower_boxed_static_nested_update(ctx, &read, &write, update.unwrap(), &value, value.span);
        } else {
            let mut value = value.clone();
            match &mut value.kind {
                ExprKind::BinaryOp { left, .. } => *left = Box::new(read),
                ExprKind::Assignment { prelude, .. } => {
                    if let StmtKind::Assign { value, .. } = &mut prelude[0].kind { *value = read; }
                }
                _ => unreachable!("classified element updates have a captured read"),
            }
            lower_nested_array_assign_inner(ctx, &write, &value, span, true);
        }
        retire_update_keys(ctx, snapshots, span);
        super::nested_write_sources::retire(ctx, sources, span);
        return;
    }
    if let Some((_, write)) = bind_concrete_static_update_root(ctx, target, target) {
        lower_nested_array_assign_inner(ctx, &write, value, span, static_update);
    } else { lower_nested_array_assign_inner(ctx, target, value, span, static_update); }
}

/// Releases key snapshots without widening their already allocated PHP storage.
fn retire_update_keys(ctx: &mut LoweringContext<'_, '_>, snapshots: Vec<String>, span: Span) {
    for name in snapshots {
        // Null assignment would widen earlier String reloads into untracked backend casts.
        let slot = ctx.local_slots[&name];
        ctx.release_stored_local_value(&name, slot, Some(span));
        ctx.emit_void(Op::ZeroLocalSlot, Vec::new(), Some(Immediate::LocalSlot(slot)),
            Op::ZeroLocalSlot.default_effects(), Some(span));
    }
}

/// Gives a concrete mixed-element root the existing relocating local writeback protocol.
pub(super) fn bind_concrete_static_update_root(
    ctx: &mut LoweringContext<'_, '_>, read: &Expr, write: &Expr,
) -> Option<(Expr, Expr)> {
    let mut root = read;
    while let ExprKind::ArrayAccess { array, .. } = &root.kind { root = array; }
    let name = bind_mixed_static_array_write_root(ctx, root)?;
    Some((replace_static_update_root(read, &name), replace_static_update_root(write, &name)))
}

/// Includes both indexed and associative concrete storage with boxed element cells.
fn static_root_has_mixed_elements(ty: &PhpType) -> bool {
    match ty.codegen_repr() {
        PhpType::Array(element) => element.codegen_repr() == PhpType::Mixed,
        PhpType::AssocArray { value, .. } => value.codegen_repr() == PhpType::Mixed,
        _ => false,
    }
}

/// Replaces only the static root while preserving captured dimensions and source spans.
fn replace_static_update_root(target: &Expr, name: &str) -> Expr {
    let kind = match &target.kind {
        ExprKind::ArrayAccess { array, index } => ExprKind::ArrayAccess {
            array: Box::new(replace_static_update_root(array, name)), index: index.clone(),
        },
        ExprKind::StaticPropertyAccess { .. } => ExprKind::Variable(name.to_string()),
        _ => unreachable!("a classified static update has a static root"),
    };
    Expr::new(kind, target.span)
}

/// Rebuilds the read with key captures at each dimension, not before the entire chain.
fn snapshot_nested_update_keys(
    ctx: &mut LoweringContext<'_, '_>,
    target: &Expr,
    snapshots: &mut Vec<String>,
    capture_bare_variables: bool,
) -> (Expr, Expr) {
    let ExprKind::ArrayAccess { array, index } = &target.kind else {
        return (target.clone(), target.clone());
    };
    let (read_array, write_array) =
        snapshot_nested_update_keys(ctx, array, snapshots, capture_bare_variables);
    if matches!(index.kind, ExprKind::IntLiteral(_) | ExprKind::StringLiteral(_))
        || (!capture_bare_variables && matches!(index.kind, ExprKind::Variable(_)))
    {
        return (
            Expr::new(ExprKind::ArrayAccess { array: Box::new(read_array), index: index.clone() }, target.span),
            Expr::new(ExprKind::ArrayAccess { array: Box::new(write_array), index: index.clone() }, target.span),
        );
    }
    let key_type = match &index.kind {
        ExprKind::Variable(name) if ctx.is_ref_bound_local(name) => PhpType::Mixed,
        ExprKind::Variable(name) => ctx.local_type(name),
        _ => crate::types::checker::infer_expr_type_syntactic(index),
    };
    let name = ctx.declare_synthetic_php_local(key_type);
    snapshots.push(name.clone());
    let key = Expr::new(ExprKind::Variable(name), index.span);
    let captured = Expr::new(ExprKind::Assignment {
        target: Box::new(key.clone()), value: index.clone(), result_target: None,
        prelude: Vec::new(), conditional_value_temp: None,
    }, index.span);
    (
        Expr::new(ExprKind::ArrayAccess { array: Box::new(read_array), index: Box::new(captured) }, target.span),
        Expr::new(ExprKind::ArrayAccess { array: Box::new(write_array), index: Box::new(key) }, target.span),
    )
}

/// Limits one-level static append fusion to boxed or mixed-element storage.
pub(super) fn static_root_supports_attached_fetch(ctx: &LoweringContext<'_, '_>, root: &Expr) -> bool {
    let ExprKind::StaticPropertyAccess { receiver, property } = &root.kind else { return false; };
    static_property_type(ctx, receiver, property).is_some_and(|ty|
        ty.codegen_repr() == PhpType::Mixed || static_root_has_mixed_elements(&ty))
}

/// Recognizes static PHP array cells whose nested parents support write-context lookup.
fn nested_static_root_is_boxed(ctx: &LoweringContext<'_, '_>, target: &Expr) -> bool {
    match &target.kind {
        ExprKind::ArrayAccess { array, .. } => nested_static_root_is_boxed(ctx, array),
        ExprKind::StaticPropertyAccess { receiver, property } => {
            static_property_type(ctx, receiver, property)
                .is_some_and(|ty| ty.codegen_repr() == PhpType::Mixed)
        }
        _ => false,
    }
}

/// Recognizes local array roots whose indexed parents need attached update traversal.
fn nested_target_has_local_array_root(ctx: &LoweringContext<'_, '_>, target: &Expr) -> bool {
    match &target.kind {
        ExprKind::ArrayAccess { array, .. } => nested_target_has_local_array_root(ctx, array),
        ExprKind::Variable(name) => matches!(ctx.local_type(name).codegen_repr(),
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed),
        _ => false,
    }
}

/// Reads and updates a local or static leaf through the same retained write-context parent cell.
fn lower_boxed_static_nested_update(
    ctx: &mut LoweringContext<'_, '_>,
    read: &Expr,
    write: &Expr,
    update: ElementUpdate<'_>,
    source_value: &Expr,
    span: Span,
) {
    let ExprKind::ArrayAccess { array: read_array, index: read_key } = &read.kind else { unreachable!() };
    let ExprKind::ArrayAccess { array: write_array, index: write_key } = &write.kind else { unreachable!() };
    let incdec = matches!(update, ElementUpdate::IncDec { .. });
    let parent = lower_static_update_parent(ctx, read_array, write_array, span);
    let (pinned, owner) = root_static_update_receiver(ctx, parent, span);
    let string_error = if incdec { "Cannot increment/decrement string offsets" }
        else { "Cannot use assign-op operators with string offsets" };
    let attached = lower_static_update_key_and_guard(ctx, pinned, read_key, string_error, span);
    let current = super::static_false_parent::read_update_leaf(
        ctx, pinned, write_key, read, attached, span,
    );
    let current_ty = ctx.builder.value_php_type(current.value);
    if let ExprKind::Assignment { prelude, .. } = &source_value.kind {
        if let Some(Stmt { kind: StmtKind::Assign { name, .. }, .. }) = prelude.first() {
            // The specialized update bypasses the AST prelude. Keep its postfix result
            // independent of both the mutable leaf cell and the incdec working local.
            let borrowed = ctx.emit_value(Op::Borrow, vec![current.value], None, current_ty.clone(),
                Op::Borrow.default_effects(), Some(span));
            ctx.builder.set_value_ownership(borrowed.value, crate::ir::Ownership::Borrowed);
            let captured = crate::ir_lower::ownership::copy_assignment_value(ctx, borrowed, Some(span));
            ctx.store_call_argument_local(name, captured, current_ty.clone(), Some(span));
        }
    }
    let temp = ctx.declare_synthetic_php_local(current_ty.clone());
    let stored = crate::ir_lower::ownership::acquire_if_refcounted(ctx, current, Some(span));
    ctx.store_local(&temp, stored, current_ty, Some(span));
    if stored.value != current.value && ctx.value_is_owning_temporary(current) {
        crate::ir_lower::ownership::release_if_owned(ctx, current, Some(span));
    }
    let value = match update {
        ElementUpdate::IncDec { increment } => Expr::new(if increment {
            ExprKind::PreIncrement(temp.clone())
        } else { ExprKind::PreDecrement(temp.clone()) }, span),
        ElementUpdate::Compound => {
            let ExprKind::BinaryOp { op, right, .. } = &source_value.kind else { unreachable!() };
            Expr::new(ExprKind::BinaryOp {
                left: Box::new(Expr::new(ExprKind::Variable(temp.clone()), span)),
                op: op.clone(), right: right.clone(),
            }, span)
        }
        ElementUpdate::NullCoalesce { .. } => unreachable!(),
    };
    let value = lower_expr(ctx, &value);
    let key = lower_expr(ctx, write_key);
    let key = crate::ir_lower::ownership::persist_string_byte_view(
        ctx,
        key,
        Some(write_key.span),
    );
    let key_pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &[key.value], write_key.span);
    // A warning handler may replace the stored parent. The pending update still
    // targets the old pinned cell, not a fresh traversal of the static property.
    ctx.emit_void(Op::RuntimeCall, vec![pinned.value, key.value, value.value],
        Some(Immediate::Bool(true)), effects_lookup::runtime_effects(), Some(span));
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, key_pins, write_key.span);
    release_persisted_string_operand(ctx, key, span);
    if let ExprKind::Assignment { target, .. } = &source_value.kind {
        if let ExprKind::Variable(name) = &target.kind {
            // Expression prefix forms yield this captured new value, not another traversal.
            let captured = crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span));
            let ty = ctx.builder.value_php_type(captured.value);
            ctx.store_call_argument_local(name, captured, ty, Some(span));
        }
    }
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
    if let Some(owner) = owner {
        crate::ir_lower::expr::retire_owned_call_operand(ctx, owner, span);
    }
    let null = LoweredValue { value: ctx.builder.emit_const_null(), ir_type: IrType::I64 };
    ctx.unset_local(&temp, null, Some(span));
}

/// Reads an object offset once while retaining array diagnostics and autovivification.
fn lower_static_update_parent(
    ctx: &mut LoweringContext<'_, '_>,
    read: &Expr,
    write: &Expr,
    span: Span,
) -> LoweredValue {
    let ExprKind::ArrayAccess { array: read_array, index: read_key } = &read.kind else {
        return lower_expr(ctx, read);
    };
    let ExprKind::ArrayAccess { array: write_array, index: write_key } = &write.kind else { unreachable!() };
    let receiver = lower_static_update_parent(ctx, read_array, write_array, span);
    let (pinned, owner) = root_static_update_receiver(ctx, receiver, span);
    lower_static_update_key_and_guard(ctx, pinned, read_key,
        "Cannot use string offset as an array", span);
    let probe = lower_array_access_from_lowered_receiver(ctx, pinned, write_key, read);
    let object = ctx.emit_value(Op::TypePredicate, vec![pinned.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Object)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(span));
    let selected = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let slot = ctx.local_slots[&selected];
    ctx.clear_owned_hidden_temp(&selected, Some(span));
    let initialized = ctx.initialized_slots_snapshot();
    let object_block = ctx.builder.create_named_block("static.update.object.parent", Vec::new());
    let array_block = ctx.builder.create_named_block("static.update.array.parent", Vec::new());
    let merge = ctx.builder.create_named_block("static.update.parent.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: object.value, then_target: object_block, then_args: Vec::new(),
        else_target: array_block, else_args: Vec::new(),
    });
    ctx.builder.position_at_end(object_block);
    let parent = retain_static_update_parent(ctx, probe, span);
    let ty = ctx.builder.value_php_type(parent.value);
    ctx.store_call_argument_local(&selected, parent, ty, Some(span));
    branch_to(ctx, merge);
    ctx.builder.position_at_end(array_block);
    ctx.restore_initialized_slots(initialized);
    if ctx.value_is_owning_temporary(probe) {
        crate::ir_lower::ownership::release_if_owned(ctx, probe, Some(span));
    }
    let parent = lower_static_update_array_parent(ctx, pinned, read_array, write_key, read, span);
    let parent = retain_static_update_parent(ctx, parent, span);
    let ty = ctx.builder.value_php_type(parent.value);
    ctx.store_call_argument_local(&selected, parent, ty, Some(span));
    branch_to(ctx, merge);
    ctx.builder.position_at_end(merge);
    // Replace the original parent's topmost record before publishing the child:
    // operand scopes are LIFO, even though the EIR pop names its protected slot.
    if let Some(owner) = owner {
        ctx.emit_void(Op::PopCallOperandOwner, Vec::new(), Some(Immediate::LocalSlot(owner)),
            Op::PopCallOperandOwner.default_effects(), Some(span));
    }
    ctx.emit_void(Op::PushCallOperandOwner, Vec::new(), Some(Immediate::LocalSlot(slot)),
        Op::PushCallOperandOwner.default_effects(), Some(span));
    if let Some(owner) = owner {
        // The child is now unwind-visible if releasing the original parent throws.
        ctx.emit_void(Op::ReleaseLocalSlot, Vec::new(), Some(Immediate::LocalSlot(owner)),
            Op::ReleaseLocalSlot.default_effects(), Some(span));
    }
    let result = ctx.load_local(&selected, Some(span));
    ctx.clear_owned_hidden_temp(&selected, Some(span));
    ctx.emit_void(Op::PopCallOperandOwner, Vec::new(), Some(Immediate::LocalSlot(slot)),
        Op::PopCallOperandOwner.default_effects(), Some(span));
    result
}

/// Keeps a selected child alive without cloning or detaching its writable identity.
fn retain_static_update_parent(
    ctx: &mut LoweringContext<'_, '_>, parent: LoweredValue, span: Span,
) -> LoweredValue {
    if ctx.value_is_owning_temporary(parent) { parent }
    else { crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, parent, Some(span)) }
}

/// Creates an array's writable child after its first read has diagnosed the dimension.
fn lower_static_update_array_parent(
    ctx: &mut LoweringContext<'_, '_>, pinned: LoweredValue, read_array: &Expr,
    write_key: &Expr, read: &Expr, span: Span,
) -> LoweredValue {
    if let ExprKind::Variable(name) = &read_array.kind {
        if let Some(parent) = lower_local_parent_fetch_for_write_inner(ctx, name, write_key, read, true) {
            return parent;
        }
    }
    let key = lower_expr(ctx, write_key);
    let key = crate::ir_lower::ownership::persist_string_byte_view(
        ctx,
        key,
        Some(write_key.span),
    );
    let key_pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &[key.value], write_key.span);
    let parent = ctx.emit_value(Op::RuntimeCall, vec![pinned.value, key.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ArrayFetchForWriteAlreadyDiagnosed)),
        PhpType::Mixed, effects_lookup::runtime_effects(), Some(span));
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, key_pins, write_key.span);
    release_persisted_string_operand(ctx, key, span);
    parent
}

/// Runs captured dimensions and selects the leaf or intermediate string-parent Error.
fn lower_static_update_key_and_guard(
    ctx: &mut LoweringContext<'_, '_>, parent: LoweredValue, read_key: &Expr, string_error: &str, span: Span,
) -> LoweredValue {
    if !matches!(read_key.kind, ExprKind::IntLiteral(_) | ExprKind::StringLiteral(_)) {
        let key = lower_expr(ctx, read_key);
        if ctx.value_is_owning_temporary(key) {
            crate::ir_lower::ownership::release_if_owned(ctx, key, Some(span));
        }
    }
    super::static_false_parent::normalize_null_for_update(ctx, parent, span);
    let attached = super::static_false_parent::normalize_for_update(ctx, parent, span);
    let array = ctx.emit_value(Op::TypePredicate, vec![parent.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Array)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(span));
    let object = ctx.emit_value(Op::TypePredicate, vec![parent.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Object)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(span));
    let valid = ctx.emit_value(Op::IBitOr, vec![array.value, object.value], None, PhpType::Bool,
        Op::IBitOr.default_effects(), Some(span));
    let present = ctx.builder.create_named_block("static.update.array", Vec::new());
    let invalid = ctx.builder.create_named_block("static.update.invalid", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: valid.value, then_target: present, then_args: Vec::new(),
        else_target: invalid, else_args: Vec::new(),
    });
    ctx.builder.position_at_end(invalid);
    let string = ctx.emit_value(Op::TypePredicate, vec![parent.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::String)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(span));
    let string_block = ctx.builder.create_named_block("static.update.string", Vec::new());
    let scalar = ctx.builder.create_named_block("static.update.scalar", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: string.value, then_target: string_block, then_args: Vec::new(),
        else_target: scalar, else_args: Vec::new(),
    });
    ctx.builder.position_at_end(string_block);
    lower_throw_access_error(ctx, string_error, span);
    ctx.builder.position_at_end(scalar);
    lower_throw_access_error(ctx, "Cannot use a scalar value as an array", span);
    ctx.builder.position_at_end(present);
    attached
}

/// Transfers a pending receiver into an unwind-visible owner and returns its borrowed view.
pub(super) fn root_static_update_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: LoweredValue,
    span: Span,
) -> (LoweredValue, Option<crate::ir::LocalSlotId>) {
    let ty = ctx.builder.value_php_type(receiver.value);
    if !crate::ir::Ownership::php_type_needs_lifetime_tracking(&ty) {
        return (receiver, None);
    }
    let receiver = if ctx.value_is_owning_temporary(receiver) {
        receiver
    } else {
        crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, receiver, Some(span))
    };
    let (rooted, owner) = crate::ir_lower::expr::root_owned_call_operand(ctx, receiver, span);
    let ty = ctx.builder.value_php_type(rooted.value);
    let borrowed = ctx.emit_value(Op::Borrow, vec![rooted.value], None, ty,
        Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(borrowed.value, crate::ir::Ownership::Borrowed);
    (borrowed, owner)
}

/// Restricts the update protocol to class-static array chains, leaving ordinary store timing intact.
pub(super) fn nested_target_has_static_array_root(ctx: &LoweringContext<'_, '_>, target: &Expr) -> bool {
    match &target.kind {
        ExprKind::ArrayAccess { array, .. } => nested_target_has_static_array_root(ctx, array),
        ExprKind::StaticPropertyAccess { receiver, property } => {
            static_property_type(ctx, receiver, property)
                .is_some_and(|ty| ty.is_php_array() || is_indexed_array_type(&ty)
                    || static_root_has_mixed_elements(&ty))
        }
        _ => false,
    }
}

/// Writes a captured update after its read, or applies ordinary nested assignment ordering.
fn lower_nested_array_assign_inner(
    ctx: &mut LoweringContext<'_, '_>,
    target: &Expr,
    value: &Expr,
    span: Span,
    key_already_diagnosed: bool,
) {
    // Lowering the FULL target as an expression routes the write through the
    // read helper (`__rt_mixed_array_get`), which returns a detached fresh box
    // whenever the slot storage is not already a boxed Mixed cell; the
    // two-operand cell replacement then mutated a temporary and the write was
    // silently lost (#529). Splitting off the innermost key writes through the
    // parent cell instead (`__rt_mixed_array_set` for Mixed parents,
    // `offsetSet` for ArrayAccess objects), which mutates the aliased
    // container for every slot representation. The parent chain itself is
    // lowered with fetch-for-write semantics so missing or null intermediate
    // elements autovivify as arrays instead of dropping the write (#555).
    if let ExprKind::ArrayAccess { array, index } = &target.kind {
        // PHP reads a plain-variable index at STORE time, and a nested target reads EVERY one of
        // them there: `$a[$i][$i] = ($i = 1)` writes through the index the right-hand side left
        // behind, not the one it started with. Deferring the whole target is sound only when it
        // carries no index EXPRESSION — otherwise a call would move across the right-hand side —
        // so the shape is checked and anything else keeps the original order. Constant
        // propagation applies the same rule ahead of this pass; fixing either alone changes
        // nothing, because the fold has already replaced the variable by the time lowering runs.
        let deferred = nested_target_is_all_bare_variables(target)
            || nested_target_has_static_root(target);
        let mut roots = Vec::new();
        let mut root_operand = |ctx: &mut LoweringContext<'_, '_>, operand: LoweredValue| {
            let (operand, owner) = crate::ir_lower::expr::root_owned_call_operand(ctx, operand, span);
            if let Some(owner) = owner { roots.push(owner); }
            operand
        };
        let value_first = (deferred || key_already_diagnosed).then(|| {
            let value = lower_expr(ctx, value);
            root_operand(ctx, value)
        });
        let parent = if key_already_diagnosed {
            lower_nested_assign_parent_with_diagnosed_key(ctx, array, span, true)
        } else {
            lower_nested_assign_parent(ctx, array, span)
        };
        let parent = if ctx.builder.value_php_type(parent.value).codegen_repr() == PhpType::Mixed {
            let resolved = ctx.emit_owned_value(
                Op::RuntimeCall, vec![parent.value],
                Some(Immediate::RuntimeCall(RuntimeCallTarget::MixedCellResolveForWrite)),
                PhpType::Mixed, effects_lookup::runtime_effects(), Some(span),
            );
            crate::ir_lower::ownership::release_if_owned(ctx, parent, Some(span));
            resolved
        } else { parent };
        let parent = root_operand(ctx, parent);
        let key = lower_expr(ctx, index);
        let key = crate::ir_lower::ownership::persist_string_byte_view(
            ctx,
            key,
            Some(index.span),
        );
        let key = root_operand(ctx, key);
        let value = match value_first {
            Some(value) => value,
            None => {
                let value = lower_expr(ctx, value);
                root_operand(ctx, value)
            }
        };
        if !super::nested_write_guards::guard_nested_write_parent(ctx, parent, Some(index), span) {
            return;
        }
        super::nested_scalar_writes::lower_nested_set(
            ctx,
            parent,
            key,
            value,
            span,
            key_already_diagnosed,
        );
        // The boxed writer borrows all three operands and retains or copies the stored value.
        // Roots also retire computed Mixed values and keep earlier operands safe if a later
        // operand or the write throws into a same-frame catch.
        for root in roots.into_iter().rev() {
            crate::ir_lower::expr::retire_owned_call_operand(ctx, root, span);
        }
        return;
    }
    let target = lower_expr(ctx, target);
    let value = lower_expr(ctx, value);
    ctx.emit_void(
        Op::RuntimeCall,
        vec![target.value, value.value],
        None,
        effects_lookup::runtime_effects(),
        Some(span),
    );
}

/// Recognizes static roots independently of their already captured dimension expressions.
fn nested_target_has_static_root(target: &Expr) -> bool {
    match &target.kind {
        ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::ArrayAccess { array, .. } => nested_target_has_static_root(array),
        _ => false,
    }
}

/// Lowers the parent chain of a nested array assignment with write-context
/// (fetch-for-write) semantics (issue #555): missing indexed elements, null
/// gap slots, boxed `Mixed(null)` elements, and missing hash keys autovivify
/// as empty arrays installed into the parent storage, and the STORED cell is
/// returned so the leaf write lands in the parent container. PHP emits no
/// undefined-key warning for these legal writes, and neither does this path.
/// Shapes without a for-write lowering fall back to the plain read used
/// before (ArrayAccess objects, non-container receivers).
pub(crate) fn lower_nested_assign_parent(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
    span: Span,
) -> LoweredValue {
    lower_nested_assign_parent_with_diagnosed_key_inner(ctx, expr, span, false, false)
}

/// Defers false conversion until a plain store or append has finished evaluating its RHS.
pub(super) fn lower_nested_assign_parent_for_store(
    ctx: &mut LoweringContext<'_, '_>, expr: &Expr, span: Span,
) -> LoweredValue {
    lower_nested_assign_parent_with_diagnosed_key_inner(ctx, expr, span, false, true)
}

/// Reuses key diagnoses when the same parent chain was already read by a captured update.
fn lower_nested_assign_parent_with_diagnosed_key(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
    span: Span,
    key_already_diagnosed: bool,
) -> LoweredValue {
    lower_nested_assign_parent_with_diagnosed_key_inner(ctx, expr, span, key_already_diagnosed, false)
}

/// Traverses an attached parent with the caller's key-diagnostic and conversion timing.
fn lower_nested_assign_parent_with_diagnosed_key_inner(
    ctx: &mut LoweringContext<'_, '_>, expr: &Expr, span: Span,
    key_already_diagnosed: bool, convert_false: bool,
) -> LoweredValue {
    let ExprKind::ArrayAccess { array, index } = &expr.kind else {
        if let ExprKind::Variable(name) = &expr.kind {
            if ctx.local_type(name).codegen_repr() == PhpType::Mixed {
                return load_array_local_for_write(ctx, name, span);
            }
        }
        if let ExprKind::PropertyAccess { object, property } = &expr.kind {
            return crate::ir_lower::expr::lower_nested_assignment_property_source(
                ctx, object, property, expr,
            );
        }
        return lower_expr(ctx, expr);
    };
    // Concrete container locals: ensure the element exists through the
    // runtime wrapper and store the possibly reallocated container back.
    if let ExprKind::Variable(name) = &array.kind {
        let name = name.clone();
        if let Some(parent) = lower_local_parent_fetch_for_write(
            ctx, &name, index, expr, key_already_diagnosed,
        ) {
            return parent;
        }
    }
    // Boxed Mixed receivers: chains recurse with for-write semantics; other
    // receiver shapes evaluate once as plain reads of the receiver cell.
    let receiver = lower_nested_assign_parent_with_diagnosed_key_inner(
        ctx, array, span, key_already_diagnosed, convert_false,
    );
    if convert_false && ctx.builder.value_php_type(receiver.value).codegen_repr() == PhpType::Str {
        crate::ir_lower::ownership::release_if_owned(ctx, receiver, Some(span));
        lower_throw_access_error(ctx, "Cannot use string offset as an array", span);
        return receiver;
    }
    if ctx.builder.value_php_type(receiver.value).codegen_repr() == PhpType::Mixed {
        if convert_false {
            return super::nested_plain_writes::fetch_parent(ctx, receiver, index, expr.span);
        }
        let key = lower_expr(ctx, index);
        let key = crate::ir_lower::ownership::persist_string_byte_view(
            ctx,
            key,
            Some(index.span),
        );
        let key_pins =
            crate::ir_lower::expr::pin_in_flight_owners(ctx, &[key.value], index.span);
        let pending_append = matches!(&array.kind,
            ExprKind::Variable(name) if ctx.append_update_cell_aliases.contains(name));
        let target = match (pending_append, key_already_diagnosed) {
            (false, false) => RuntimeCallTarget::ArrayFetchForWrite,
            (false, true) => RuntimeCallTarget::ArrayFetchForWriteAlreadyDiagnosed,
            (true, false) => RuntimeCallTarget::PendingAppendFetchForWrite,
            (true, true) => RuntimeCallTarget::PendingAppendFetchForWriteAlreadyDiagnosed,
        };
        let parent = ctx.emit_value(
            Op::RuntimeCall,
            vec![receiver.value, key.value],
            Some(Immediate::RuntimeCall(target)),
            PhpType::Mixed,
            effects_lookup::runtime_effects(),
            Some(expr.span),
        );
        crate::ir_lower::expr::unpin_in_flight_owners(ctx, key_pins, index.span);
        release_persisted_string_operand(ctx, key, span);
        if ctx.value_is_owning_temporary(receiver) {
            crate::ir_lower::ownership::release_if_owned(ctx, receiver, Some(span));
        }
        return parent;
    }
    // The receiver is already evaluated but not a boxed Mixed cell: finish as
    // the plain subscript read the pre-#555 lowering produced.
    lower_array_access_from_lowered_receiver(ctx, receiver, index, expr)
}

/// Lowers `$local[key]` as the parent of a nested assignment when the local
/// holds an indexed array or an assoc array:
/// `__rt_array_ensure_elem_for_write` autovivifies the element in write
/// context, the possibly promoted/reallocated container is stored back into
/// the local, and the guaranteed-present element is re-read as the parent
/// cell. Every key shape keeps selected scalar storage attached, keys are evaluated once,
/// and a captured compound diagnosis is reused.
/// Returns `None` for receiver or key shapes without a concrete lowering.
pub(super) fn lower_local_parent_fetch_for_write(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    index: &Expr,
    parent_expr: &Expr,
    key_already_diagnosed: bool,
) -> Option<LoweredValue> {
    lower_local_parent_fetch_for_write_inner(ctx, name, index, parent_expr, key_already_diagnosed)
}

/// Reuses an update's converted numeric dimension without issuing a duplicate warning.
fn lower_local_parent_fetch_for_write_inner(
    ctx: &mut LoweringContext<'_, '_>, name: &str, index: &Expr, parent_expr: &Expr,
    key_already_diagnosed: bool,
) -> Option<LoweredValue> {
    let span = parent_expr.span;
    let local_ty = ctx.local_type(name);
    match local_ty.codegen_repr() {
        PhpType::Array(elem_ty) => {
            match index_expr_key_type(ctx, index) {
                PhpType::Int => {
                    let array = ctx.load_local(name, Some(span));
                    let key = lower_expr(ctx, index);
                    let key = crate::ir_lower::ownership::persist_string_byte_view(
                        ctx,
                        key,
                        Some(index.span),
                    );
                    if matches!(key.ir_type, IrType::Str
                        | IrType::Heap(crate::ir::IrHeapKind::Mixed | crate::ir::IrHeapKind::Union)) {
                        // Syntactic inference defaults variables/calls to integer keys. Their
                        // lowered values are authoritative, and must not be coerced to zero.
                        if ctx.is_ref_bound_local(name) && elem_ty.codegen_repr() == PhpType::Mixed {
                            let cell = lower_indexed_parent_fetch_for_write(
                                ctx, name, array, key, span, key_already_diagnosed,
                            );
                            crate::ir_lower::ownership::release_if_owned(ctx, key, Some(span));
                            return Some(cell);
                        }
                        let assoc_ty = promoted_assoc_array_type(local_ty, PhpType::Mixed);
                        ctx.prepare_mutated_local_owner_for_backend_retire(name, array, assoc_ty.clone(), Some(span));
                        let hash = ctx.emit_value(
                            Op::ArrayToHash, vec![array.value], None, assoc_ty.clone(),
                            Op::ArrayToHash.default_effects(), Some(span),
                        );
                        return Some(lower_hash_parent_fetch_for_write_with_key(
                            ctx,
                            name,
                            hash,
                            assoc_ty,
                            key,
                            span,
                            key_already_diagnosed,
                        ));
                    }
                    // A nested write needs the attached child cell even for a typed string
                    // or boolean element. A plain read loses its writeback identity.
                    let key = coerce_array_key_to_int_at_span(
                        ctx,
                        key,
                        Some(index.span),
                        key_already_diagnosed,
                    );
                    Some(lower_indexed_parent_fetch_for_write(ctx, name, array, key, span, key_already_diagnosed))
                }
                PhpType::Float | PhpType::Bool => {
                    let array = ctx.load_local(name, Some(span));
                    let key = lower_expr(ctx, index);
                    let key = crate::ir_lower::ownership::persist_string_byte_view(
                        ctx,
                        key,
                        Some(index.span),
                    );
                    let key = coerce_array_key_to_int_at_span(
                        ctx,
                        key,
                        Some(index.span),
                        key_already_diagnosed,
                    );
                    Some(lower_indexed_parent_fetch_for_write(ctx, name, array, key, span, key_already_diagnosed))
                }
                PhpType::Str if elem_ty.codegen_repr() == PhpType::Mixed
                    && ctx.is_ref_bound_local(name) => {
                    // Keep reference variadics on the runtime storage-aware fetch. A string
                    // parent can promote storage, but must not reinterpret marker payloads.
                    let array = ctx.load_local(name, Some(span));
                    let key = lower_expr(ctx, index);
                    let key = crate::ir_lower::ownership::persist_string_byte_view(
                        ctx,
                        key,
                        Some(index.span),
                    );
                    let cell = lower_indexed_parent_fetch_for_write(
                        ctx, name, array, key, span, key_already_diagnosed,
                    );
                    release_persisted_string_operand(ctx, key, span);
                    Some(cell)
                }
                PhpType::Str | PhpType::Mixed | PhpType::TaggedScalar | PhpType::Void => {
                    // A string, boxed key, or nullable scalar can select a hash key:
                    // promote to a Mixed-valued hash, whose key conversion
                    // preserves string/numeric keys and null's empty-string key. The
                    // promoted hash flows straight into the ensure call and
                    // is stored back exactly once at the end.
                    let array_value = ctx.load_local(name, Some(span));
                    let assoc_ty = promoted_assoc_array_type(local_ty, PhpType::Mixed);
                    ctx.prepare_mutated_local_owner_for_backend_retire(name, array_value, assoc_ty.clone(), Some(span));
                    let hash = ctx.emit_value(
                        Op::ArrayToHash,
                        vec![array_value.value],
                        None,
                        assoc_ty.clone(),
                        Op::ArrayToHash.default_effects(),
                        Some(span),
                    );
                    Some(lower_hash_parent_fetch_for_write(ctx, name, hash, assoc_ty, index, span, key_already_diagnosed))
                }
                _ => None,
            }
        }
        PhpType::AssocArray { key, .. } => {
            let hash_value = ctx.load_local(name, Some(span));
            let assoc_ty = PhpType::AssocArray { key, value: Box::new(PhpType::Mixed) };
            // Fetch-for-write normalizes typed entries in place and returns their attached
            // Mixed cells. Declare that result contract before publishing the changed hash.
            ctx.prepare_mutated_local_owner(name, hash_value, assoc_ty.clone(), Some(span));
            Some(lower_hash_parent_fetch_for_write(ctx, name, hash_value, assoc_ty, index, span, key_already_diagnosed))
        }
        _ => None,
    }
}

/// Ensures and re-reads a writable indexed or runtime-promoted parent without copying its cell.
/// The ensure helper consumes the loaded owner; prepared storeback must not retire it again.
/// Mixed-backed frame slots are widened before cleanup, while reference cells publish normally.
fn lower_indexed_parent_fetch_for_write(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    array: LoweredValue,
    key: LoweredValue,
    span: Span,
    key_already_diagnosed: bool,
) -> LoweredValue {
    let ensured_ty = PhpType::Array(Box::new(PhpType::Mixed));
    ctx.prepare_mutated_local_owner(name, array, ensured_ty.clone(), Some(span));
    let key_pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &[key.value], span);
    let ensured = ctx.emit_value(
        Op::RuntimeCall,
        vec![array.value, key.value],
        Some(Immediate::RuntimeCall(if key_already_diagnosed {
            RuntimeCallTarget::ArrayFetchForWriteAlreadyDiagnosed
        } else {
            RuntimeCallTarget::ArrayFetchForWrite
        })),
        ensured_ty.clone(),
        effects_lookup::runtime_effects(),
        Some(span),
    );
    ctx.store_prepared_mutated_local(name, ensured, ensured_ty, Some(span));
    let cell = ctx.emit_value(
        Op::ArrayGetForWrite,
        vec![ensured.value, key.value],
        None,
        PhpType::Mixed,
        Op::ArrayGetForWrite.default_effects(),
        Some(span),
    );
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, key_pins, span);
    cell
}

/// Ensures a hash element exists for a nested write parent, stores the
/// possibly reallocated hash back into the local (the previous owner was
/// already released by `prepare_mutated_local_owner`), and re-reads the
/// stored cell (retained by `Op::HashGetForWrite`) as the parent of the leaf write.
/// An earlier read's key diagnosis is preserved during this relocation.
fn lower_hash_parent_fetch_for_write(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    hash_value: LoweredValue,
    assoc_ty: PhpType,
    index: &Expr,
    span: Span,
    key_already_diagnosed: bool,
) -> LoweredValue {
    let key = lower_expr(ctx, index);
    lower_hash_parent_fetch_for_write_with_key(
        ctx,
        name,
        hash_value,
        assoc_ty,
        key,
        span,
        key_already_diagnosed,
    )
}

/// Uses an already evaluated hash key for both the ensure and stored-cell read, then retires it.
fn lower_hash_parent_fetch_for_write_with_key(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    hash_value: LoweredValue,
    assoc_ty: PhpType,
    key: LoweredValue,
    span: Span,
    key_already_diagnosed: bool,
) -> LoweredValue {
    let key = crate::ir_lower::ownership::persist_string_byte_view(ctx, key, Some(span));
    let key_pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &[key.value], span);
    let ensured = ctx.emit_value(
        Op::RuntimeCall,
        vec![hash_value.value, key.value],
        Some(Immediate::RuntimeCall(if key_already_diagnosed {
            RuntimeCallTarget::ArrayFetchForWriteAlreadyDiagnosed
        } else {
            RuntimeCallTarget::ArrayFetchForWrite
        })),
        assoc_ty.clone(),
        effects_lookup::runtime_effects(),
        Some(span),
    );
    ctx.store_prepared_mutated_local(name, ensured, assoc_ty, Some(span));
    let cell = ctx.emit_value(
        Op::HashGetForWrite,
        vec![ensured.value, key.value],
        Some(Immediate::Bool(true)),
        PhpType::Mixed,
        Op::HashGetForWrite.default_effects(),
        Some(span),
    );
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, key_pins, span);
    crate::ir_lower::ownership::release_if_owned(ctx, key, Some(span));
    cell
}

/// Returns true when every part of a nested write target is a bare variable.
///
/// `$a[$i][$j]` qualifies; `$a[f()][$i]` does not. With no index expression there is no side
/// effect whose order could change, so deferring the target past the right-hand side moves only
/// the READ of each variable — which is exactly PHP's store-time rule applied to a chain.
fn nested_target_is_all_bare_variables(target: &Expr) -> bool {
    match &target.kind {
        ExprKind::Variable(_) => true,
        ExprKind::ArrayAccess { array, index } => {
            matches!(index.kind, ExprKind::Variable(_))
                && nested_target_is_all_bare_variables(array)
        }
        _ => false,
    }
}
