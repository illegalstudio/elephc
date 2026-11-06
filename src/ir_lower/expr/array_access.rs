//! Purpose:
//! Array, hash, string, and ArrayAccess read lowering.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers array, hash, string, or ArrayAccess indexing.
pub(super) fn lower_array_access(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
    index: &Expr,
    expr: &Expr,
) -> LoweredValue {
    lower_array_access_with_missing_warning(ctx, array, index, expr, true)
}

/// Lowers `$base[...][$index]` as the source of a by-reference `foreach`, separating the element
/// for writing instead of reading a copy of it.
///
/// A by-reference loop mutates its source container in place, and `iter_start` reaches that
/// through `__rt_array_ensure_unique`, which copies as soon as the source is shared. The plain
/// `array_get` read hands back the parent's container plus a reference of its own, so the
/// element sat at refcount 2 and every write went into a private copy that the loop then
/// dropped (issue #580). `Op::ArrayGetForWrite` performs the copy-on-write split itself —
/// separating the receiver first, then the element, publishing each back into the slot it came
/// from — and returns the element borrowed, so the loop mutates the very container the parent
/// holds, exactly like PHP.
///
/// Both receiver kinds take this path. An indexed receiver reaches its element slot with pointer
/// arithmetic (`Op::ArrayGetForWrite`); a hash entry has to be probed for, so it goes through
/// `Op::HashGetForWrite`, which separates the container the matching entry holds. The hash form
/// never needed the reference binding the checker rejects for `$r = &$h['k'];` — nothing here
/// aliases a hash slot into a local, the loop simply iterates the parent's own storage.
///
/// Falls back to the ordinary retaining read whenever the fetch-for-write would not be sound or
/// the codegen cannot express it: a non-container receiver, an indexed receiver with a
/// non-integer key, a `Mixed` element (its read can materialize a fresh box rather than the
/// slot's own storage), or a subscript chain not rooted in a local. The receiver is evaluated
/// exactly once on every path.
pub(crate) fn lower_by_ref_foreach_element_source(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
    index: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let array_value = lower_by_ref_foreach_source_receiver(ctx, array);
    let Some(op) = element_fetch_for_write_op(ctx, &array_value, index, expr) else {
        if receiver_needs_null_arm(ctx, array_value.value) {
            return lower_nullable_array_access(ctx, array_value, index, expr, true);
        }
        return lower_array_access_from_value(ctx, array_value, index, expr, true);
    };
    let index_value = lower_expr(ctx, index);
    // The hash lookup normalizes its own key, so only the indexed slot arithmetic needs an
    // int-coerced one.
    let index_value = if op == Op::ArrayGetForWrite {
        coerce_array_key_to_int_at_span(ctx, index_value, Some(index.span), false)
    } else {
        index_value
    };
    let read_op = if op == Op::ArrayGetForWrite {
        Op::ArrayGet
    } else {
        Op::HashGet
    };
    let result_type = array_access_result_type(ctx, array_value.value, read_op, expr);
    let result = ctx.emit_value(
        op,
        vec![array_value.value, index_value.value],
        None,
        result_type,
        op.default_effects(),
        Some(expr.span),
    );
    // The separated element belongs to the parent's slot, not to this read — pin that explicitly
    // instead of leaving it at the `MaybeOwned` default, which `mark_owned_temporaries` is free
    // to promote to `Owned` later, and which would then have the loop release the parent's
    // element on the way out.
    ctx.builder
        .set_value_ownership(result.value, Ownership::Borrowed);
    release_coerced_source_if_owned(ctx, index_value, Some(index.span));
    // An intermediate that fell back to a retaining read owns a reference to the container it
    // handed us, and holding it for the whole loop would be a leak. Dropping it here is safe
    // precisely because `element_fetch_for_write_op` demanded a local-rooted chain: the base
    // local keeps every level alive until the loop ends. An intermediate that took the
    // fetch-for-write path is already borrowed, so this is a no-op for it.
    // `stabilize_borrowed_result_and_release_receiver` is deliberately NOT used — it would
    // acquire the borrowed result first, restoring the refcount 2 that causes the copy.
    if ctx.value_is_owning_temporary(array_value) {
        crate::ir_lower::ownership::release_if_owned(ctx, array_value, Some(expr.span));
    }
    result
}

/// Lowers the receiver of a by-reference `foreach` element source, fetching it for write too
/// when it is itself an eligible element.
///
/// A chain has to be separated top-down, exactly as PHP does it: `foreach ($a[0][0] as &$v)`
/// separates `$a`, then `$a[0]` into `$a`'s slot, then `$a[0][0]` into `$a[0]`'s slot. Lowering
/// the intermediate with a plain read instead would leave it shared, so publishing the innermost
/// split into it would be visible through every alias of that intermediate. Each level hands the
/// next one a container that is already unique, which is why the inner levels find nothing left
/// to split and why nothing in the chain is owned by the reader.
fn lower_by_ref_foreach_source_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
) -> LoweredValue {
    if let ExprKind::ArrayAccess { array: receiver, index } = &array.kind {
        return lower_by_ref_foreach_element_source(ctx, receiver, index, array);
    }
    lower_expr(ctx, array)
}

