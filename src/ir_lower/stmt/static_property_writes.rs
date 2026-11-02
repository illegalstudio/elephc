//! Purpose:
//! Static property writes and static array mutations.
//!
//! Called from:
//! - `crate::ir_lower::stmt`.
//!
//! Key details:
//! - Preserves statement ordering, CFG shape, EIR effects, and ownership contracts.

use super::*;

/// Lowers a static property write.
///
/// A static property outlives the enclosing scope, so it must hold its own
/// reference to a refcounted value. There are two storage disciplines, matched
/// to what the codegen store actually does:
///
/// - **Independent-value store** (the store crosses the boxed/unboxed boundary, in
///   either direction): a Mixed/Union slot receiving a non-Mixed value, e.g.
///   `Class::$h = new C()`, is boxed with `__rt_mixed_from_value`, which takes its
///   *own* retained reference to the child; and a narrowing slot receiving a Mixed
///   value, e.g. `Class::$s += 1` on an `int` slot, is cast out of the box, so the
///   slot holds a payload word that owns nothing of it. Either way the slot keeps a
///   reference independent of the source, so an owning temporary must be *released*
///   after the store (its reference is not the one the slot holds), and a borrowed
///   source must be left untouched. Acquiring here would leak the extra reference on
///   top of the slot's, and skipping the release leaks the source's own -- which is
///   what made every compound assignment to a typed static property leak one Mixed
///   cell (issue #1041).
/// - **Moving store** (every other case: a same-representation store, or a Mixed
///   value into a slot codegen leaves untouched): the store consumes (moves) its
///   value operand. An owning temporary is moved in as-is, but a *borrowed* value (a
///   parameter, local, or container read) must be `Acquire`d first. Without this,
///   storing a borrowed `Mixed` (e.g. `Class::$h = $handler` where `$handler` is a
///   `?SessionHandlerInterface` parameter) leaves the property dangling once the
///   borrow's owner releases its reference, so a later read dispatches on freed
///   memory (a fatal "on null").
pub(super) fn lower_static_property_assign(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    value: &Expr,
    span: Span,
) {
    let source = match static_property_type(ctx, receiver, property) {
        Some(slot_ty) => crate::ir_lower::expr::lower_expr_for_destination(ctx, value, &slot_ty),
        None => lower_expr(ctx, value),
    };
    if has_deferred_default_error(ctx, receiver) {
        raise_deferred_default_error(ctx, receiver, &[source], span);
        return;
    }
    let source = static_property_type(ctx, receiver, property)
        .map(|slot_ty| {
            let source = ctx.retype_packed_address_for_pointer_sink(source, &slot_ty, Some(span));
            coerce_typed_assign_value(ctx, source, &slot_ty, span)
        })
        .unwrap_or(source);
    if static_property_store_retains_independent_value(ctx, receiver, property, source) {
        store_static_property(ctx, receiver, property, source.value, span);
        if ctx.value_is_owning_temporary(source) {
            crate::ir_lower::ownership::release_if_owned(ctx, source, Some(span));
        }
        return;
    }
    // A `Str` is PERSISTED into the slot whatever its ownership says, and the source is then
    // released. Ownership is the wrong question for a string here: "owning" distinguishes who
    // must release, not WHERE the bytes live, and a great many string producers hand back a
    // pointer into the shared 64 KiB `_concat_buf` scratch. Moving such a pointer into a slot
    // that outlives the statement stores the right LENGTH over bytes the next producer
    // overwrites — `B::$s = strtoupper($x)` read back a later `str_repeat`'s bytes.
    //
    // Fixing this at the producer instead (reclassifying runtime-call `Str` results as
    // non-owning) was tried and is wrong twice over: it only reaches the producers someone
    // thought to list — `$x . "c"`, `"v=$x!"`, `(string)$i`, `strval($i)` all still clobbered —
    // and above 64 KiB `__rt_concat_reserve` returns an OWNED heap block, so suppressing the
    // release leaked one block per call. The consumer is the only place that knows the value is
    // about to outlive the frame, so it is the only place the question can be answered once.
    //
    // The three storage classes all land correctly, which is why this is safe rather than
    // merely conservative:
    // - scratch slice: `Acquire` is `__rt_str_persist`, which duplicates it onto the heap; the
    //   paired `Release` reaches `__rt_heap_free_safe`, which skips a non-heap pointer.
    // - owned heap block (a unary-string result over 64 KiB): duplicated, then the original is
    //   freed — this is the leak the producer-side attempt introduced.
    // - `CONCAT_TEMP_HEAP_KIND` block (a `.` result over 64 KiB): `__rt_str_persist` takes it
    //   over IN PLACE, and the release cannot double-free it because codegen classifies
    //   `Op::StrConcat` as a scratch string and drops its `Release` entirely.
    //
    // `store_local` reached the same answer for `static` LOCALS at `context.rs`'s
    // `static_local_store_needs_string_retain`; a static property has the identical lifetime
    // problem and now has the identical rule.
    let stores_a_string = matches!(
        ctx.builder.value_php_type(source.value).codegen_repr(),
        PhpType::Str
    );
    let source_is_owning_temporary = ctx.value_is_owning_temporary(source);
    let provisional_load = ctx.value_is_owned_unboxed_local_load(source.value);
    let stored = if stores_a_string || !source_is_owning_temporary || provisional_load {
        crate::ir_lower::ownership::acquire_if_refcounted(ctx, source, Some(span))
    } else {
        source
    };
    // A concrete local load can still be borrowed after final frame typing.
    // Retain its published owner, then retire only an actual Mixed unbox owner.
    if provisional_load {
        crate::ir_lower::ownership::release_if_owned(ctx, source, Some(span));
    }
    store_static_property(ctx, receiver, property, stored.value, span);
    // The string rule's own release, SKIPPED when the provisional-load arm already issued
    // one: the two conditions overlap for an owned unboxed local load of a `Str`, and
    // releasing the same source twice is a double free rather than a tidy-up.
    if stores_a_string && source_is_owning_temporary && !provisional_load {
        crate::ir_lower::ownership::release_if_owned(ctx, source, Some(span));
    }
}

