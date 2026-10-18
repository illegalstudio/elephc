//! Purpose:
//! Nested write-context array autovivification.
//!
//! Called from:
//! - `crate::ir_lower::stmt`.
//!
//! Key details:
//! - Preserves statement ordering, CFG shape, EIR effects, and ownership contracts.

use super::*;

/// Lowers a nested array assignment that already carries an expression target.
pub(super) fn lower_nested_array_assign(
    ctx: &mut LoweringContext<'_, '_>,
    target: &Expr,
    value: &Expr,
    span: Span,
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
    let stable_root = nested_target_has_stable_root(target);
    let original_target = target;
    let mut key_owners = Vec::new();
    let prepared_target = stable_root.then(|| snapshot_nested_write_indices(ctx, target, &mut key_owners));
    let target = prepared_target.as_ref().unwrap_or(target);
    if let ExprKind::ArrayAccess { array, index } = &target.kind {
        // Computed key results were captured in source order before the RHS. Bare variable
        // operands stay in the place and are read at store time, including mixed chains.
        // Fetch stable local/property/static roots only after the value. Pin that value over
        // COW and callbacks, and rebox a concrete local if an operand changed its representation.
        let root_written = stable_root
            && (nested_value_may_write_root(ctx, original_target, value)
                || nested_value_may_write_root(ctx, original_target, original_target));
        let mut pinned_slot = None;
        let value_first = if stable_root {
            let lowered = lower_expr(ctx, value);
            let (pinned, slot) = crate::ir_lower::expr::root_call_operand(ctx, lowered, span);
            pinned_slot = slot;
            if root_written {
                rebox_nested_root_local(ctx, target, span);
            }
            Some(pinned)
        } else {
            None
        };
        let parent = lower_nested_assign_parent(ctx, array, span);
        let key = lower_expr(ctx, index);
        let value = match value_first {
            Some(value) => value,
            None => lower_expr(ctx, value),
        };
        ctx.emit_void(
            Op::RuntimeCall,
            vec![parent.value, key.value, value.value],
            None,
            effects_lookup::runtime_effects(),
            Some(span),
        );
        release_persisted_string_operand(ctx, key, span);
        if let Some(slot) = pinned_slot {
            crate::ir_lower::expr::retire_owned_call_operand(ctx, slot, span);
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
        for slot in key_owners.into_iter().rev() {
            crate::ir_lower::expr::retire_owned_call_operand(ctx, slot, span);
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
    let receiver = lower_nested_assign_parent(ctx, array, span);
    if ctx.builder.value_php_type(receiver.value).codegen_repr() == PhpType::Mixed {
        let key = lower_expr(ctx, index);
        let parent = ctx.emit_value(
            Op::RuntimeCall,
            vec![receiver.value, key.value],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::ArrayFetchForWrite)),
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
    let span = parent_expr.span;
    let local_ty = ctx.local_type(name);
    match local_ty.codegen_repr() {
        PhpType::Array(elem_ty)
            if elem_ty.codegen_repr() == PhpType::Mixed
                || is_empty_indexed_array_element(elem_ty.as_ref()) =>
        {
            match index_expr_key_type(ctx, index) {
                PhpType::Int => {
                    let array_value = ctx.load_local(name, Some(span));
                    let key = lower_expr(ctx, index);
                    let key = coerce_array_key_to_int_at_span(ctx, key, Some(index.span), false);
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

/// Puts a nested write's root local back into boxed `Mixed` storage after a value that
/// reassigned it to a concrete array.
///
/// The checker accepted the nested write for a `Mixed` root, the only kind the nested writer
/// supports (besides `ArrayAccess`). A value such as `($m = ["a" => [...]])` retypes the local to
/// a concrete hash before the parent chain is fetched, which that writer would refuse. Boxing
/// retains the payload, and the store releases the slot's previous owner.
fn rebox_nested_root_local(ctx: &mut LoweringContext<'_, '_>, target: &Expr, span: Span) {
    let mut root = target;
    while let ExprKind::ArrayAccess { array, .. } = &root.kind {
        root = array;
    }
    let ExprKind::Variable(name) = &root.kind else {
        return;
    };
    let ty = ctx.local_type(name).codegen_repr();
    if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return;
    }
    let current = ctx.load_local(name, Some(span));
    let boxed = ctx.box_value_as_mixed(current, PhpType::Mixed, Some(span));
    ctx.store_call_argument_local(name, boxed, PhpType::Mixed, Some(span));
}

/// Returns true when the value of a nested write may write the chain's root, so the value has to
/// run before the parent chain is fetched. The root is the innermost container: a local, a
/// property or a static property, each checked by its own `element_write_order` gate.
fn nested_value_may_write_root(ctx: &LoweringContext<'_, '_>, target: &Expr, value: &Expr) -> bool {
    let mut root = target;
    while let ExprKind::ArrayAccess { array, .. } = &root.kind {
        root = array;
    }
    match &root.kind {
        ExprKind::Variable(name) => {
            super::element_write_order::element_write_operands_may_write_receiver(ctx, name, &[value])
        }
        ExprKind::PropertyAccess { property, .. } => {
            super::element_write_order::property_element_operands_may_write_property(property, &[value])
        }
        ExprKind::StaticPropertyAccess { property, .. } => {
            super::element_write_order::static_property_element_operands_may_write_property(
                property,
                &[value],
            )
        }
        _ => false,
    }
}

/// Returns whether fetching the root later moves no effectful base expression.
///
/// Computed indices are handled separately; locals, simple object properties and static
/// properties can be fetched after the operands without replaying a receiver call.
fn nested_target_has_stable_root(target: &Expr) -> bool {
    match &target.kind {
        ExprKind::Variable(_) | ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::PropertyAccess { object, .. } => {
            matches!(object.kind, ExprKind::Variable(_) | ExprKind::This)
        }
        ExprKind::ArrayAccess { array, .. } => nested_target_has_stable_root(array),
        _ => false,
    }
}

/// Captures computed keys before the RHS, leaving literals and PHP variable operands late.
fn snapshot_nested_write_indices(
    ctx: &mut LoweringContext<'_, '_>,
    target: &Expr,
    owners: &mut Vec<crate::ir::LocalSlotId>,
) -> Expr {
    match &target.kind {
        ExprKind::ArrayAccess { array, index } => {
            let array = snapshot_nested_write_indices(ctx, array, owners);
            let index = if matches!(index.kind,
                ExprKind::Variable(_) | ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_)
                | ExprKind::StringLiteral(_) | ExprKind::BoolLiteral(_) | ExprKind::Null)
            {
                index.as_ref().clone()
            } else {
                let value = lower_expr(ctx, index);
                let (value, owner) = crate::ir_lower::expr::root_named_call_operand(ctx, value, index.span);
                let name = if let Some((name, slot)) = owner {
                    owners.push(slot);
                    name
                } else {
                    let ty = ctx.builder.value_php_type(value.value);
                    let name = ctx.declare_hidden_temp(ty.clone());
                    ctx.store_local(&name, value, ty, Some(index.span));
                    name
                };
                Expr::new(ExprKind::Variable(name), index.span)
            };
            Expr::new(ExprKind::ArrayAccess {
                array: Box::new(array), index: Box::new(index),
            }, target.span)
        }
        _ => target.clone(),
    }
}
