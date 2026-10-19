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
    let update = desugared_element_update(value, span, |read| read == target);
    if matches!(&update, Some(ElementUpdate::Compound))
        && crate::ir_lower::expr::lower_weak_map_incdec_stmt(ctx, value, span)
    {
        return;
    }
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
        // A bare-variable right-hand side is also read first when no key has an effect
        // (literals and plain variables only): then reading it early cannot be observed, and it
        // lets the value own its reference (below). With an effectful key (`$a[$i++]['k'] = $i`)
        // PHP evaluates the keys first, so the original order is kept.
        let deferred = nested_target_is_all_bare_variables(target)
            || (matches!(value.kind, ExprKind::Variable(_)) && nested_target_keys_are_pure(target));
        let mut roots = Vec::new();
        let mut root_operand = |ctx: &mut LoweringContext<'_, '_>, operand: LoweredValue| {
            let (operand, owner) = crate::ir_lower::expr::root_owned_call_operand(ctx, operand, span);
            if let Some(owner) = owner { roots.push(owner); }
            operand
        };
        // A borrowed container value (`$b[0][0] = $b`) must hold its own reference BEFORE the
        // parent chain is fetched for write: otherwise the fetch sees a unique container,
        // mutates it in place and stores the container inside itself (an endless cycle, a
        // crash in `var_dump`), where PHP copies the right-hand side first. That extra
        // reference is released explicitly after the write; every other operand is rooted.
        let value_is_variable = matches!(value.kind, ExprKind::Variable(_));
        let value_first = deferred.then(|| {
            let lowered = lower_expr(ctx, value);
            if value_is_variable {
                let (lowered, acquired) = own_variable_container_value(ctx, lowered, span);
                if acquired {
                    return (lowered, true);
                }
                return (root_operand(ctx, lowered), false);
            }
            (root_operand(ctx, lowered), false)
        });
        let acquired_value = value_first.and_then(|(v, acquired)| acquired.then_some(v.value));
        let value_first = value_first.map(|(v, _)| v);
        let parent = lower_nested_assign_parent(ctx, array, span);
        let parent = root_operand(ctx, parent);
        let key = lower_expr(ctx, index);
        let key = root_operand(ctx, key);
        let value = match value_first {
            Some(value) => value,
            None => {
                let value = lower_expr(ctx, value);
                root_operand(ctx, value)
            }
        };
        ctx.emit_void(
            Op::RuntimeCall,
            vec![parent.value, key.value, value.value],
            None,
            effects_lookup::runtime_effects(),
            Some(span),
        );
        // The boxed writer borrows all three operands and retains or copies the stored value.
        // Roots also retire computed Mixed values and keep earlier operands safe if a later
        // operand or the write throws into a same-frame catch.
        for root in roots.into_iter().rev() {
            crate::ir_lower::expr::retire_owned_call_operand(ctx, root, span);
        }
        // The reference `own_variable_container_value` took for the write is dropped now that
        // the set helper holds its own.
        if acquired_value == Some(value.value) {
            crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
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

/// Acquires a variable's array or boxed value so a nested write cannot alias it in place.
///
/// Returns the value to store and whether this function took the reference (the caller then
/// releases it after the set, which keeps its own reference).
fn own_variable_container_value(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> (LoweredValue, bool) {
    let is_container = matches!(
        value.ir_type,
        IrType::Heap(
            crate::ir::IrHeapKind::Array
                | crate::ir::IrHeapKind::Hash
                | crate::ir::IrHeapKind::Mixed
                | crate::ir::IrHeapKind::Union
        )
    );
    if !is_container {
        return (value, false);
    }
    let owned = ctx.emit_value(
        Op::Acquire,
        vec![value.value],
        None,
        ctx.builder.value_php_type(value.value),
        Op::Acquire.default_effects(),
        Some(span),
    );
    (owned, true)
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
    // the plain subscript read the pre-#555 lowering produced. When that read
    // yields a CONTAINER (an array, hash or boxed value) it is a detached copy:
    // the leaf write would mutate the copy and vanish. Refuse it instead
    // of compiling a write that silently does nothing; an object element
    // (ArrayAccess) is a handle and keeps working through the read.
    // Only a chain rooted in named storage can lose a write this way: writing into an element
    // of a call result (`debug_backtrace()[1]['args'][0] = ...`) mutates a temporary in PHP too.
    let receiver_is_by_value_container = matches!(
        receiver.ir_type,
        IrType::Heap(crate::ir::IrHeapKind::Array | crate::ir::IrHeapKind::Hash)
    ) && nested_chain_root_is_named_storage(expr);
    let parent = lower_array_access_from_lowered_receiver(ctx, receiver, index, expr);
    if receiver_is_by_value_container
        && matches!(
            parent.ir_type,
            IrType::Heap(
                crate::ir::IrHeapKind::Array
                    | crate::ir::IrHeapKind::Hash
                    | crate::ir::IrHeapKind::Mixed
                    | crate::ir::IrHeapKind::Union
            )
        )
    {
        crate::ir_lower::diagnostics::refuse(
            expr.span,
            "Nested array write through this container is not supported: its elements are \
             concretely typed (or it is reached through a by-reference, typed or temporary \
             value), so the write would land in a copy. Declare the container as `array` \
             (or leave it untyped) so its elements are boxed",
        );
    }
    parent
}

/// Returns whether a nested-write chain starts at a variable, property or static property (as
/// opposed to a call result or another temporary value).
fn nested_chain_root_is_named_storage(expr: &Expr) -> bool {
    let mut root = expr;
    while let ExprKind::ArrayAccess { array, .. } = &root.kind {
        root = array;
    }
    matches!(
        root.kind,
        ExprKind::Variable(_)
            | ExprKind::This
            | ExprKind::PropertyAccess { .. }
            | ExprKind::StaticPropertyAccess { .. }
    )
}

/// Returns whether `lower_nested_assign_parent(expr)` yields the STORED cell of the container,
/// so a mutation through it reaches the variable or property the chain names.
///
/// Mirrors the decisions `lower_nested_assign_parent` takes before lowering anything: a local
/// concrete container with Mixed elements, a boxed local, a boxed fixed property slot behind a
/// stable receiver, or a boxed static property; deeper levels inherit the answer of their
/// receiver because every boxed level is fetched for write. Any other shape lowers to a plain
/// read whose result is a detached copy.
pub(crate) fn nested_parent_is_write_through(ctx: &LoweringContext<'_, '_>, expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(name) => ctx.local_type(name).codegen_repr() == PhpType::Mixed,
        ExprKind::PropertyAccess { object, property } => {
            crate::ir_lower::expr::nested_write_property_source_is_addressable(ctx, object, property)
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            super::static_property_type(ctx, receiver, property)
                .is_some_and(|ty| ty.codegen_repr() == PhpType::Mixed)
        }
        ExprKind::ArrayAccess { array, .. } => {
            if let ExprKind::Variable(name) = &array.kind {
                let local_ty = ctx.local_type(name).codegen_repr();
                let concrete = match &local_ty {
                    PhpType::Array(elem) => {
                        elem.codegen_repr() == PhpType::Mixed
                            || is_empty_indexed_array_element(elem.as_ref())
                    }
                    PhpType::AssocArray { value, .. } => value.codegen_repr() == PhpType::Mixed,
                    _ => false,
                };
                return concrete
                    || local_ty == PhpType::Mixed
                    || concrete_container_elements_boxing(ctx, name).is_some();
            }
            nested_parent_is_write_through(ctx, array)
        }
        _ => false,
    }
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
    box_concrete_container_elements(ctx, name, span);
    let local_ty = ctx.local_type(name);
    let indexed_mixed = matches!(
        local_ty.codegen_repr(),
        PhpType::Array(ref elem_ty)
            if elem_ty.codegen_repr() == PhpType::Mixed
                || is_empty_indexed_array_element(elem_ty.as_ref())
    );
    let assoc_mixed = matches!(
        local_ty.codegen_repr(),
        PhpType::AssocArray { ref value, .. } if value.codegen_repr() == PhpType::Mixed
    );
    if !indexed_mixed && !assoc_mixed {
        return None;
    }
    // The key's storage class decides the container shape, so it is classified from the LOWERED
    // value, not from syntax: a plain variable, property or call key is syntactically `Int`
    // (`infer_expr_type_syntactic`), and coercing a string-valued one to int collapsed
    // `$c[$e]['k'] = 'x'` onto key 0. The key is lowered before the container is loaded,
    // so a key expression that reallocates the container cannot leave a stale pointer behind.
    let syntactic_key_ty = index_expr_key_type(ctx, index);
    let key = lower_expr(ctx, index);
    let key_class = nested_parent_key_class(ctx, index, key, syntactic_key_ty);
    if assoc_mixed {
        let hash_value = ctx.load_local(name, Some(span));
        let assoc_ty = ctx.local_type(name);
        ctx.prepare_mutated_local_owner(name, hash_value, assoc_ty.clone(), Some(span));
        return Some(lower_hash_parent_fetch_for_write(ctx, name, hash_value, assoc_ty, key, span));
    }
    match key_class {
        NestedParentKey::Int => {
            let array_value = ctx.load_local(name, Some(span));
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
        NestedParentKey::Str => {
            // A string key on an indexed local is always a hash key:
            // promote the local to a Mixed-valued hash first (mirrors
            // `lower_string_key_array_promotion`), then ensure the element
            // through the hash path. The promoted hash flows straight into
            // the ensure call and is stored back exactly once at the end.
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
            Some(lower_hash_parent_fetch_for_write(ctx, name, hash, assoc_ty, key, span))
        }
        NestedParentKey::Mixed => {
            // A runtime-tagged key may be an integer or a string. Like `Op::ArraySetMixedKey`,
            // the local stays statically `Array(Mixed)`: the ensure helper promotes the payload
            // to hash storage only for a string key, and the mixed-key for-write read
            // dispatches on the runtime container kind.
            let array_value = ctx.load_local(name, Some(span));
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
            let cell = ctx.emit_value(
                Op::ArrayGetForWrite,
                vec![ensured.value, key.value],
                None,
                PhpType::Mixed,
                Op::ArrayGetForWrite.default_effects(),
                Some(span),
            );
            release_nested_parent_key(ctx, key, span);
            Some(cell)
        }
    }
}

/// Releases an owning string or boxed key temporary once the parent cell has been fetched.
///
/// The hash and mixed-key helpers copy (strings) or retain (boxed payloads) what they keep, so
/// the call/concat result that produced the key is dead after the fetch.
pub(super) fn release_nested_parent_key(ctx: &mut LoweringContext<'_, '_>, key: LoweredValue, span: Span) {
    if !ctx.value_is_owning_temporary(key) {
        return;
    }
    let ty = ctx.builder.value_php_type(key.value).codegen_repr();
    if matches!(ty, PhpType::Str | PhpType::Mixed | PhpType::Union(_)) {
        crate::ir_lower::ownership::release_if_owned(ctx, key, Some(span));
    }
}

/// Returns the boxing conversion a local needs before a nested write can autovivify through it.
///
/// A container whose elements are themselves concretely typed arrays (`[[]]`, `['root' => []]`)
/// stores raw array pointers in its slots; the for-write fetch needs boxed cells it can hand out
/// and replace. Such a local is converted to Mixed elements first (the checker widened its type
/// the same way). A by-reference local keeps its representation: its storage belongs to someone
/// else's slot type.
fn concrete_container_elements_boxing(ctx: &LoweringContext<'_, '_>, name: &str) -> Option<(Op, PhpType)> {
    if ctx.is_ref_bound_local(name) {
        return None;
    }
    let is_container = |ty: &PhpType| matches!(ty.codegen_repr(), PhpType::Array(_) | PhpType::AssocArray { .. });
    match ctx.local_type(name).codegen_repr() {
        PhpType::Array(elem) if is_container(&elem) => {
            Some((Op::ArrayToMixed, PhpType::Array(Box::new(PhpType::Mixed))))
        }
        PhpType::AssocArray { key, value } if is_container(&value) => Some((
            Op::HashToMixed,
            PhpType::AssocArray { key, value: Box::new(PhpType::Mixed) },
        )),
        _ => None,
    }
}

/// Boxes a local's concretely typed array elements in place (see
/// `concrete_container_elements_boxing`), mirroring the arm-edge widening of `if` merges.
fn box_concrete_container_elements(ctx: &mut LoweringContext<'_, '_>, name: &str, span: Span) {
    let Some((op, widened_ty)) = concrete_container_elements_boxing(ctx, name) else {
        return;
    };
    let source = ctx.load_local(name, Some(span));
    let converted = ctx.emit_value(
        op,
        vec![source.value],
        None,
        widened_ty.clone(),
        op.default_effects(),
        Some(span),
    );
    ctx.store_mutated_local(name, converted, widened_ty, Some(span));
}

/// How a nested-write parent key selects the parent container's storage.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum NestedParentKey {
    /// An integer-valued key (int, bool, float, null, or a foreach key over an indexed source).
    Int,
    /// A definitely-string key: always a hash key in PHP (numeric strings are normalized by the
    /// hash helpers).
    Str,
    /// A boxed key whose int/string tag is only known at runtime.
    Mixed,
}

/// Classifies a lowered nested-write parent key from its SSA value type.
pub(super) fn nested_parent_key_class(
    ctx: &LoweringContext<'_, '_>,
    index: &Expr,
    key: LoweredValue,
    syntactic: PhpType,
) -> NestedParentKey {
    if syntactic == PhpType::Str {
        return NestedParentKey::Str;
    }
    let lowered = ctx.builder.value_php_type(key.value);
    if key.ir_type == IrType::Str || lowered.codegen_repr() == PhpType::Str {
        return NestedParentKey::Str;
    }
    if index_is_boxed_mixed_key(key.ir_type) {
        if matches!(&index.kind, ExprKind::Variable(name) if ctx.is_foreach_int_key(name)) {
            return NestedParentKey::Int;
        }
        return NestedParentKey::Mixed;
    }
    NestedParentKey::Int
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
    release_nested_parent_key(ctx, key, span);
    cell
}

/// Returns true when no key of a nested write target can have a side effect.
fn nested_target_keys_are_pure(target: &Expr) -> bool {
    match &target.kind {
        ExprKind::Variable(_) => true,
        ExprKind::ArrayAccess { array, index } => {
            matches!(
                index.kind,
                ExprKind::Variable(_)
                    | ExprKind::IntLiteral(_)
                    | ExprKind::StringLiteral(_)
                    | ExprKind::BoolLiteral(_)
                    | ExprKind::FloatLiteral(_)
                    | ExprKind::Null
                    | ExprKind::ConstRef(_)
            ) && nested_target_keys_are_pure(array)
        }
        _ => false,
    }
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