/// Returns true when codegen gives the static-property slot an independently retained value.
///
/// The rule is whether the store crosses the boxed/unboxed boundary. A concrete value going
/// into a Mixed/Union slot is boxed, and the box retains the child itself. A boxed value
/// going into a slot codegen narrows to -- `Str`, `Int`, `Bool`, `Float`, `Object`, or the
/// tagged-scalar pair -- is cast out of the box, and what lands in the slot owns nothing of
/// it (`__rt_str_persist` copies, the scalar casts read a payload word, the object arm
/// increfs the unboxed pointer on its own). Both directions leave the source's reference
/// unrelated to the slot's, so borrowed sources need no `Acquire` and owning temporary
/// sources must be released after the store -- without that release the box outlives its
/// last use and leaks, one cell per compound assignment (issue #1041).
///
/// Every other case is a moving store: a Mixed value into a Mixed/Union slot, or into one of
/// the container slots codegen leaves untouched, writes the pointer itself into the slot, so
/// the store consumes the reference. Unknown metadata conservatively keeps that discipline.
pub(super) fn static_property_store_retains_independent_value(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    value: LoweredValue,
) -> bool {
    let Some(slot_ty) = static_property_type(ctx, receiver, property) else {
        return false;
    };
    let value_ty = ctx.builder.value_php_type(value.value);
    let slot_ty = slot_ty.codegen_repr();
    let value_ty = value_ty.codegen_repr();
    let value_is_boxed = matches!(value_ty, PhpType::Mixed | PhpType::Union(_));
    let boxes_into_mixed = matches!(slot_ty, PhpType::Mixed | PhpType::Union(_)) && !value_is_boxed;
    // The slot types codegen actually casts a Mixed source out of. Deliberately not "any slot
    // that is not Mixed": an array or iterable slot falls through `load_static_property_store_
    // value_to_result`'s catch-all arm untouched, so the Mixed pointer itself is what gets
    // stored and the store still consumes it.
    let narrows_out_of_mixed = value_is_boxed
        && matches!(
            slot_ty,
            PhpType::Str
                | PhpType::Int
                | PhpType::Pointer(_)
                | PhpType::Bool
                | PhpType::Float
                | PhpType::Object(_)
                | PhpType::TaggedScalar
        );
    boxes_into_mixed || narrows_out_of_mixed
}