/// Returns the fetch-for-write element read a by-reference `foreach` source can take, if any.
///
/// Requires a statically-known container element — an indexed array or a hash, the two kinds
/// with a copy-on-write helper to split them — and a subscript chain rooted in a plain variable.
/// That last condition is what makes dropping an intermediate receiver safe: a chain over a
/// temporary — `f()[0]` — has no owner once the read returns, so borrowing out of it would leave
/// the loop iterating freed storage.
///
/// An indexed receiver additionally needs an integer key, because its element slot is reached by
/// scaling the key into the payload. A hash receiver has no such restriction: `__rt_hash_get`
/// normalizes string and integer keys alike and reports the matching entry's address.
fn element_fetch_for_write_op(
    ctx: &LoweringContext<'_, '_>,
    array_value: &LoweredValue,
    index: &Expr,
    expr: &Expr,
) -> Option<Op> {
    if value_is_nullable(ctx, array_value.value) {
        return None;
    }
    let (op, elem_ty) = match (
        array_value.ir_type,
        ctx.builder.value_php_type(array_value.value).codegen_repr(),
    ) {
        (IrType::Heap(IrHeapKind::Array), PhpType::Array(elem_ty)) => {
            if index_expr_key_type(ctx, index) != PhpType::Int {
                return None;
            }
            (Op::ArrayGetForWrite, elem_ty)
        }
        (IrType::Heap(IrHeapKind::Hash), PhpType::AssocArray { value, .. }) => {
            (Op::HashGetForWrite, value)
        }
        _ => return None,
    };
    let elem_ty = normalize_value_php_type(*elem_ty).codegen_repr();
    if !matches!(elem_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return None;
    }
    subscript_chain_is_variable_rooted(expr).then_some(op)
}

/// Returns whether a subscript expression bottoms out in a plain variable receiver.
fn subscript_chain_is_variable_rooted(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(_) => true,
        ExprKind::ArrayAccess { array, .. } => subscript_chain_is_variable_rooted(array),
        _ => false,
    }
}

/// Lowers `$container[$key]` without the undefined-key warning, when `expr` is a subscript.
///
/// Returns `None` for any other expression. This is the read a WRITE context does: the bucket of
/// a nested append (`$a[$k][] = $v`) is fetched before it is appended to, and php vivifies a
/// missing `$k` silently. The fused lowering in `stmt::nested_append` does that itself; the
/// ordinary desugar it falls back to (a gradual container) read the bucket with the warning.
pub(crate) fn lower_array_access_quietly(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
) -> Option<LoweredValue> {
    let ExprKind::ArrayAccess { array, index } = &expr.kind else {
        return None;
    };
    Some(lower_array_access_with_missing_warning(ctx, array, index, expr, false))
}

/// Lowers array, hash, string, or ArrayAccess indexing with configurable
/// undefined-offset warning behavior for array and string reads. Suppressed
/// warnings propagate through the whole subscript chain: PHP's `isset()` and `??`
/// are silent for every level of `$a[1][2][3]`, not just the outermost read.
pub(super) fn lower_array_access_with_missing_warning(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
    index: &Expr,
    expr: &Expr,
    warn_on_missing: bool,
) -> LoweredValue {
    if receiver_fetch_is_delayed(array) && crate::optimize::expr_has_side_effects(index) {
        return lower_array_access_after_offset(ctx, array, index, expr, warn_on_missing);
    }
    let array_value = if warn_on_missing {
        lower_expr(ctx, array)
    } else {
        lower_subscript_receiver_silently(ctx, array)
    };
    lower_array_access_from_receiver(ctx, array_value, index, expr, warn_on_missing)
}

/// Whether php reads this subscript receiver only AFTER evaluating the offset.
///
/// php compiles `$v[...]`, `$this->p[...]`, `$o->p[...]` and `Cls::$p[...]` reads with a delayed
/// receiver fetch: the offset expression runs first, then the variable or property is read. An
/// offset with side effects therefore sees its own writes: `$this->bytes[$this->mutate()]` and
/// `$s[$f()]` with `$f` rewriting `$s` by reference read the NEW string.
fn receiver_fetch_is_delayed(array: &Expr) -> bool {
    match &array.kind {
        ExprKind::Variable(_) | ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::PropertyAccess { object, .. } => {
            matches!(object.kind, ExprKind::Variable(_) | ExprKind::This)
        }
        _ => false,
    }
}

