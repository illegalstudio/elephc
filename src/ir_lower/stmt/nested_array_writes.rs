//! Purpose:
//! Nested write-context array autovivification.
//!
//! Called from:
//! - `crate::ir_lower::stmt`.
//!
//! Key details:
//! - Preserves statement ordering, CFG shape, EIR effects, and ownership contracts.

use super::*;
use crate::ir::IrHeapKind;

/// Lowers a nested array assignment that already carries an expression target.
pub(super) fn lower_nested_array_assign(
    ctx: &mut LoweringContext<'_, '_>,
    target: &Expr,
    value: &Expr,
    span: Span,
) {
    let update = desugared_element_update(value, span, |read| read == target);
    if matches!(update, Some(ElementUpdate::Compound | ElementUpdate::IncDec { .. }))
        && nested_target_has_static_array_root(ctx, target)
    {
        let mut snapshots = Vec::new();
        let (read, write) = snapshot_nested_update_keys(ctx, target, &mut snapshots);
        if let Some((read, write)) = bind_concrete_static_update_root(ctx, &read, &write) {
            lower_boxed_static_nested_update(ctx, &read, &write, update.unwrap(), value, value.span);
        } else if nested_static_root_is_boxed(ctx, target) {
            lower_boxed_static_nested_update(ctx, &read, &write, update.unwrap(), value, value.span);
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
        for name in snapshots {
            // Null assignment would widen the entire frame slot, turning earlier String
            // reloads into detached backend casts that lowering never had a chance to release.
            let slot = ctx.local_slots[&name];
            ctx.release_stored_local_value(&name, slot, Some(span));
            ctx.emit_void(Op::ZeroLocalSlot, Vec::new(), Some(Immediate::LocalSlot(slot)),
                Op::ZeroLocalSlot.default_effects(), Some(span));
        }
        return;
    }
    if let Some((_, write)) = bind_concrete_static_update_root(ctx, target, target) {
        lower_nested_array_assign_inner(ctx, &write, value, span, false);
    } else { lower_nested_array_assign_inner(ctx, target, value, span, false); }
}

/// Gives a concrete mixed-element root the existing relocating local writeback protocol.
fn bind_concrete_static_update_root(
    ctx: &mut LoweringContext<'_, '_>, read: &Expr, write: &Expr,
) -> Option<(Expr, Expr)> {
    let mut root = read;
    while let ExprKind::ArrayAccess { array, .. } = &root.kind { root = array; }
    let ExprKind::StaticPropertyAccess { receiver, property } = &root.kind else { return None; };
    let ty = static_property_type(ctx, receiver, property)?;
    if !static_root_has_mixed_elements(&ty) {
        return None;
    }
    let name = ctx.declare_synthetic_php_local(ty);
    crate::ir_lower::expr::lower_ref_assign_static_property(ctx, &name, root, root.span);
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
) -> (Expr, Expr) {
    let ExprKind::ArrayAccess { array, index } = &target.kind else {
        return (target.clone(), target.clone());
    };
    let (read_array, write_array) = snapshot_nested_update_keys(ctx, array, snapshots);
    if matches!(index.kind, ExprKind::IntLiteral(_) | ExprKind::StringLiteral(_)) {
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

/// Reads and updates a leaf through the same retained write-context parent cell.
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
    let parent = lower_static_update_parent(ctx, read_array, write_array, span);
    let (pinned, owner) = root_static_update_receiver(ctx, parent, span);
    lower_static_update_key_and_guard(ctx, pinned, read_key, span);
    let current = lower_array_access_from_lowered_receiver(ctx, pinned, write_key, read);
    let current_ty = ctx.builder.value_php_type(current.value);
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
    // A warning handler may replace the stored parent. The pending update still
    // targets the old pinned cell, not a fresh traversal of the static property.
    ctx.emit_void(Op::RuntimeCall, vec![pinned.value, key.value, value.value],
        Some(Immediate::Bool(true)), effects_lookup::runtime_effects(), Some(span));
    release_persisted_string_operand(ctx, key, span);
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
    if let Some(owner) = owner {
        crate::ir_lower::expr::retire_owned_call_operand(ctx, owner, span);
    }
    let null = LoweredValue { value: ctx.builder.emit_const_null(), ir_type: IrType::I64 };
    ctx.unset_local(&temp, null, Some(span));
}

/// Diagnoses each intermediate read, then creates and retains its writable stored cell.
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
    lower_static_update_key_and_guard(ctx, pinned, read_key, span);
    let probe = lower_array_access_from_lowered_receiver(ctx, pinned, write_key, read);
    if ctx.value_is_owning_temporary(probe) {
        crate::ir_lower::ownership::release_if_owned(ctx, probe, Some(span));
    }
    if let ExprKind::Variable(name) = &read_array.kind {
        if let Some(parent) = lower_local_parent_fetch_for_write_inner(ctx, name, write_key, read, true) {
            if let Some(owner) = owner {
                crate::ir_lower::expr::retire_owned_call_operand(ctx, owner, span);
            }
            return parent;
        }
    }
    let key = lower_expr(ctx, write_key);
    let parent = ctx.emit_value(Op::RuntimeCall, vec![pinned.value, key.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ArrayFetchForWriteAlreadyDiagnosed)),
        PhpType::Mixed, effects_lookup::runtime_effects(), Some(span));
    release_persisted_string_operand(ctx, key, span);
    if let Some(owner) = owner {
        crate::ir_lower::expr::retire_owned_call_operand(ctx, owner, span);
    }
    parent
}

