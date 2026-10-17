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
    if let ExprKind::ArrayAccess { array, index } = &target.kind {
        // PHP reads a plain-variable index at STORE time, and a nested target reads EVERY one of
        // them there: `$a[$i][$i] = ($i = 1)` writes through the index the right-hand side left
        // behind, not the one it started with. Deferring the whole target is sound only when it
        // carries no index EXPRESSION — otherwise a call would move across the right-hand side —
        // so the shape is checked and anything else keeps the original order. Constant
        // propagation applies the same rule ahead of this pass; fixing either alone changes
        // nothing, because the fold has already replaced the variable by the time lowering runs.
        let deferred = nested_target_is_all_bare_variables(target);
        let value_first = deferred.then(|| {
            let value = lower_expr(ctx, value);
            let pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &[value.value], span);
            (value, pins)
        });
        let parent = lower_nested_assign_parent(ctx, array, span);
        let parent = if ctx.builder.value_php_type(parent.value).codegen_repr() == PhpType::Mixed {
            let resolved = ctx.emit_owned_value(
                Op::RuntimeCall, vec![parent.value],
                Some(Immediate::RuntimeCall(RuntimeCallTarget::MixedCellResolveForWrite)),
                PhpType::Mixed, effects_lookup::runtime_effects(), Some(span),
            );
            crate::ir_lower::ownership::release_if_owned(ctx, parent, Some(span));
            resolved
        } else { parent };
        let parent_pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &[parent.value], span);
        let key = lower_expr(ctx, index);
        let key_pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &[key.value], span);
        let (value, value_pins) = match value_first {
            Some(value) => value,
            None => {
                let value = lower_expr(ctx, value);
                let pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &[value.value], span);
                (value, pins)
            }
        };
        if !super::nested_write_guards::guard_nested_write_parent(ctx, parent, Some(index), span) {
            return;
        }
        super::nested_scalar_writes::lower_nested_set(ctx, parent, key, value, span);
        if deferred {
            crate::ir_lower::expr::unpin_in_flight_owners(ctx, key_pins, span);
            crate::ir_lower::expr::unpin_in_flight_owners(ctx, parent_pins, span);
            crate::ir_lower::expr::unpin_in_flight_owners(ctx, value_pins, span);
        } else {
            crate::ir_lower::expr::unpin_in_flight_owners(ctx, value_pins, span);
            crate::ir_lower::expr::unpin_in_flight_owners(ctx, key_pins, span);
            crate::ir_lower::expr::unpin_in_flight_owners(ctx, parent_pins, span);
        }
        release_persisted_string_operand(ctx, key, span);
        // The setter boxes/retains its stored value; retire the caller's temporary owner.
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
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
pub(crate) fn lower_nested_assign_parent(
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
        let target = match &array.kind {
            ExprKind::Variable(name) if ctx.append_update_cell_aliases.contains(name) => {
                RuntimeCallTarget::PendingAppendFetchForWrite
            }
            _ => RuntimeCallTarget::ArrayFetchForWrite,
        };
        let parent = ctx.emit_value(
            Op::RuntimeCall,
            vec![receiver.value, key.value],
            Some(Immediate::RuntimeCall(target)),
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
/// holds an indexed array or an assoc array:
/// `__rt_array_ensure_elem_for_write` autovivifies the element in write
/// context, the possibly promoted/reallocated container is stored back into
/// the local, and the guaranteed-present element is re-read as the parent
/// cell. Every key shape keeps selected scalar storage attached, and keys are evaluated once.
/// Returns `None` for receiver or key shapes without a concrete lowering.
pub(super) fn lower_local_parent_fetch_for_write(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    index: &Expr,
    parent_expr: &Expr,
) -> Option<LoweredValue> {
    let span = parent_expr.span;
    let local_ty = ctx.local_type(name);
    match local_ty.codegen_repr() {
        PhpType::Array(elem_ty) => {
            match index_expr_key_type(ctx, index) {
                PhpType::Int => {
                    let array = ctx.load_local(name, Some(span));
                    let key = lower_expr(ctx, index);
                    if matches!(key.ir_type, IrType::Str
                        | IrType::Heap(crate::ir::IrHeapKind::Mixed | crate::ir::IrHeapKind::Union)) {
                        // Syntactic inference defaults variables/calls to integer keys. Their
                        // lowered values are authoritative, and must not be coerced to zero.
                        if ctx.is_ref_bound_local(name) && elem_ty.codegen_repr() == PhpType::Mixed {
                            let cell = lower_indexed_parent_fetch_for_write(ctx, name, array, key, span);
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
                            ctx, name, hash, assoc_ty, key, span,
                        ));
                    }
                    // A nested write needs the attached child cell even for a typed string
                    // or boolean element. A plain read loses its writeback identity.
                    let key = coerce_array_key_to_int_at_span(ctx, key, Some(index.span), false);
                    Some(lower_indexed_parent_fetch_for_write(ctx, name, array, key, span))
                }
                PhpType::Str if elem_ty.codegen_repr() == PhpType::Mixed
                    && ctx.is_ref_bound_local(name) => {
                    // Keep reference variadics on the runtime storage-aware fetch. A string
                    // parent can promote storage, but must not reinterpret marker payloads.
                    let array = ctx.load_local(name, Some(span));
                    let key = lower_expr(ctx, index);
                    let cell = lower_indexed_parent_fetch_for_write(ctx, name, array, key, span);
                    release_persisted_string_operand(ctx, key, span);
                    Some(cell)
                }
                PhpType::Str => {
                    // A literal string key on an indexed local is always a
                    // hash key: promote the local to a Mixed-valued hash
                    // first (mirrors `lower_array_key_hash_promotion`),
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
        PhpType::AssocArray { key, .. } => {
            let hash_value = ctx.load_local(name, Some(span));
            let assoc_ty = PhpType::AssocArray { key, value: Box::new(PhpType::Mixed) };
            // Fetch-for-write normalizes typed entries in place and returns their attached
            // Mixed cells. Declare that result contract before publishing the changed hash.
            ctx.prepare_mutated_local_owner(name, hash_value, assoc_ty.clone(), Some(span));
            Some(lower_hash_parent_fetch_for_write(ctx, name, hash_value, assoc_ty, index, span))
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
) -> LoweredValue {
    let ensured_ty = PhpType::Array(Box::new(PhpType::Mixed));
    ctx.prepare_mutated_local_owner(name, array, ensured_ty.clone(), Some(span));
    let ensured = ctx.emit_value(
        Op::RuntimeCall,
        vec![array.value, key.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ArrayFetchForWrite)),
        ensured_ty.clone(),
        effects_lookup::runtime_effects(),
        Some(span),
    );
    ctx.store_prepared_mutated_local(name, ensured, ensured_ty, Some(span));
    ctx.emit_value(
        Op::ArrayGetForWrite,
        vec![ensured.value, key.value],
        None,
        PhpType::Mixed,
        Op::ArrayGetForWrite.default_effects(),
        Some(span),
    )
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
    lower_hash_parent_fetch_for_write_with_key(ctx, name, hash_value, assoc_ty, key, span)
}

/// Uses an already evaluated hash key for both the ensure and stored-cell read, then retires it.
fn lower_hash_parent_fetch_for_write_with_key(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    hash_value: LoweredValue,
    assoc_ty: PhpType,
    key: LoweredValue,
    span: Span,
) -> LoweredValue {
    let ensured = ctx.emit_value(
        Op::RuntimeCall,
        vec![hash_value.value, key.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ArrayFetchForWrite)),
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