/// Reads `$receiver[$offset]` in php's order when the offset has side effects: offset first, then
/// the receiver.
///
/// Reading the receiver first was not only the wrong value but a read of freed memory: the string
/// was loaded as a borrowed view, the offset's call replaced and released it, and the byte was
/// then taken from the released buffer.
fn lower_array_access_after_offset(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
    index: &Expr,
    expr: &Expr,
    warn_on_missing: bool,
) -> LoweredValue {
    let index_value = lower_expr(ctx, index);
    // A property read may run a hook or `__get` that throws; the evaluated offset must not leak.
    let pins = super::pin_in_flight_owners(ctx, &[index_value.value], index.span);
    let array_value = if warn_on_missing {
        lower_expr(ctx, array)
    } else {
        lower_subscript_receiver_silently(ctx, array)
    };
    super::unpin_in_flight_owners(ctx, pins, index.span);
    // A receiver that may be null takes the boxed read, which answers null and raises php's
    // `Trying to access array offset on null` itself.
    let array_value = if receiver_needs_null_arm(ctx, array_value.value) {
        ctx.box_value_as_mixed(array_value, PhpType::Mixed, Some(expr.span))
    } else {
        array_value
    };
    let array_value =
        crate::ir_lower::ownership::persist_string_byte_view(ctx, array_value, Some(expr.span));
    lower_array_access_from_values(ctx, array_value, index_value, index, expr, warn_on_missing, false)
}

/// Lowers array access over an already evaluated receiver, taking the null-safe route when
/// the receiver may be null.
pub(super) fn lower_array_access_from_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    array_value: LoweredValue,
    index: &Expr,
    expr: &Expr,
    warn_on_missing: bool,
) -> LoweredValue {
    if receiver_needs_null_arm(ctx, array_value.value) {
        return lower_nullable_array_access(ctx, array_value, index, expr, warn_on_missing);
    }
    lower_array_access_from_value(ctx, array_value, index, expr, warn_on_missing)
}