/// Runs each captured dimension before rejecting a scalar parent with a catchable Error.
fn lower_static_update_key_and_guard(
    ctx: &mut LoweringContext<'_, '_>, parent: LoweredValue, read_key: &Expr, span: Span,
) {
    if !matches!(read_key.kind, ExprKind::IntLiteral(_) | ExprKind::StringLiteral(_)) {
        let key = lower_expr(ctx, read_key);
        if ctx.value_is_owning_temporary(key) {
            crate::ir_lower::ownership::release_if_owned(ctx, key, Some(span));
        }
    }
    let array = ctx.emit_value(Op::TypePredicate, vec![parent.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Array)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(span));
    let object = ctx.emit_value(Op::TypePredicate, vec![parent.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Object)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(span));
    let valid = ctx.emit_value(Op::IBitOr, vec![array.value, object.value], None, PhpType::Bool,
        Op::IBitOr.default_effects(), Some(span));
    let present = ctx.builder.create_named_block("static.update.array", Vec::new());
    let scalar = ctx.builder.create_named_block("static.update.scalar", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: valid.value, then_target: present, then_args: Vec::new(),
        else_target: scalar, else_args: Vec::new(),
    });
    ctx.builder.position_at_end(scalar);
    lower_throw_access_error(ctx, "Cannot use a scalar value as an array", span);
    ctx.builder.position_at_end(present);
}