/// Lowers `Class::$prop[] = value`.
pub(super) fn lower_static_property_array_push(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    value: &Expr,
    span: Span,
) {
    if has_deferred_default_error(ctx, receiver) {
        let value = lower_expr(ctx, value);
        raise_deferred_default_error(ctx, receiver, &[value], span);
        return;
    }
    let source = Expr::new(ExprKind::StaticPropertyAccess {
        receiver: receiver.clone(), property: property.into(),
    }, span);
    if crate::ir_lower::expr::lower_weak_map_denied_append(ctx, &source, value, span) { return; }
    let value = lower_expr(ctx, value);
    if let Some(array) = separate_php_array_static_property(ctx, receiver, property, span) {
        ctx.emit_void(
            Op::MixedArrayAppend,
            vec![array.value, value.value],
            None,
            Op::MixedArrayAppend.default_effects(),
            Some(span),
        );
        if ctx.value_is_owning_temporary(value) {
            crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
        }
        return;
    }
    if let Some(property_ty) =
        static_property_type(ctx, receiver, property).filter(is_indexed_array_type)
    {
        let value = ctx.retype_value_for_container_element(&property_ty, value, Some(span));
        let property_value = load_static_property_as(ctx, receiver, property, property_ty, span);
        ctx.emit_void(
            Op::ArrayPush,
            vec![property_value.value, value.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(span),
        );
        store_static_property(ctx, receiver, property, property_value.value, span);
        return;
    }

    let property_value = load_static_property(ctx, receiver, property, span);
    if static_property_may_be_eval_dynamic(ctx, receiver) {
        ctx.emit_void(
            Op::MixedArrayAppend,
            vec![property_value.value, value.value],
            None,
            Op::MixedArrayAppend.default_effects(),
            Some(span),
        );
        store_static_property(ctx, receiver, property, property_value.value, span);
        return;
    }
    ctx.emit_void(
        Op::RuntimeCall,
        vec![property_value.value, value.value],
        None,
        effects_lookup::runtime_effects(),
        Some(span),
    );
}

/// Lowers `Class::$prop[index] = value`.
///
/// A desugared `??=` becomes a probe plus a conditional insert, and a desugared compound update
/// writes with the key its own read already converted (see `ElementUpdate`).
pub(super) fn lower_static_property_array_assign(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
) {
    let update = desugared_element_update(value, span, |read| {
        reads_static_property_array_element(read, receiver, property, index)
    });
    if has_deferred_default_error(ctx, receiver) {
        let index = lower_expr(ctx, index);
        // An eager compound settles its RHS before the failing element read.
        // Plain assignments and lazy coalescing retain their original expression semantics.
        let value = if matches!(&update, Some(ElementUpdate::Compound)) {
            let ExprKind::BinaryOp { right, .. } = &value.kind else { unreachable!() };
            lower_expr(ctx, right)
        } else {
            lower_expr(ctx, value)
        };
        raise_deferred_default_error(ctx, receiver, &[index, value], span);
        return;
    }
    if matches!(&update, Some(ElementUpdate::Compound | ElementUpdate::IncDec { .. }))
        && crate::ir_lower::expr::lower_weak_map_incdec_stmt(ctx, value, span)
    {
        return;
    }
    if let Some(ElementUpdate::NullCoalesce { read, default }) = update {
        crate::ir_lower::expr::lower_null_coalesce_update_stmt(ctx, read, default, span);
        return;
    }
    if matches!(update, Some(ElementUpdate::Compound)) {
        let ExprKind::BinaryOp { left, op, right } = &value.kind else { unreachable!() };
        let rhs = lower_expr(ctx, right);
        let ty = ctx.builder.value_php_type(rhs.value);
        let name = ctx.declare_synthetic_php_local(ty.clone());
        let rhs = crate::ir_lower::ownership::copy_assignment_value(ctx, rhs, Some(span));
        ctx.store_local(&name, rhs, ty, Some(span));
        let staged = Expr::new(ExprKind::BinaryOp {
            left: left.clone(), op: op.clone(),
            right: Box::new(Expr::new(ExprKind::Variable(name.clone()), right.span)),
        }, value.span);
        lower_static_property_array_assign_with_diagnosed_key(
            ctx, receiver, property, index, &staged, span, true,
        );
        let null = LoweredValue { value: ctx.builder.emit_const_null(), ir_type: IrType::I64 };
        ctx.unset_local(&name, null, Some(span));
        return;
    }
    lower_static_property_array_assign_with_diagnosed_key(
        ctx,
        receiver,
        property,
        index,
        value,
        span,
        update.is_some(),
    );
}

/// Identifies a class-wide initialization failure before emitting an unreachable storage operation.
fn has_deferred_default_error(ctx: &LoweringContext<'_, '_>, receiver: &StaticReceiver) -> bool {
    static_receiver_class_name(ctx, receiver).and_then(|name| ctx.classes.get(&name))
        .is_some_and(|class| class.deferred_property_default_error.is_some())
}

/// Preserves write operand effects and releases owned operands if initialization throws.
fn raise_deferred_default_error(
    ctx: &mut LoweringContext<'_, '_>, receiver: &StaticReceiver, values: &[LoweredValue], span: Span,
) {
    ctx.begin_argument_guard_scope();
    for (index, value) in values.iter().copied().enumerate() {
        if ctx.value_is_owning_temporary(value) && !ctx.value_is_owned_unboxed_local_load(value.value) {
            ctx.guard_call_argument(value, index, span);
        }
    }
    ctx.end_argument_guard_scope();
    crate::ir_lower::property_default_errors::for_receiver(ctx, receiver, span);
}

/// Returns whether `read` is the element `Class::$property[index]` a statement writes.
fn reads_static_property_array_element(
    read: &Expr,
    receiver: &StaticReceiver,
    property: &str,
    index: &Expr,
) -> bool {
    matches!(
        &read.kind,
        ExprKind::ArrayAccess { array, index: read_index }
            if read_index.as_ref() == index
                && matches!(
                    &array.kind,
                    ExprKind::StaticPropertyAccess { receiver: read_receiver, property: read_property }
                        if read_receiver == receiver && read_property == property
                )
    )
}

/// Lowers `Class::$prop[index] = value`, optionally reusing a float-key diagnosis.
///
/// `key_already_diagnosed` marks the write half of a compound update whose read of the same
/// element already reported the float key's conversion. An `ArrayAccess` receiver gets the key
/// unconverted, so its `offsetSet` call never consults the flag.
pub(crate) fn lower_static_property_array_assign_with_diagnosed_key(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
    key_already_diagnosed: bool,
) {
    let key_marker = key_already_diagnosed.then_some(Immediate::Bool(true));
    let (index, value, mut index_pins) =
        array_write_core::lower_write_key_and_value(ctx, index, value);
    if let Some(array) = separate_php_array_static_property(ctx, receiver, property, span) {
        ctx.emit_void(
            Op::RuntimeCall,
            vec![array.value, index.value, value.value],
            key_marker,
            effects_lookup::runtime_effects(),
            Some(span),
        );
        crate::ir_lower::expr::unpin_in_flight_owners(
            ctx,
            std::mem::take(&mut index_pins),
            span,
        );
        release_persisted_string_operand(ctx, index, span);
        if ctx.value_is_owning_temporary(value) {
            crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
        }
        return;
    }
    let root = Expr::new(ExprKind::StaticPropertyAccess {
        receiver: receiver.clone(), property: property.to_string(),
    }, span);
    if let Some(name) = bind_mixed_static_array_write_root(ctx, &root) {
        let array = load_array_local_for_write(ctx, &name, span);
        array_write_core::lower_concrete_array_set_values(
            ctx,
            array,
            index,
            value,
            std::mem::take(&mut index_pins),
            span,
            key_already_diagnosed,
        );
        return;
    }
    if let Some(buffer_ty) = static_property_type(ctx, receiver, property)
        .filter(|ty| matches!(ty.codegen_repr(), PhpType::Buffer(_)))
    {
        // The slot holds the buffer handle, so the element write goes through the handle it
        // holds once the key and value are evaluated, exactly as a buffer property does. The
        // slot itself is not written back: a buffer has no copy-on-write storage to publish.
        let buffer = load_static_property_as(ctx, receiver, property, buffer_ty.clone(), span);
        crate::ir_lower::expr::unpin_in_flight_owners(
            ctx,
            std::mem::take(&mut index_pins),
            span,
        );
        let index = coerce_to_int_at_span(ctx, index, Some(span));
        let value = coerce_buffer_set_value(ctx, &buffer_ty, value, Some(span));
        ctx.emit_void(
            Op::BufferSet,
            vec![buffer.value, index.value, value.value],
            None,
            Op::BufferSet.default_effects(),
            Some(span),
        );
        return;
    }
    if let Some(property_ty) =
        static_property_type(ctx, receiver, property).filter(is_indexed_array_type)
    {
        let value = ctx.retype_value_for_container_element(&property_ty, value, Some(span));
        let array_ty = property_ty.clone();
        let property_value = load_static_property_as(ctx, receiver, property, property_ty, span);
        // PHP reads a plain-variable index at STORE time, after the right-hand side, so
        // `$o->a[$i] = ($i = 1)` writes index 1. The bare-local write already used this
        // rule; sharing the helper is what keeps the two from answering differently for
        // the same source line.
        crate::ir_lower::expr::unpin_in_flight_owners(
            ctx,
            std::mem::take(&mut index_pins),
            span,
        );
        let index =
            coerce_array_key_to_int_at_span(ctx, index, Some(span), key_already_diagnosed);
        let value = coerce_indexed_array_set_value(ctx, &array_ty, value, Some(span));
        ctx.emit_void(
            Op::ArraySet,
            vec![property_value.value, index.value, value.value],
            None,
            Op::ArraySet.default_effects(),
            Some(span),
        );
        store_static_property(ctx, receiver, property, property_value.value, span);
        return;
    }

    // A concrete hash-stamped static property writes through the hash store and is published
    // back, the way the indexed branch above does: the generic boxed writer would mutate a
    // detached clone and lose the element (#1950).
    if let Some(property_ty) = static_property_type(ctx, receiver, property)
        .filter(|ty| matches!(ty.codegen_repr(), PhpType::AssocArray { .. }))
    {
        let value = ctx.retype_value_for_container_element(&property_ty, value, Some(span));
        let property_value = load_static_property_as(ctx, receiver, property, property_ty.clone(), span);
        // A value the stamped hash cannot hold must not be written with the stamp's tag: widening
        // the storage is refused by the property store, which is the loud, fail-closed outcome —
        // PHP would keep the runtime value, tracked with the untyped-property storage change
        // (#1951's shared helper unifies this predicate once it lands).
        if let Some(updated_ty) = widened_hash_storage(
            &property_ty,
            &ctx.builder.value_php_type(value.value),
        ) {
            let converted = ctx.emit_value(
                Op::HashToMixed,
                vec![property_value.value],
                None,
                updated_ty.clone(),
                Op::HashToMixed.default_effects(),
                Some(span),
            );
            store_static_property(ctx, receiver, property, converted.value, span);
            release_persisted_string_operand(ctx, index, span);
            release_persisted_string_operand(ctx, value, span);
            return;
        }
        crate::ir_lower::expr::unpin_in_flight_owners(
            ctx,
            std::mem::take(&mut index_pins),
            span,
        );
        ctx.emit_void(
            Op::HashSet,
            vec![property_value.value, index.value, value.value],
            key_marker,
            Op::HashSet.default_effects(),
            Some(span),
        );
        store_static_property(ctx, receiver, property, property_value.value, span);
        release_persisted_string_operand(ctx, index, span);
        release_persisted_string_operand(ctx, value, span);
        return;
    }
    let array_access_ty = static_property_type(ctx, receiver, property)
        .filter(|ty| type_satisfies_array_access_for_ir(ctx, ty));
    // `offsetSet` receives the original key, so only a boxed array receiver has a key to rebuild.
    let key_marker = if array_access_ty.is_some() { None } else { key_marker };
    let property_value = if let Some(property_ty) = array_access_ty {
        load_static_property_as(ctx, receiver, property, property_ty, span)
    } else {
        load_static_property(ctx, receiver, property, span)
    };
    // PHP reads a plain-variable index at STORE time, after the right-hand side, so
    // `$o->a[$i] = ($i = 1)` writes index 1. The bare-local write already used this
    // rule; sharing the helper is what keeps the two from answering differently for
    // the same source line.
    if static_property_may_be_eval_dynamic(ctx, receiver) {
        ctx.emit_void(
            Op::RuntimeCall,
            vec![property_value.value, index.value, value.value],
            key_marker,
            effects_lookup::runtime_effects(),
            Some(span),
        );
        crate::ir_lower::expr::unpin_in_flight_owners(
            ctx,
            std::mem::take(&mut index_pins),
            span,
        );
        release_persisted_string_operand(ctx, index, span);
        store_static_property(ctx, receiver, property, property_value.value, span);
        return;
    }
    ctx.emit_void(
        Op::RuntimeCall,
        vec![property_value.value, index.value, value.value],
        key_marker,
        effects_lookup::runtime_effects(),
        Some(span),
    );
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, index_pins, span);
    release_persisted_string_operand(ctx, index, span);
}