/// Fetches `$list[$key]->property` for `isset()`, `empty()` and `??`, boxed: null past a missing
/// element.
///
/// A typed array describes its hits, not the read: `array<string, Mapping>` types the element an
/// object while a miss holds null, so the element is tested before anything dereferences it.
pub(super) fn lower_quiet_element_property(
    ctx: &mut LoweringContext<'_, '_>,
    element: &Expr,
    property: &str,
    expr: &Expr,
) -> LoweredValue {
    let receiver = lower_subscript_receiver_silently(ctx, element);
    let temp_name = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![receiver.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(expr.span),
    );
    let missing = ctx.builder.create_named_block("quiet_element.missing", Vec::new());
    let present = ctx.builder.create_named_block("quiet_element.present", Vec::new());
    let merge = ctx.builder.create_named_block("quiet_element.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: missing,
        then_args: Vec::new(),
        else_target: present,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(missing);
    if ctx.value_is_owning_temporary(receiver) {
        crate::ir_lower::ownership::release_if_owned(ctx, receiver, Some(expr.span));
    }
    let null = lower_boxed_null(ctx, expr);
    store_value_into_temp(ctx, &temp_name, PhpType::Mixed, null, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(present);
    let receiver = if ctx.value_is_owning_temporary(receiver) {
        receiver
    } else {
        crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(
            ctx,
            receiver,
            Some(expr.span),
        )
    };
    let fetched = super::property_access::lower_property_fetch_mode_from_value(
        ctx,
        receiver,
        property,
        PropertyFetchMode::Coalesce,
        expr,
    );
    let fetched_type = ctx.builder.value_php_type(fetched.value);
    let boxed = if matches!(fetched_type.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        fetched
    } else {
        ctx.box_value_as_mixed(fetched, fetched_type, Some(expr.span))
    };
    store_value_into_temp(ctx, &temp_name, PhpType::Mixed, boxed, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Lowers a subscript-chain receiver with undefined-offset warnings suppressed on
/// nested array reads, so `isset()`/`??` stay silent across chained subscripts.
pub(super) fn lower_subscript_receiver_silently(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
) -> LoweredValue {
    if let ExprKind::ArrayAccess { array: inner_array, index: inner_index } = &array.kind {
        return lower_array_access_with_missing_warning(ctx, inner_array, inner_index, array, false);
    }
    // A property of an element that may not exist (`empty($mappings[$name]->list['x'])`): the
    // element is fetched quietly too, and the property fetch answers null for a null receiver.
    if let ExprKind::PropertyAccess { object, property } = &array.kind {
        if matches!(object.kind, ExprKind::ArrayAccess { .. } | ExprKind::PropertyAccess { .. })
            && super::lazy_isset::object_chain_reads_an_element(object)
        {
            return lower_quiet_element_property(ctx, object, property, array);
        }
    }
    if let Some(value) =
        super::lazy_branches::lower_magic_property_null_coalesce_probe(ctx, array)
    {
        return value;
    }
    if let Some(value) =
        super::lazy_branches::lower_initialized_property_null_coalesce_probe(ctx, array)
    {
        return value;
    }
    // A link whose receiver names no class (a `mixed` global, an untyped parameter) is resolved
    // at run time, where an inaccessible name reaches `__isset`/`__get`. Those hooks may drop the
    // receiver's last outside reference (`$root = null;` inside `__isset`), so the chain holds its
    // own for the whole fetch -- the magic-prefix arm above does the same for a known class --
    // and reads through the silent probe, as `isset()` requires of every link.
    if let ExprKind::PropertyAccess { object, property } = &array.kind {
        if super::native_isset::isset_object_expr_class(ctx, object).is_none() {
            let receiver = lower_subscript_receiver_silently(ctx, object);
            if matches!(
                ctx.builder.value_php_type(receiver.value).codegen_repr(),
                PhpType::Mixed | PhpType::Union(_)
            ) {
                let receiver = if ctx.value_is_owning_temporary(receiver) {
                    receiver
                } else {
                    crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(
                        ctx,
                        receiver,
                        Some(array.span),
                    )
                };
                // An intermediate link needs the VALUE (the next link reads it): php fetches it
                // like `??` does, `__isset` and then `__get`, not `isset()`'s `__isset` alone.
                let probed = super::property_access::lower_property_fetch_mode_from_value(
                    ctx, receiver, property, PropertyFetchMode::Coalesce, array,
                );
                // The probe answers null for a name `__isset` denies, whatever `__get` declares,
                // so the next link must see a nullable value and probe it rather than read it.
                if matches!(
                    ctx.builder.value_php_type(probed.value).codegen_repr(),
                    PhpType::Mixed | PhpType::Union(_)
                ) {
                    return probed;
                }
                return ctx.box_value_as_mixed(probed, PhpType::Mixed, Some(array.span));
            }
            return super::property_access::lower_property_get_from_value(
                ctx,
                receiver,
                property,
                Op::PropGet,
                array,
            );
        }
    }
    lower_expr(ctx, array)
}

/// Lowers array access once the receiver is already evaluated.
pub(super) fn lower_array_access_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    array_value: LoweredValue,
    index: &Expr,
    expr: &Expr,
    warn_on_missing: bool,
) -> LoweredValue {
    lower_array_access_from_value_with_null_preservation(ctx, array_value, index, expr, warn_on_missing, false)
}

/// Keeps scalar misses boxed at merge boundaries while ordinary typed reads retain their ABI.
pub(super) fn lower_array_access_from_value_with_null_preservation(
    ctx: &mut LoweringContext<'_, '_>,
    array_value: LoweredValue,
    index: &Expr,
    expr: &Expr,
    warn_on_missing: bool,
    preserve_null: bool,
) -> LoweredValue {
    let array_value = crate::ir_lower::ownership::persist_string_byte_view(
        ctx,
        array_value,
        Some(expr.span),
    );
    let (array_value, receiver_owner) = super::string_offset_owners::prepare_receiver(
        ctx, array_value, expr.span,
    );
    let mut pending_receiver = [array_value];
    let (index_value, receiver_pins) = super::lower_with_pending_owners(
        ctx,
        &mut pending_receiver,
        expr.span,
        |ctx| lower_expr(ctx, index),
    );
    let array_value = pending_receiver[0];
    let receiver_pins = if receiver_pins.is_empty() {
        super::pin_in_flight_owners(ctx, &[array_value.value], expr.span)
    } else {
        receiver_pins
    };
    lower_array_access_from_values_with_receiver_pins(
        ctx,
        array_value,
        index_value,
        index,
        expr,
        warn_on_missing,
        preserve_null,
        receiver_pins,
        receiver_owner,
    )
}

/// Reads an already evaluated receiver and key, preserving their normal read cleanup.
pub(crate) fn lower_array_access_from_values(
    ctx: &mut LoweringContext<'_, '_>,
    array_value: LoweredValue,
    index_value: LoweredValue,
    index: &Expr,
    expr: &Expr,
    warn_on_missing: bool,
    preserve_null: bool,
) -> LoweredValue {
    // Key coercion and missing-key warnings can invoke a throwing user handler. Keep an
    // owning intermediate receiver reachable until the read finishes; the normal release
    // below still retires it after the independent result has been stabilized.
    let (array_value, receiver_owner) = super::string_offset_owners::prepare_receiver(
        ctx, array_value, expr.span,
    );
    let receiver_pins = super::pin_in_flight_owners(ctx, &[array_value.value], expr.span);
    lower_array_access_from_values_with_receiver_pins(
        ctx,
        array_value,
        index_value,
        index,
        expr,
        warn_on_missing,
        preserve_null,
        receiver_pins,
        receiver_owner,
    )
}

/// Reads an evaluated receiver and key while preserving an owner published before key evaluation.
fn lower_array_access_from_values_with_receiver_pins(
    ctx: &mut LoweringContext<'_, '_>,
    array_value: LoweredValue,
    mut index_value: LoweredValue,
    index: &Expr,
    expr: &Expr,
    warn_on_missing: bool,
    preserve_null: bool,
    receiver_pins: Vec<super::PinnedInFlightOwner>,
    receiver_owner: Option<crate::ir::LocalSlotId>,
) -> LoweredValue {
    // A named-argument variadic is typed `iterable` and holds a RAW list or hash pointer, while
    // the generic read below expects a boxed Mixed receiver: it handed the bare hash to
    // `__rt_mixed_array_get`. Box it (the tag comes from the heap kind); the box is an owning
    // temporary that the receiver release at the end retires.
    //
    // An object typed by a contract or a non-final base that does not itself declare
    // `ArrayAccess` (`Illuminate\Contracts\Container\Container`) is read the same way: the class
    // behind it is what implements the interface, so the read dispatches on the boxed object at
    // run time, exactly as it does for a `mixed` receiver.
    let receiver_is_unproven_array_access = matches!(
        ctx.builder.value_php_type(array_value.value).codegen_repr(),
        PhpType::Object(ref class_name)
            if !object_name_satisfies_interface_for_ir(ctx, class_name, "ArrayAccess")
    );
    let array_value = if receiver_is_unproven_array_access
        || matches!(
            ctx.builder.value_php_type(array_value.value).codegen_repr(),
            PhpType::Iterable
        ) {
        ctx.box_value_as_mixed(array_value, PhpType::Mixed, Some(expr.span))
    } else {
        array_value
    };
    let op = match array_value.ir_type {
        IrType::Heap(IrHeapKind::Array) => {
            // Nullable scalar keys carry an inline tag. Preserve null as the empty-string
            // key instead of coercing its payload to integer zero before the runtime lookup.
            if index_value.ir_type == IrType::TaggedScalar {
                index_value = ctx.box_value_as_mixed(index_value, PhpType::Mixed, Some(index.span));
            }
            let index_ty = lowered_index_expr_key_type(ctx, index, index_value.value);
            // A genuinely boxed Mixed key is materialized by array codegen. Do not coerce it
            // here: string keys would become integer zero, while checked integer loop counters
            // use I64 and therefore still take the ordinary coercion path below.
            let index_is_mixed = matches!(index_value.ir_type, IrType::Heap(IrHeapKind::Mixed));
            if index_is_mixed {
                if warn_on_missing {
                    Op::ArrayGetMixedKey
                } else {
                    Op::ArrayGetMixedKeySilent
                }
            } else if index_ty == PhpType::Int {
                index_value = coerce_array_key_to_int_at_span(ctx, index_value, Some(index.span), false);
                if warn_on_missing {
                    Op::ArrayGet
                } else {
                    Op::ArrayGetSilent
                }
            } else {
                // String or Mixed key on indexed storage: use the mixed-key
                // runtime read path (mirrors Op::ArraySetMixedKey for writes).
                if warn_on_missing {
                    Op::ArrayGetMixedKey
                } else {
                    Op::ArrayGetMixedKeySilent
                }
            }
        }
        IrType::Heap(IrHeapKind::Hash) => {
            if warn_on_missing {
                Op::HashGet
            } else {
                Op::HashGetSilent
            }
        }
        IrType::Heap(IrHeapKind::Buffer) => {
            index_value = coerce_to_int_at_span(ctx, index_value, Some(index.span));
            Op::BufferGet
        }
        IrType::Str => {
            // PHP fetches a boxed variable again after its cast warning handler returns.
            let reload_boxed_variable = matches!(index.kind, ExprKind::Variable(_))
                && matches!(index_value.ir_type, IrType::Heap(IrHeapKind::Mixed | IrHeapKind::Union));
            index_value = if index_value.ir_type == IrType::F64 {
                ctx.emit_value(
                    Op::FToI,
                    vec![index_value.value],
                    Some(Immediate::Bool(true)),
                    PhpType::Int,
                    Op::FToI.default_effects() | Effects::MAY_WARN,
                    Some(index.span),
                )
            } else {
                coerce_string_offset_to_int(ctx, index_value, index.span)
            };
            if reload_boxed_variable {
                let current_index = lower_expr(ctx, index);
                index_value = coerce_to_int_at_span(ctx, current_index, Some(index.span));
            }
            Op::StrCharAt
        }
        _ => Op::RuntimeCall,
    };
    let mut result_type = array_access_result_type(ctx, array_value.value, op, expr);
    if preserve_null
        && matches!(op, Op::ArrayGet | Op::ArrayGetSilent | Op::HashGet | Op::HashGetSilent)
        && matches!(result_type.codegen_repr(),
            PhpType::Int | PhpType::Bool | PhpType::False | PhpType::Float | PhpType::Str | PhpType::Callable)
    {
        // The boxed getter distinguishes a miss before scalar conversion destroys its marker.
        // Object/container getters already preserve null and retain their typed fast paths.
        result_type = PhpType::Mixed;
    }
    let index_owner = if op == Op::RuntimeCall
        && ctx.builder.value_php_type(index_value.value).codegen_repr() == PhpType::Callable
    {
        let (rooted, owner) = root_owned_call_operand(ctx, index_value, index.span);
        index_value = rooted;
        owner
    } else { None };
    let mut operands = vec![array_value.value, index_value.value];
    if matches!(op, Op::RuntimeCall) {
        let warning_flag = emit_bool_literal(ctx, warn_on_missing, Some(expr.span));
        operands.push(warning_flag.value);
    }
    let result = ctx.emit_value(
        op,
        operands,
        (op == Op::StrCharAt).then_some(Immediate::Bool(warn_on_missing)),
        result_type,
        op.default_effects(),
        Some(expr.span),
    );
    if op == Op::StrCharAt {
        // The backend returns a view into the receiver. The common stabilization below
        // must copy that byte before an owning receiver can be retired (issue #1908).
        ctx.builder.set_value_ownership(result.value, Ownership::Borrowed);
    }
    super::unpin_in_flight_owners(ctx, receiver_pins, expr.span);
    // An owning boxed index temporary (e.g. `$B[$i + 1]` on the mixed-key read
    // path) is consumed by the read without any runtime refcount operation on
    // the key, and the result is freshly allocated storage that never aliases
    // it — release it here or it leaks per read (issue #500). Int-coerced
    // index paths rebound `index_value` to a non-owning raw cast, so the
    // owning-temporary gate makes this a no-op for them.
    if let Some(owner) = index_owner {
        guard_descriptor_container(ctx, result, expr.span);
        retire_owned_call_operand(ctx, owner, expr.span);
        ctx.unguard_call_argument(result.value, expr.span);
    } else if ctx.value_needs_release_after_use(index_value) {
        // A string-local key whose slot is boxed Mixed (`$k` assigned only under an `if`) is read
        // as a DETACHED copy; the provisional release is pruned when the slot lends a concrete
        // string, and otherwise frees that copy, which leaked one block per read.
        crate::ir_lower::ownership::release_if_owned(ctx, index_value, Some(index.span));
    } else {
        release_coerced_source_if_owned(ctx, index_value, Some(index.span));
        // A string key is not a coerced Mixed/array source, so the call above
        // leaves a normalized `(string)` copy alive after `hash_get_silent`.
        crate::ir_lower::ownership::release_marked_normalized_cast_copies(
            ctx, index_value, Some(index.span),
        );
    }
    // Array access consumes an owning receiver produced by an earlier read,
    // call, or one-shot temp. Preserve borrowed string/callable payloads before
    // dropping that receiver; boxed and retained container reads are already
    // independent and must not be acquired twice.
    if let Some(owner) = receiver_owner {
        return super::string_offset_owners::finish_read(ctx, result, owner, expr.span);
    }
    stabilize_borrowed_result_and_release_receiver(ctx, array_value, result, expr.span)
}

/// Converts a non-float string offset while preserving warnings for boxed float values.
fn coerce_string_offset_to_int(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: crate::span::Span,
) -> LoweredValue {
    if matches!(value.ir_type, IrType::Heap(IrHeapKind::Mixed | IrHeapKind::Union)) {
        let result = ctx.emit_value(
            Op::Cast,
            vec![value.value],
            Some(Immediate::StringOffsetCast),
            PhpType::Int,
            Op::Cast.default_effects(),
            Some(span),
        );
        release_coerced_source_if_owned(ctx, value, Some(span));
        result
    } else {
        coerce_to_int_at_span(ctx, value, Some(span))
    }
}

/// Whether a subscript read needs the separate null-receiver arm.
///
/// Only a receiver that is statically `null` (or may be, next to concrete types) does: the read
/// path cannot index it. A boxed `mixed` receiver goes through the runtime read, which handles a
/// null payload itself AND raises php's `Trying to access array offset on null` warning; the arm
/// answers a quiet null, so `function f(mixed $x) { return $x[0]; } f(null)` printed no warning.
fn receiver_needs_null_arm(ctx: &LoweringContext<'_, '_>, value: crate::ir::ValueId) -> bool {
    match ctx.builder.value_php_type(value) {
        PhpType::Void | PhpType::Never => true,
        PhpType::Union(members) => {
            members.iter().any(|member| matches!(member, PhpType::Void))
                && !members.iter().any(|member| matches!(member, PhpType::Mixed))
        }
        _ => false,
    }
}

/// Evaluates, for its effects alone, an index the null-receiver edge does not read through.
///
/// PHP evaluates every dim operand of a fetch chain before it looks at the container, so
/// `isset($seen[$group][$class = $def->getClass()])` assigns `$class` even when `$seen[$group]`
/// is missing -- and Symfony's `PhpDumper::generateProxyClasses` reads `$class` right after that
/// probe. Skipping the index on the null edge left `$class` holding the previous iteration's
/// value (or nothing). A side-effect-free index is still skipped: evaluating it cannot be
/// observed, and lowering it twice would only cost code.
pub(super) fn lower_skipped_index_for_effects(ctx: &mut LoweringContext<'_, '_>, index: &Expr) {
    if !crate::optimize::expr_has_side_effects(index) {
        return;
    }
    // Discarded like an expression statement: `$class = …` yields the local's own storage, which
    // must survive; only a value that owns a temporary is released.
    let value = lower_expr(ctx, index);
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(index.span));
    }
}

/// Lowers nullable indexing while preserving offset evaluation and ordinary warning behavior.
pub(super) fn lower_nullable_array_access(
    ctx: &mut LoweringContext<'_, '_>,
    array_value: LoweredValue,
    index: &Expr,
    expr: &Expr,
    warn_on_missing: bool,
) -> LoweredValue {
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![array_value.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(expr.span),
    );
    let result_type = PhpType::Mixed;
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
    let null_block = ctx
        .builder
        .create_named_block("nullable.index.null", Vec::new());
    let read_block = ctx
        .builder
        .create_named_block("nullable.index.read", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("nullable.index.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: null_block,
        then_args: Vec::new(),
        else_target: read_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(null_block);
    // Undefined roots and native nullable containers may carry a raw null word, not a Mixed
    // cell. The getter's dynamic ABI requires a real boxed null while evaluating the offset
    // and delivering its ordinary/silent warning. A proven-null source has no destructor,
    // so retire its owner first and protect the canonical receiver across throwing user code.
    release_owning_receiver_temporary(ctx, array_value, expr.span);
    let null_receiver = lower_boxed_null(ctx, expr);
    // This pin covers only offset evaluation: the shared read publishes its own receiver
    // record, and two live records over one reference would release it twice on a throw.
    let pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &[null_receiver.value], expr.span);
    let index_value = lower_expr(ctx, index);
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, pins, expr.span);
    let null_value = lower_array_access_from_values(
        ctx, null_receiver, index_value, index, expr, warn_on_missing, false,
    );
    store_value_into_temp(ctx, &temp_name, result_type.clone(), null_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(read_block);
    let read_value = lower_array_access_from_value(ctx, array_value, index, expr, warn_on_missing);
    store_value_into_temp(ctx, &temp_name, result_type, read_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Lowers a subscript read whose receiver has already been evaluated,
/// including the nullable-receiver guard. Used by the nested-assign parent
/// lowering when a receiver produced by a for-write chain turns out not to be
/// a boxed Mixed cell (e.g. ArrayAccess object intermediates, issue #555).
pub(crate) fn lower_array_access_from_lowered_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: LoweredValue,
    index: &Expr,
    expr: &Expr,
) -> LoweredValue {
    if receiver_needs_null_arm(ctx, receiver.value) {
        return lower_nullable_array_access(ctx, receiver, index, expr, true);
    }
    lower_array_access_from_value(ctx, receiver, index, expr, true)
}

/// Returns the statically-known key type for an array index expression.
/// Used to decide between Op::ArrayGet (int key) and Op::ArrayGetMixedKey.
pub(crate) fn index_expr_key_type(ctx: &LoweringContext<'_, '_>, index: &Expr) -> PhpType {
    let ty = match &index.kind {
        ExprKind::Variable(name) => ctx.local_type(name),
        ExprKind::PropertyAccess { object, property } => {
            property_access_expr_type_for_ir(ctx, object, property)
                .unwrap_or_else(|| infer_expr_type_syntactic(index))
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            static_property_result_type(ctx, receiver, property, index)
        }
        _ => infer_expr_type_syntactic(index),
    };
    normalized_array_key_type(index, ty)
}

/// Refines a read key's syntactic type from its lowered SSA value when it is definite.
pub(crate) fn lowered_index_expr_key_type(
    ctx: &LoweringContext<'_, '_>,
    index: &Expr,
    index_value: ValueId,
) -> PhpType {
    let syntactic = index_expr_key_type(ctx, index);
    match ctx.builder.value_php_type(index_value).codegen_repr() {
        // Dynamic strings still undergo PHP's numeric-key normalization inside the mixed-key/hash
        // runtime helpers. Keep their definite lowered type here so write-context lowering can
        // select that runtime path instead of abandoning a property store-back. Literal numeric
        // strings remain statically classifiable as integer keys.
        PhpType::Str if matches!(index.kind, ExprKind::StringLiteral(_)) => {
            normalized_array_key_type(index, PhpType::Str)
        }
        PhpType::Str => PhpType::Str,
        PhpType::Int => PhpType::Int,
        gradual @ (PhpType::Mixed | PhpType::Union(_)) => gradual,
        _ => syntactic,
    }
}

/// Refines an `isset` key from its lowered value, including boxed Mixed keys.
pub(super) fn isset_index_expr_key_type(
    ctx: &LoweringContext<'_, '_>,
    index: &Expr,
    index_value: ValueId,
) -> PhpType {
    let syntactic = index_expr_key_type(ctx, index);
    let lowered = ctx.builder.value_php_type(index_value);
    // The lowered key is the authority when it is a plain integer: a call the syntactic guess
    // cannot type (`isset($items[mark()])`, an inlined user function) still produced an `int`,
    // and routing it to the mixed-key probe asked the backend for an indexed array read by a
    // mixed key, which it refuses.
    if lowered == PhpType::Int {
        return PhpType::Int;
    }
    if syntactic != PhpType::Int {
        return syntactic;
    }
    if matches!(lowered.codegen_repr(), PhpType::TaggedScalar) {
        return syntactic;
    }
    match lowered {
        ty @ (PhpType::Str | PhpType::Mixed | PhpType::Union(_)) => {
            normalized_array_key_type(index, ty)
        }
        _ => syntactic,
    }
}

/// Returns the best PHP result type for a lowered array/string/hash access.
pub(super) fn array_access_result_type(
    ctx: &LoweringContext<'_, '_>,
    array: crate::ir::ValueId,
    op: Op,
    expr: &Expr,
) -> PhpType {
    match op {
        Op::StrCharAt => PhpType::Str,
        Op::ArrayGet | Op::ArrayGetSilent => match ctx.builder.value_php_type(array).codegen_repr() {
            PhpType::Array(elem_ty) => array_access_element_result_type(*elem_ty),
            _ => fallback_expr_type(expr),
        },
        Op::HashGet | Op::HashGetSilent => match ctx.builder.value_php_type(array).codegen_repr() {
            PhpType::AssocArray { value, .. } => array_access_element_result_type(*value),
            _ => fallback_expr_type(expr),
        },
        Op::BufferGet => match ctx.builder.value_php_type(array).codegen_repr() {
            PhpType::Buffer(elem_ty) => normalize_value_php_type(elem_ty.storage_value_type()),
            _ => fallback_expr_type(expr),
        },
        Op::RuntimeCall => array_access_runtime_call_result_type(ctx, array, expr),
        Op::ArrayGetMixedKey | Op::ArrayGetMixedKeySilent => PhpType::Mixed,
        _ => match ctx.builder.value_php_type(array).codegen_repr() {
            PhpType::Mixed | PhpType::Union(_) => PhpType::Mixed,
            _ => fallback_expr_type(expr),
        },
    }
}

/// Returns the materialized result type for a PHP array read, including miss-capable int reads.
///
/// A pointer element reads as `?ptr<T>`: the slot holds the bare word, a stored `null` is 0,
/// and a missing key reads as 0 too, so the read must keep PHP null in its type.
pub(crate) fn array_access_element_result_type(element_ty: PhpType) -> PhpType {
    if matches!(element_ty, PhpType::Never) {
        PhpType::Mixed
    } else if crate::codegen::sentinels::null_repr_is_tagged()
        && matches!(element_ty, PhpType::Int)
    {
        PhpType::TaggedScalar
    } else {
        crate::types::pointer_element_read_type(element_ty)
    }
}

/// Returns the EIR result type for object indexing routed through `ArrayAccess::offsetGet`.
pub(super) fn array_access_runtime_call_result_type(
    ctx: &LoweringContext<'_, '_>,
    array: crate::ir::ValueId,
    expr: &Expr,
) -> PhpType {
    match ctx.builder.value_php_type(array).codegen_repr() {
        PhpType::Object(class_name) => array_access_offset_get_return_type(ctx, &class_name)
            .unwrap_or_else(|| fallback_expr_type(expr)),
        // A named-argument variadic (`iterable`): the runtime read answers a boxed value; the
        // syntactic fallback guessed `int` from the literal index.
        PhpType::Mixed | PhpType::Callable | PhpType::Iterable => PhpType::Mixed,
        _ => fallback_expr_type(expr),
    }
}

/// Looks up the effective `offsetGet` return type for an ArrayAccess class.
pub(super) fn array_access_offset_get_return_type(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
) -> Option<PhpType> {
    if !object_name_satisfies_interface_for_ir(ctx, class_name, "ArrayAccess") {
        return None;
    }
    let method_key = php_symbol_key("offsetGet");
    class_method_return_type_for_ir(ctx, class_name, &method_key)
        .or_else(|| interface_method_return_type_for_ir(ctx, "ArrayAccess", &method_key))
        .map(normalize_value_php_type)
}

/// Returns true when a syntactic array receiver is statically known as `ArrayAccess`.
pub(super) fn array_access_expr_satisfies_array_access(
    ctx: &LoweringContext<'_, '_>,
    array: &Expr,
) -> bool {
    let ty = match &array.kind {
        ExprKind::Variable(name) => ctx
            .local_types
            .get(name)
            .cloned()
            .unwrap_or_else(|| infer_expr_type_syntactic(array)),
        ExprKind::PropertyAccess { object, property } => {
            instance_callable_object_class(ctx, object)
                .and_then(|class_name| {
                    ctx.classes
                        .get(class_name.trim_start_matches('\\'))?
                        .visible_property(property)
                        .map(|(_, (_, ty))| ty.clone())
                })
                .or_else(|| property_access_expr_type_for_ir(ctx, object, property))
                .unwrap_or_else(|| infer_expr_type_syntactic(array))
        }
        _ => infer_expr_type_syntactic(array),
    };
    type_satisfies_array_access_for_ir(ctx, &ty)
}