/// Transfers a pending receiver into an unwind-visible owner and returns its borrowed view.
fn root_static_update_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: LoweredValue,
    span: Span,
) -> (LoweredValue, Option<crate::ir::LocalSlotId>) {
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
fn nested_target_has_static_array_root(ctx: &LoweringContext<'_, '_>, target: &Expr) -> bool {
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
        let deferred = nested_target_is_all_bare_variables(target);
        let value_first = (deferred || key_already_diagnosed).then(|| lower_expr(ctx, value));
        let parent = if key_already_diagnosed {
            lower_nested_assign_parent_with_diagnosed_key(ctx, array, span, true)
        } else { lower_nested_assign_parent(ctx, array, span) };
        let key = lower_expr(ctx, index);
        let value = match value_first {
            Some(value) => value,
            None => lower_expr(ctx, value),
        };
        ctx.emit_void(
            Op::RuntimeCall,
            vec![parent.value, key.value, value.value],
            key_already_diagnosed.then_some(Immediate::Bool(true)),
            effects_lookup::runtime_effects(),
            Some(span),
        );
        release_persisted_string_operand(ctx, key, span);
        if matches!(value.ir_type, IrType::Heap(IrHeapKind::Mixed | IrHeapKind::Union))
            && ctx.value_is_owning_temporary(value)
        {
            crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
        } else {
            release_persisted_string_operand(ctx, value, span);
        }
        // Parent subscript reads of Mixed/refcounted elements are owning
        // temporaries (`ArrayGet`/`HashGet`/`RuntimeCall` return a +1 caller
        // reference — fresh, retained, or installed by autovivification). The
        // set helper mutates through the cell/object without consuming that
        // reference, so release it here. Non-owning parents (plain locals,
        // `$this`) are left to normal scope cleanup.
        if ctx.value_is_owning_temporary(parent) {
            crate::ir_lower::ownership::release_if_owned(ctx, parent, Some(span));
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

/// Lowers the parent chain of a nested array assignment with write-context
/// (fetch-for-write) semantics (issue #555): missing indexed elements, null
/// gap slots, boxed `Mixed(null)` elements, and missing hash keys autovivify
/// as empty arrays installed into the parent storage, and the STORED cell is
/// returned so the leaf write lands in the parent container. PHP emits no
/// undefined-key warning for these legal writes, and neither does this path.
/// Shapes without a for-write lowering fall back to the plain read used
/// before (ArrayAccess objects, non-container receivers).
pub(super) fn lower_nested_assign_parent(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
    span: Span,
) -> LoweredValue {
    lower_nested_assign_parent_with_diagnosed_key(ctx, expr, span, false)
}

/// Reuses key diagnoses when the same parent chain was already read by a captured update.
fn lower_nested_assign_parent_with_diagnosed_key(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
    span: Span,
    key_already_diagnosed: bool,
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
        if let Some(parent) = lower_local_parent_fetch_for_write(ctx, &name, index, expr) {
            return parent;
        }
    }
    // Boxed Mixed receivers: chains recurse with for-write semantics; other
    // receiver shapes evaluate once as plain reads of the receiver cell.
    let receiver = lower_nested_assign_parent_with_diagnosed_key(ctx, array, span, key_already_diagnosed);
    if ctx.builder.value_php_type(receiver.value).codegen_repr() == PhpType::Mixed {
        let key = lower_expr(ctx, index);
        let parent = ctx.emit_value(
            Op::RuntimeCall,
            vec![receiver.value, key.value],
            Some(Immediate::RuntimeCall(if key_already_diagnosed {
                RuntimeCallTarget::ArrayFetchForWriteAlreadyDiagnosed
            } else { RuntimeCallTarget::ArrayFetchForWrite })),
            PhpType::Mixed,
            effects_lookup::runtime_effects(),
            Some(expr.span),
        );
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
/// holds a concrete container (`array<mixed>` or a Mixed-valued assoc array):
/// `__rt_array_ensure_elem_for_write` autovivifies the element in write
/// context, the possibly promoted/reallocated container is stored back into
/// the local, and the guaranteed-present element is re-read as the parent
/// cell. Returns `None` for shapes without a concrete for-write lowering
/// (typed element arrays, non-Int/Str key expressions).
pub(super) fn lower_local_parent_fetch_for_write(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    index: &Expr,
    parent_expr: &Expr,
) -> Option<LoweredValue> {
    lower_local_parent_fetch_for_write_inner(ctx, name, index, parent_expr, false)
}

/// Reuses an update's converted numeric dimension without issuing a duplicate warning.
fn lower_local_parent_fetch_for_write_inner(
    ctx: &mut LoweringContext<'_, '_>, name: &str, index: &Expr, parent_expr: &Expr,
    key_already_diagnosed: bool,
) -> Option<LoweredValue> {
    let span = parent_expr.span;
    let local_ty = ctx.local_type(name);
    match local_ty.codegen_repr() {
        PhpType::Array(elem_ty)
            if elem_ty.codegen_repr() == PhpType::Mixed
                || is_empty_indexed_array_element(elem_ty.as_ref()) =>
        {
            let key_ty = match &index.kind {
                ExprKind::Variable(name) if ctx.local_type(name).codegen_repr() == PhpType::Str => PhpType::Str,
                _ => index_expr_key_type(ctx, index),
            };
            match key_ty {
                PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void => {
                    let array_value = ctx.load_local(name, Some(span));
                    let key = lower_expr(ctx, index);
                    let key = coerce_array_key_to_int_at_span(ctx, key, Some(index.span), key_already_diagnosed);
                    // Autovivification makes the element type effectively
                    // Mixed even when the array started empty-typed. The
                    // ensure call consumes the loaded container (in-place
                    // mutation or realloc), so the previous boxed owner of a
                    // Mixed-storage slot must be released up front and the
                    // storeback must not release again.
                    let ensured_ty = PhpType::Array(Box::new(PhpType::Mixed));
                    ctx.prepare_mutated_local_owner(name, array_value, ensured_ty.clone(), Some(span));
                    let ensured = ctx.emit_value(
                        Op::RuntimeCall,
                        vec![array_value.value, key.value],
                        Some(Immediate::RuntimeCall(RuntimeCallTarget::ArrayFetchForWrite)),
                        ensured_ty.clone(),
                        effects_lookup::runtime_effects(),
                        Some(span),
                    );
                    ctx.store_prepared_mutated_local(name, ensured, ensured_ty, Some(span));
                    // The element now exists: the in-bounds read returns the
                    // STORED cell (retained) without an undefined-key warning.
                    let cell = ctx.emit_value(
                        Op::ArrayGetForWrite,
                        vec![ensured.value, key.value],
                        None,
                        PhpType::Mixed,
                        Op::ArrayGetForWrite.default_effects(),
                        Some(span),
                    );
                    Some(cell)
                }
                PhpType::Str => {
                    // A literal string key on an indexed local is always a
                    // hash key: promote the local to a Mixed-valued hash
                    // first (mirrors `lower_string_key_array_promotion`),
                    // then ensure the element through the hash path. The
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
                    Some(lower_hash_parent_fetch_for_write(ctx, name, hash, assoc_ty, index, span))
                }
                _ => None,
            }
        }
        PhpType::AssocArray { value, .. } if value.codegen_repr() == PhpType::Mixed => {
            let hash_value = ctx.load_local(name, Some(span));
            let assoc_ty = ctx.local_type(name);
            ctx.prepare_mutated_local_owner(name, hash_value, assoc_ty.clone(), Some(span));
            Some(lower_hash_parent_fetch_for_write(ctx, name, hash_value, assoc_ty, index, span))
        }
        _ => None,
    }
}

/// Ensures a hash element exists for a nested write parent, stores the
/// possibly reallocated hash back into the local (the previous owner was
/// already released by `prepare_mutated_local_owner`), and re-reads the
/// stored cell (retained by `Op::HashGetForWrite`) as the parent of the leaf write.
pub(super) fn lower_hash_parent_fetch_for_write(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    hash_value: LoweredValue,
    assoc_ty: PhpType,
    index: &Expr,
    span: Span,
) -> LoweredValue {
    let key = lower_expr(ctx, index);
    let ensured = ctx.emit_value(
        Op::RuntimeCall,
        vec![hash_value.value, key.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ArrayFetchForWrite)),
        assoc_ty.clone(),
        effects_lookup::runtime_effects(),
        Some(span),
    );
    ctx.store_prepared_mutated_local(name, ensured, assoc_ty, Some(span));
    ctx.emit_value(
        Op::HashGetForWrite,
        vec![ensured.value, key.value],
        Some(Immediate::Bool(true)),
        PhpType::Mixed,
        Op::HashGetForWrite.default_effects(),
        Some(span),
    )
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