/// Binds mixed-element static storage through a hash-aware relocating write alias.
/// A previous statement or call may already have promoted an indexed declaration to a hash.
pub(super) fn bind_mixed_static_array_write_root(
    ctx: &mut LoweringContext<'_, '_>, root: &Expr,
) -> Option<String> {
    let ExprKind::StaticPropertyAccess { receiver, property } = &root.kind else { return None; };
    let ty = static_property_type(ctx, receiver, property)?;
    let indexed = match ty.codegen_repr() {
        PhpType::Array(element) if element.codegen_repr() == PhpType::Mixed => true,
        PhpType::AssocArray { value, .. } if value.codegen_repr() == PhpType::Mixed => false,
        _ => return None,
    };
    let name = ctx.declare_synthetic_php_local(ty.clone());
    crate::ir_lower::expr::lower_ref_assign_static_property(ctx, &name, root, root.span);
    if indexed {
        let source = ctx.load_local(&name, Some(root.span));
        let assoc_ty = promoted_assoc_array_type(ty, PhpType::Mixed);
        ctx.prepare_mutated_local_owner_for_backend_retire(
            &name, source, assoc_ty.clone(), Some(root.span),
        );
        // ArrayToHash checks the actual heap kind, reusing already-promoted storage.
        let hash = ctx.emit_value(Op::ArrayToHash, vec![source.value], None, assoc_ty.clone(),
            Op::ArrayToHash.default_effects(), Some(root.span));
        ctx.store_prepared_mutated_local(&name, hash, assoc_ty, Some(root.span));
    }
    Some(name)
}

/// Lowers `unset(Class::$prop[$key])` for a declared PHP `array` static property.
///
/// The key is lowered by the caller, before the property is touched, which is PHP's order. The
/// static slot's boxed cell is separated first, the same way an element write separates it, so a
/// `$copy = Class::$prop` taken earlier keeps its own storage; `OffsetUnset` then promotes the
/// separated cell to hash storage and removes the key without renumbering the survivors.
pub(crate) fn lower_static_property_array_unset(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    index: LoweredValue,
    span: Span,
) {
    // `unset_target_supported` only admits this shape for a declared PHP `array` static
    // property, and the separation succeeds for exactly that type.
    let Some(array) = separate_php_array_static_property(ctx, receiver, property, span) else {
        return;
    };
    ctx.emit_void(
        Op::OffsetUnset,
        vec![array.value, index.value],
        None,
        Op::OffsetUnset.default_effects(),
        Some(span),
    );
}

/// Publishes a detached boxed PHP array before a static-property element mutation.
/// The static slot consumes the new cell and releases its previous owner through ordinary storage.
fn separate_php_array_static_property(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    span: Span,
) -> Option<LoweredValue> {
    let ty = static_property_type(ctx, receiver, property)?;
    if !ty.is_php_array() {
        return None;
    }
    let previous = load_static_property_as(ctx, receiver, property, ty.clone(), span);
    let separated = ctx.emit_value(
        Op::MixedClone,
        vec![previous.value],
        None,
        ty,
        Op::MixedClone.default_effects(),
        Some(span),
    );
    store_static_property(ctx, receiver, property, separated.value, span);
    Some(separated)
}

/// Returns the widened storage when a stamped static hash cannot hold the assigned value.
///
/// PHP's untyped arrays keep the runtime value; a value the stamp cannot represent widens the
/// storage to `Mixed` values, which the property store then refuses loudly rather than writing
/// the value with the stamp's tag (#1950).
fn widened_hash_storage(storage: &PhpType, value: &PhpType) -> Option<PhpType> {
    let PhpType::AssocArray { key, value: stored } = storage.codegen_repr() else {
        return None;
    };
    if stored.codegen_repr() == PhpType::Mixed || stored.codegen_repr() == value.codegen_repr() {
        return None;
    }
    Some(PhpType::AssocArray {
        key,
        value: Box::new(PhpType::Mixed),
    })
}

/// Returns true when a named static-property receiver may resolve through eval metadata.
pub(super) fn static_property_may_be_eval_dynamic(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
) -> bool {
    let StaticReceiver::Named(class_name) = receiver else {
        return false;
    };
    ctx.has_eval_barrier()
        && !ctx
            .classes
            .contains_key(class_name.as_str().trim_start_matches('\\'))
}
