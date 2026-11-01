//! Purpose:
//! Direct array writes, key promotion, and write-operand ownership.
//!
//! Called from:
//! - `crate::ir_lower::stmt`.
//!
//! Key details:
//! - Preserves statement ordering, CFG shape, EIR effects, and ownership contracts.

use super::*;

/// Releases the value operand of an array/hash element write when it is an owned
/// string. These writes PERSIST (copy) a string value into the container instead
/// of moving it (`__rt_str_persist`), so an owned string operand — e.g. a function
/// or extern call result like `$_ENV[$k] = getenv_value()` — would otherwise never
/// be freed (a per-write heap leak that exhausts the heap under `--web`). Non-string
/// refcounted values (objects, arrays) are moved, or retained only when borrowed,
/// by the write itself, so they must not be released here.
pub(in crate::ir_lower) fn release_persisted_string_operand(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) {
    let ty = ctx.builder.value_php_type(value.value);
    // Only release a FRESH owning string temporary (a call/concat result, etc.).
    // A borrowed load of a variable that still owns the string (e.g. the prelude's
    // `$_GET[$k] = $v`) must NOT be released here, or the container's stored copy
    // would be freed out from under it.
    if matches!(ty.codegen_repr(), PhpType::Str) && ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
}

/// Releases a string written into a Mixed container slot when the slot's box COPIED it.
///
/// A box takes over a heap string that a call or `StrPersist` produced
/// (`Op::yields_box_absorbable_string`), so that one is no longer this write's to release --
/// releasing it freed the string the container now holds. Every other fresh string -- a
/// `(string)` cast, a concatenation -- is copied into the box and still has to be dropped.
pub(in crate::ir_lower) fn release_string_copied_into_box(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) {
    let absorbed = ctx
        .builder
        .value_defining_op(value.value)
        .is_some_and(Op::yields_box_absorbable_string);
    if !absorbed {
        release_persisted_string_operand(ctx, value, span);
    }
}

/// Releases an indexed-array write operand when the backend retained or copied it.
pub(in crate::ir_lower) fn release_indexed_array_write_operand(
    ctx: &mut LoweringContext<'_, '_>,
    container_elem_ty: Option<&PhpType>,
    value: LoweredValue,
    span: Span,
) {
    if !ctx.value_is_owning_temporary(value) {
        return;
    }
    let value_ty = ctx.builder.value_php_type(value.value).codegen_repr();
    if matches!(
        container_elem_ty.map(PhpType::codegen_repr),
        Some(PhpType::Mixed)
    ) && !matches!(value_ty, PhpType::Mixed | PhpType::Union(_))
    {
        return;
    }
    crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
}

/// Returns the indexed-array element type in effect for a write.
pub(in crate::ir_lower) fn indexed_array_write_element_type(
    ctx: &LoweringContext<'_, '_>,
    array_value: LoweredValue,
    updated_ty: Option<&PhpType>,
) -> Option<PhpType> {
    let array_ty = updated_ty
        .cloned()
        .unwrap_or_else(|| ctx.builder.value_php_type(array_value.value));
    match array_ty.codegen_repr() {
        PhpType::Array(elem_ty) => Some(elem_ty.codegen_repr()),
        _ => None,
    }
}

/// Lowers the key and the value of an element write in PHP's evaluation order.
///
/// PHP freezes an index *expression* into a temporary before the right-hand side runs, so
/// `$a[idx()] = val()` prints `[idx][val]` and stores at whatever `idx()` returned. But a
/// plain *variable* index is not frozen: the store reads the variable's slot at store time,
/// after the right-hand side, so `$i = 0; $a[$i] = ($i = 1);` writes index 1, not index 0.
/// Only the read moves — the index expression keeps its place so side-effect order is
/// unchanged, which for a bare variable is no order at all.
///
/// The value goes through `lower_array_reference_or_value`, so `$a[$k] = &$x` materializes the
/// source's ref cell instead of a plain read. That path is a strict extension: an expression that
/// is not an `ArrayReference` marker lowers exactly as `lower_expr` would.
pub(super) fn lower_write_key_and_value(
    ctx: &mut LoweringContext<'_, '_>,
    index: &Expr,
    value: &Expr,
) -> (
    LoweredValue,
    LoweredValue,
    Vec<crate::ir_lower::expr::PinnedInFlightOwner>,
) {
    lower_write_key_then(ctx, index, |ctx| lower_array_reference_or_value(ctx, value))
}

/// Lowers an element write's key and then its value, in the order of `lower_write_key_and_value`.
///
/// `lower_value` produces the stored value; a bare variable key is still read after it and a
/// computed key before it, with the key kept unwind-visible while the value runs.
pub(super) fn lower_write_key_then<'m, 'f, T>(
    ctx: &mut LoweringContext<'m, 'f>,
    index: &Expr,
    lower_value: impl FnOnce(&mut LoweringContext<'m, 'f>) -> T,
) -> (
    LoweredValue,
    T,
    Vec<crate::ir_lower::expr::PinnedInFlightOwner>,
) {
    if matches!(index.kind, ExprKind::Variable(_)) {
        let value_value = lower_value(ctx);
        return (lower_expr(ctx, index), value_value, Vec::new());
    }
    // A scratch key (`"svc$i"`) must outlive a value whose calls reset the concat buffer:
    // `$defs["svc$i"] = (new Def())->m([new Ref("a$i")])->m(["name$i"])` keyed the entry "name"
    // once the second concatenation reused the bytes, so Symfony's container lost services.
    let index_value = lower_expr(ctx, index);
    let index_value = crate::ir_lower::ownership::persist_string_byte_view(
        ctx,
        index_value,
        Some(index.span),
    );
    let mut pending = [index_value];
    let (value_value, index_pins) = crate::ir_lower::expr::lower_with_pending_owners(
        ctx, &mut pending, index.span, lower_value,
    );
    (pending[0], value_value, index_pins)
}

/// Lowers an indexed array assignment.
pub(super) fn lower_array_assign(
    ctx: &mut LoweringContext<'_, '_>,
    array: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
) {
    let update = desugared_element_update(value, span, |read| {
        reads_local_array_element(read, array, index)
    });
    if matches!(&update, Some(ElementUpdate::Compound | ElementUpdate::IncDec { .. }))
        && crate::ir_lower::expr::lower_weak_map_incdec_stmt(ctx, value, span)
    {
        return;
    }
    if let Some(ElementUpdate::NullCoalesce { read, default }) = update {
        crate::ir_lower::expr::lower_null_coalesce_update_stmt(ctx, read, default, span);
        return;
    }
    let incdec = match &update {
        Some(ElementUpdate::IncDec { increment }) => Some(*increment),
        _ => None,
    };
    lower_array_assign_with_update(ctx, array, index, value, span, update.is_some(), incdec);
}

/// How a statement value produced by the parser's update desugaring re-reads its own target.
///
/// `$place[$k] op= $v` is written as `$place[$k] op $v` and `$place[$k] ??= $v` as
/// `$place[$k] ?? $v`, both built with the statement's own span. A right-hand side the user
/// wrote never carries that span, so `$a[$k] = $a[$k] + 1` stays two separate PHP accesses.
pub(crate) enum ElementUpdate<'a> {
    /// A compound operator. The value already read the element, so the write reuses the key
    /// that read converted instead of diagnosing a float key a second time.
    Compound,
    /// A captured static element uses the shared PHP string/null incdec rules.
    IncDec { increment: bool },
    /// A null-coalescing assignment. `read` probes the element, and `default` is written only
    /// when that probe produced null, as PHP does.
    NullCoalesce { read: &'a Expr, default: &'a Expr },
}

/// Classifies a statement value that re-reads the element the statement writes.
pub(crate) fn desugared_element_update<'a>(
    value: &'a Expr,
    assignment_span: Span,
    reads_target: impl Fn(&Expr) -> bool,
) -> Option<ElementUpdate<'a>> {
    if value.span != assignment_span {
        return None;
    }
    let value = match &value.kind {
        ExprKind::Assignment { target, value, result_target: None, prelude, .. }
            if prelude.is_empty() && matches!(target.kind, ExprKind::Variable(_)) => value,
        _ => value,
    };
    if value.span != assignment_span {
        return None;
    }
    match &value.kind {
        ExprKind::BinaryOp { left, .. } if reads_target(left) => Some(ElementUpdate::Compound),
        ExprKind::NullCoalesce { value: read, default } if reads_target(read) => {
            Some(ElementUpdate::NullCoalesce { read, default })
        }
        ExprKind::Assignment { value, prelude, result_target: None, .. }
            if matches!(prelude.len(), 1 | 2) => {
            let StmtKind::Assign { name, value: read } = &prelude[0].kind else { return None; };
            if !reads_target(read) { return None; }
            let working = if let Some(copy) = prelude.get(1) {
                let StmtKind::Assign { name: working, value: copy } = &copy.kind else { return None; };
                if !matches!(&copy.kind, ExprKind::Variable(source) if source == name) { return None; }
                working
            } else { name };
            match &value.kind {
                ExprKind::PreIncrement(local) if local == working => Some(ElementUpdate::IncDec { increment: true }),
                ExprKind::PreDecrement(local) if local == working => Some(ElementUpdate::IncDec { increment: false }),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Returns whether `read` is the element `$array[index]` of the named local.
fn reads_local_array_element(read: &Expr, array: &str, index: &Expr) -> bool {
    matches!(
        &read.kind,
        ExprKind::ArrayAccess { array: receiver, index: read_index }
            if matches!(&receiver.kind, ExprKind::Variable(name) if name == array)
                && read_index.as_ref() == index
    )
}

/// Lowers an array write after an earlier read of the same source-level dimension.
pub(crate) fn lower_array_assign_with_diagnosed_key(
    ctx: &mut LoweringContext<'_, '_>,
    array: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
    key_already_diagnosed: bool,
) {
    lower_array_assign_with_update(ctx, array, index, value, span, key_already_diagnosed, None);
}

/// Lowers an array write whose value may be a statement `++`/`--` of the element it writes.
///
/// `incdec` carries the direction of such an update. A local scalar buffer takes it in
/// registers (`buffer_element_updates`); every other container lowers the value as written.
fn lower_array_assign_with_update(
    ctx: &mut LoweringContext<'_, '_>,
    array: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
    key_already_diagnosed: bool,
    incdec: Option<bool>,
) {
    // The write path is chosen from the container's type BEFORE the key and the value are
    // lowered, and an `eval()` or runtime include among them widens every local to `mixed` as it
    // runs. Widen first, so the path chosen is the one the container has when it is written:
    // `$routes[4] = $forDump ? $code : eval($code)` (Symfony's CompiledUrlMatcherDumper) picked
    // the indexed writer and then handed it the boxed container.
    // A closed-world build only widens for an eval of a literal fragment: a computed string
    // raises instead of running, so it can change no local.
    let widens = |expr: &Expr| {
        crate::ir_lower::expr::expr_contains_eval_call(expr)
            && !(crate::closed_world::is_enabled() && !contains_literal_eval(expr))
    };
    if widens(value) || widens(index) {
        ctx.apply_eval_barrier();
    }
    if string_offset_writes::local_is_string_offset_target(ctx, array) {
        string_offset_writes::lower_string_offset_assign(ctx, array, index, value, span);
        return;
    }
    if let Some(increment) = incdec {
        if matches!(ctx.local_type(array).codegen_repr(), PhpType::Buffer(_)) {
            let array_value = load_array_local_for_write(ctx, array, span);
            if let Some(update) = super::buffer_element_updates::statement_update(
                ctx, array_value, value, increment,
            ) {
                super::buffer_element_updates::lower_local_update(
                    ctx, array_value, index, &update, span,
                );
                return;
            }
        }
    }
    // The path is chosen from the representation the container would load with, without loading
    // it: the container is fetched below, after the key and the value. A real load taken here
    // was left behind once the chosen path loaded again, and a load that unboxes a Mixed-storage
    // slot holds its own reference -- `$set = []; $set['k'] = true;` leaked the array per write
    // in any function whose frame widened to boxed slots.
    // `$boxed[$k] = &$x` binds a reference, which the boxed writer cannot carry: the general
    // path below materializes it through `lower_array_reference_or_value`.
    if matches!(
        ctx.peek_local_load_ir_type(array),
        IrType::Heap(crate::ir::IrHeapKind::Mixed | crate::ir::IrHeapKind::Union)
    ) && !matches!(value.kind, ExprKind::ArrayReference(_))
    {
        lower_boxed_array_set(ctx, array, index, value, span, key_already_diagnosed);
        return;
    }
    let (mut index_value, mut value_value, mut index_pins) =
        lower_write_key_and_value(ctx, index, value);
    // The container is fetched for writing AFTER the key and the value, as PHP does: a value that
    // grows the array (`$a['x'] = grow($a)` with a by-reference `grow`) reallocates it, and a
    // pointer loaded first would write into the old block.
    if ctx.builder.insertion_block_is_terminated() {
        return;
    }
    let array_value = load_array_local_for_write(ctx, array, span);
    let op = array_set_op(array_value.ir_type);
    let container_ty = ctx.builder.value_php_type(array_value.value);
    value_value = ctx.retype_value_for_container_element(&container_ty, value_value, Some(value.span));
    // A hash whose stamped value type cannot hold the assigned value widens its storage first,
    // like the indexed path does; PHP's untyped arrays keep the runtime value (#1951).
    let (array_value, hash_storeback) = if op == Op::HashSet {
        super::array_write_storage::prepare_hash_local_write(ctx, array, array_value, value_value, span)
    } else {
        (array_value, None)
    };
    // Concrete string keys normally select associative storage. Ref-bound Mixed-slot
    // arrays instead use the shared writer, preserving invoker markers and keeping numeric
    // strings indexed. A boxed Mixed/Union index may hold either
    // an integer or a string key (foreach loop keys are always Mixed in EIR via
    // `Op::IterCurrentKey`), so it goes through `Op::ArraySetMixedKey`, whose
    // runtime helper keeps integer keys on indexed storage (preserving indexed
    // consumers like `implode`) and promotes only string keys to a hash. This
    // stops a `foreach($arr as $k=>$v) $dst[$k]=$v` rebuild from collapsing a
    // string key onto int 0. A foreach key over a concretely-indexed array is
    // known to be int-valued, so it is left on the coerce path to avoid
    // needlessly dispatching.
    let empty_sparse_key = matches!(ctx.builder.value_php_type(array_value.value).codegen_repr(),
        PhpType::Array(element) if is_empty_indexed_array_element(&element))
        && crate::types::empty_array_key_requires_hash_storage(index)
        && !index_is_boxed_mixed_key(index_value.ir_type);
    let mixed_elements = matches!(ctx.builder.value_php_type(array_value.value).codegen_repr(),
        PhpType::Array(element) if element.codegen_repr() == PhpType::Mixed);
    let reference_string_key = mixed_elements && index_value.ir_type == IrType::Str
        && ctx.is_ref_bound_local(array);
    if op == Op::ArraySet && (index_value.ir_type == IrType::Str || empty_sparse_key)
        && !reference_string_key
    {
        lower_array_key_hash_promotion(
            ctx,
            array,
            array_value,
            index_value,
            value_value,
            std::mem::take(&mut index_pins),
            span,
            key_already_diagnosed,
        );
        return;
    }
    // Contextual storage may already have widened an empty destination to Mixed slots.
    // A scalar key can still be negative or sparse, even when arithmetic lowering
    // produced an unboxed integer for a checker-facing Mixed expression. Even a
    // literal zero must preserve an existing invoker marker through the shared writer.
    if op == Op::ArraySet
        && mixed_elements
        && !index_is_boxed_mixed_key(index_value.ir_type)
        && !index_is_foreach_int_key(ctx, index)
        && (index_value.ir_type == IrType::Str
            || crate::types::empty_array_key_requires_hash_storage(index)
            || matches!(value_value.ir_type,
                IrType::Heap(crate::ir::IrHeapKind::Mixed | crate::ir::IrHeapKind::Union)))
    {
        let key = ctx.box_value_as_mixed(index_value, PhpType::Mixed, Some(index.span));
        crate::ir_lower::expr::unpin_in_flight_owners(
            ctx,
            std::mem::take(&mut index_pins),
            index.span,
        );
        lower_mixed_key_array_set(
            ctx, array, array_value, key, value_value, span, key_already_diagnosed,
        );
        crate::ir_lower::ownership::release_if_owned(ctx, key, Some(index.span));
        return;
    }
    if op == Op::ArraySet && index_value.ir_type == IrType::TaggedScalar {
        // The mixed-key helper reads a boxed key. Preserve null's tag while
        // adapting an inline nullable scalar, then retire the unconsumed box.
        let key = ctx.box_value_as_mixed(index_value, PhpType::Mixed, Some(index.span));
        crate::ir_lower::expr::unpin_in_flight_owners(
            ctx,
            std::mem::take(&mut index_pins),
            index.span,
        );
        release_persisted_string_operand(ctx, index_value, span);
        lower_mixed_key_array_set(
            ctx, array, array_value, key, value_value, span, key_already_diagnosed,
        );
        crate::ir_lower::ownership::release_if_owned(ctx, key, Some(index.span));
        return;
    }
    if op == Op::ArraySet
        && index_is_boxed_mixed_key(index_value.ir_type)
        && !index_is_foreach_int_key(ctx, index)
    {
        crate::ir_lower::expr::unpin_in_flight_owners(
            ctx,
            std::mem::take(&mut index_pins),
            index.span,
        );
        lower_mixed_key_array_set(
            ctx, array, array_value, index_value, value_value, span, key_already_diagnosed,
        );
        // The helper only borrows the key cell: a call's fresh key (`$map[key()] = 1`) is
        // released here, as the boxed scalar key above is; the backend skips a borrowed one.
        crate::ir_lower::ownership::release_if_owned(ctx, index_value, Some(index.span));
        return;
    }
    if op == Op::ArraySet
        && index_value.ir_type == IrType::I64
        && index_requires_runtime_array_key_dispatch(index)
    {
        let index_value = ctx.box_value_as_mixed(index_value, PhpType::Mixed, Some(index.span));
        crate::ir_lower::expr::unpin_in_flight_owners(
            ctx,
            std::mem::take(&mut index_pins),
            index.span,
        );
        lower_mixed_key_array_set(
            ctx, array, array_value, index_value, value_value, span, key_already_diagnosed,
        );
        // The helper only borrows the key cell, so the box made for this write is dropped here.
        crate::ir_lower::ownership::release_if_owned(ctx, index_value, Some(index.span));
        return;
    }
    if op == Op::ArraySet {
        crate::ir_lower::expr::unpin_in_flight_owners(
            ctx,
            std::mem::take(&mut index_pins),
            index.span,
        );
        index_value = coerce_array_key_to_int_at_span(
            ctx, index_value, Some(index.span), key_already_diagnosed,
        );
        // A reference marker is the slot's new IDENTITY, not a value to narrow into the current
        // element type: casting it to `int` stored the referenced value's copy and dropped the
        // reference. Left Mixed, the write below widens the container to boxed slots instead.
        if !matches!(value.kind, ExprKind::ArrayReference(_)) {
            let array_ty = ctx.builder.value_php_type(array_value.value);
            value_value =
                coerce_indexed_array_set_value(ctx, &array_ty, value_value, Some(value.span));
        }
    }
    if op == Op::BufferSet {
        crate::ir_lower::expr::unpin_in_flight_owners(
            ctx,
            std::mem::take(&mut index_pins),
            index.span,
        );
        index_value = coerce_to_int_at_span(ctx, index_value, Some(index.span));
        let buffer_ty = ctx.builder.value_php_type(array_value.value);
        value_value = coerce_buffer_set_value(ctx, &buffer_ty, value_value, Some(value.span));
    }
    if op == Op::HashSet {
        let current_ty = ctx.builder.value_php_type(array_value.value);
        // A `static` hash keeps one storage type across calls, so this write cannot widen it the
        // way it widens an ordinary local: `$s['k'] = $s['k'] + 1` produced a boxed sum, the
        // slot was rewritten as `array<string, mixed>`, and the next call still read `int`
        // values -- the box's address. Coerce into the slot's int element instead, as the
        // indexed write does for every local.
        if matches!(ctx.local_kinds.get(array), Some(LocalKind::StaticLocal)) {
            value_value = coerce_static_hash_set_value(ctx, &current_ty, value_value, Some(value.span));
        }
        let value_ty = ctx.builder.value_php_type(value_value.value);
        if let Some(updated_ty) = assoc_array_write_updated_type(current_ty, value_ty) {
            ctx.prepare_mutated_local_owner(array, array_value, updated_ty.clone(), Some(span));
            let converted = ctx.emit_value(
                Op::HashToMixed,
                vec![array_value.value],
                None,
                updated_ty.clone(),
                Op::HashToMixed.default_effects(),
                Some(span),
            );
            ctx.emit_void(
                Op::HashSet,
                vec![converted.value, index_value.value, value_value.value],
                None,
                Op::HashSet.default_effects(),
                Some(span),
            );
            release_persisted_string_operand(ctx, index_value, span);
            release_persisted_string_operand(ctx, value_value, span);
            ctx.store_prepared_mutated_local(array, converted, updated_ty, Some(span));
            return;
        }
    }
    if op == Op::ArraySet {
        let (array_value, updated_ty, needs_storeback) =
            prepare_indexed_array_local_set(ctx, array_value, value_value, span);
        ctx.emit_void(
            op,
            vec![array_value.value, index_value.value, value_value.value],
            None,
            op.default_effects(),
            Some(span),
        );
        let elem_ty = indexed_array_write_element_type(ctx, array_value, updated_ty.as_ref());
        finish_indexed_array_local_write(
            ctx,
            array,
            array_value,
            updated_ty,
            needs_storeback,
            span,
        );
        release_indexed_array_write_operand(ctx, elem_ty.as_ref(), value_value, span);
        return;
    }
    lower_concrete_array_set_values(
        ctx,
        array_value,
        index_value,
        value_value,
        index_pins,
        span,
        key_already_diagnosed,
    );
    super::array_write_storage::finish_hash_local_write(
        ctx, array, array_value, hash_storeback, span,
    );
}

/// Stores evaluated operands after concrete storage promotion and scalar coercion are complete.
pub(super) fn lower_concrete_array_set_values(
    ctx: &mut LoweringContext<'_, '_>, array: LoweredValue,
    index: LoweredValue, value: LoweredValue,
    index_pins: Vec<crate::ir_lower::expr::PinnedInFlightOwner>,
    span: Span, key_already_diagnosed: bool,
) {
    let op = array_set_op(array.ir_type);
    ctx.emit_void(
        op,
        vec![array.value, index.value, value.value],
        (op == Op::HashSet && key_already_diagnosed).then_some(Immediate::Bool(true)),
        op.default_effects(),
        Some(span),
    );
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, index_pins, span);
    release_persisted_string_operand(ctx, index, span);
    if op == Op::RuntimeCall {
        release_object_offset_set_operand(ctx, value, span);
        // The key is borrowed by `offsetSet()` too. An object key unboxed from a slot another
        // store widened to boxed storage (`$s[$q] = 1` with `$q` later reassigned) is an owned
        // unbox, and `$s[new P()] = 1` builds one; neither was released.
        if !matches!(ctx.builder.value_php_type(index.value).codegen_repr(), PhpType::Str) {
            release_object_offset_set_operand(ctx, index, span);
        }
    } else {
        release_persisted_string_operand(ctx, value, span);
    }
}

/// Releases the value an `ArrayAccess` write handed to `offsetSet()`.
///
/// `$o[$k] = v` on an object is a method call: `offsetSet()` BORROWS its arguments and acquires
/// what it stores, so a fresh temporary of any kind is still this write's to drop. Releasing
/// only strings leaked one array per `$o[$k] = [ ... ]` -- every `SplObjectStorage`/`WeakMap`
/// entry and every user `ArrayAccess` write of a literal.
pub(in crate::ir_lower) fn release_object_offset_set_operand(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) {
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
}

/// Returns whether an integer-keyed indexed write needs a runtime storage dispatch.
///
/// Dense indexed storage can represent only known non-negative literal offsets. A dynamic
/// integer may be negative or sparse, both of which require hash storage in PHP; boxing it and
/// routing through `ArraySetMixedKey` keeps the dense fast path for proven literals while the
/// runtime helper promotes the other cases without losing the key.
fn index_requires_runtime_array_key_dispatch(index: &Expr) -> bool {
    !matches!(index.kind, ExprKind::IntLiteral(value) if value >= 0)
}

/// Returns the widened associative-array type needed by a heterogeneous local write.
///
/// A concrete hash value representation cannot describe a later value of another PHP type.
/// Widening to `Mixed` forces `HashToMixed` to preserve the earlier entries' runtime tags before
/// the new entry is inserted, so later gradual reads and callback boundaries see each real type.
pub(super) fn assoc_array_write_updated_type(current_ty: PhpType, value_ty: PhpType) -> Option<PhpType> {
    let PhpType::AssocArray { key, value } = current_ty.codegen_repr() else {
        return None;
    };
    if hash_storage_represents_written_value(&value.codegen_repr(), &value_ty.codegen_repr()) {
        return None;
    }
    Some(PhpType::AssocArray {
        key,
        value: Box::new(PhpType::Mixed),
    })
}

/// Returns true when a hash stamped with `storage` can hold a `written` value as it stands.
///
/// The question is about the RUNTIME REPRESENTATION, not about PHP assignability: an entry is
/// stored as a raw payload plus the tag `storage` dictates, and every later read materializes it
/// through `storage` as well. So the only writes that need no widening are the ones whose payload
/// already has the stored shape.
///
/// `normalize_array_write_element_type` cannot answer this: it maps every refcounted non-string
/// type to `Mixed`, which conflates a hash whose entries are raw object pointers with one whose
/// entries are boxed Mixed cells. Under that conflation an `array<string, N>` looked like it
/// already accepted anything -- `$nodes = ['node' => $node]; $nodes['n'] = 1;` compiled and then
/// segfaulted reading an integer as an object, and `twig/twig`'s
/// `TestExpression::__construct` -- which writes a `?Node` parameter into exactly such a literal --
/// stopped the backend with `hash_set value PHP type Mixed`.
fn hash_storage_represents_written_value(storage: &PhpType, written: &PhpType) -> bool {
    // Boxed storage describes every payload: each entry carries its own tag.
    if matches!(storage, PhpType::Mixed | PhpType::Iterable) {
        return true;
    }
    if storage == written {
        return true;
    }
    // An object payload is a pointer whose class is read from the object itself -- property and
    // method access dispatch on the runtime class id -- so two classes share one representation.
    // A container payload does NOT: its ELEMENT type is what a read of `$hash[k][i]` materializes,
    // so `['x' => [1, 2]]` followed by `$a['y'] = ['s', 't']` must widen. Letting the two share a
    // slot printed the element's raw pointer as an integer.
    matches!((storage, written), (PhpType::Object(_), PhpType::Object(_)))
}

/// Roots and retires operands borrowed by the boxed writer, including on same-frame catches.
/// The backend copies strings and retains other payloads in a separate consumed box.
/// `key_already_diagnosed` marks the write half of a compound update, whose read already
/// reported a float key's conversion, so the boxed writer rebuilds the key silently.
pub(super) fn lower_boxed_array_set(
    ctx: &mut LoweringContext<'_, '_>,
    array: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
    key_already_diagnosed: bool,
) {
    let mut roots = Vec::new();
    let mut lower_operand = |ctx: &mut LoweringContext<'_, '_>, expr: &Expr| {
        let value = lower_expr(ctx, expr);
        let (value, root) = crate::ir_lower::expr::root_owned_call_operand(ctx, value, expr.span);
        if let Some(root) = root { roots.push(root); }
        value
    };
    // Preserve the existing PHP ordering for bare variable keys versus computed keys.
    let (index, value) = if matches!(index.kind, ExprKind::Variable(_)) {
        let value = lower_operand(ctx, value);
        (lower_operand(ctx, index), value)
    } else {
        let index = lower_operand(ctx, index);
        (index, lower_operand(ctx, value))
    };
    // The box is detached and fetched AFTER the operands, like the typed paths: a by-reference
    // call among them (`$v['r'] = rec($v)`) detaches and republishes the shared cell's box, and a
    // box fetched first received the write and was then dropped -- every such entry vanished.
    // Each operand is rooted above, so one BORROWED from that box (`$context['_parent'] =
    // $context`, which every compiled Twig template starts its loops with) stays alive across
    // the detach, which stores a clone into the slot and releases the box it held.
    if ctx.builder.insertion_block_is_terminated() {
        return;
    }
    // Rooting only covers OWNED temporaries: a borrowed operand (the local itself, or one of its
    // elements) has no root, and the detach below would free the box it reads from. A lifetime
    // pin keeps each borrowed operand alive across the detach and the write.
    let mut pins = Vec::new();
    for operand in [index, value] {
        if ctx.value_is_owning_temporary(operand) {
            continue;
        }
        let pin = crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(
            ctx, operand, Some(span),
        );
        if pin.value != operand.value {
            pins.push(pin);
        }
    }
    let array = load_array_local_for_write(ctx, array, span);
    let (array, receiver_owner) =
        super::nested_array_writes::root_static_update_receiver(ctx, array, span);
    if let Some(owner) = receiver_owner { roots.push(owner); }
    string_offset_writes::lower_boxed_set(
        ctx, array, index, value, span, key_already_diagnosed,
    );
    for pin in pins.into_iter().rev() {
        crate::ir_lower::ownership::release_if_owned(ctx, pin, Some(span));
    }
    for root in roots.into_iter().rev() {
        crate::ir_lower::expr::retire_owned_call_operand(ctx, root, span);
    }
}

/// Stores operands that the caller already evaluated while preserving boxed-writer unwind roots.
pub(super) fn lower_boxed_array_set_values(
    ctx: &mut LoweringContext<'_, '_>,
    array: LoweredValue,
    index: LoweredValue,
    value: LoweredValue,
    span: Span,
    key_already_diagnosed: bool,
) {
    let mut roots = Vec::new();
    let (array, receiver_owner) = super::nested_array_writes::root_static_update_receiver(ctx, array, span);
    if let Some(owner) = receiver_owner { roots.push(owner); }
    let mut root = |ctx: &mut LoweringContext<'_, '_>, operand: LoweredValue| {
        let (operand, owner) = crate::ir_lower::expr::root_owned_call_operand(ctx, operand, span);
        if let Some(owner) = owner {
            roots.push(owner);
        }
        operand
    };
    let index = root(ctx, index);
    let value = root(ctx, value);
    string_offset_writes::lower_boxed_set(
        ctx, array, index, value, span, key_already_diagnosed,
    );
    for root in roots.into_iter().rev() {
        crate::ir_lower::expr::retire_owned_call_operand(ctx, root, span);
    }
}

/// Coerces a buffer element write value into the scalar storage accepted by `BufferSet`.
pub(super) fn coerce_buffer_set_value(
    ctx: &mut LoweringContext<'_, '_>,
    buffer_ty: &PhpType,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    let coerced = match buffer_ty.codegen_repr() {
        PhpType::Buffer(elem_ty) => match elem_ty.storage_value_type().codegen_repr() {
            PhpType::Float => coerce_to_float(ctx, value, span),
            PhpType::Int | PhpType::Bool => coerce_to_int(ctx, value, span),
            _ => value,
        },
        _ => value,
    };
    if coerced.value != value.value && ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, span);
    }
    coerced
}

/// Promotes string-key or potentially sparse first writes into associative storage.
pub(super) fn lower_array_key_hash_promotion(
    ctx: &mut LoweringContext<'_, '_>,
    array: &str,
    array_value: LoweredValue,
    index: LoweredValue,
    value: LoweredValue,
    index_pins: Vec<crate::ir_lower::expr::PinnedInFlightOwner>,
    span: Span,
    key_already_diagnosed: bool,
) {
    let current_ty = ctx.builder.value_php_type(array_value.value);
    let value_ty = ctx.builder.value_php_type(value.value);
    let assoc_ty = promoted_assoc_array_type(current_ty, value_ty);
    ctx.prepare_mutated_local_owner_for_backend_retire(array, array_value, assoc_ty.clone(), Some(span));
    // The slot's reference moves into the conversion; codegen consumes only a marked move.
    let hash = ctx.emit_local_array_to_hash_move(array, array_value, assoc_ty.clone(), span);
    ctx.emit_void(
        Op::HashSet,
        vec![hash.value, index.value, value.value],
        key_already_diagnosed.then_some(Immediate::Bool(true)),
        Op::HashSet.default_effects(),
        Some(span),
    );
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, index_pins, span);
    release_persisted_string_operand(ctx, index, span);
    release_persisted_string_operand(ctx, value, span);
    ctx.store_prepared_mutated_local(array, hash, assoc_ty, Some(span));
}

/// Writes `value` into the indexed local `array` under a boxed Mixed/Union key.
///
/// The destination stays statically `Array(Mixed)` (so indexed consumers such as
/// `implode` keep routing to the indexed path) while `Op::ArraySetMixedKey`
/// dispatches the key tag at runtime: integer keys stay on indexed storage and
/// string keys promote the destination to a hash. This is the Mixed-key analogue
/// of `lower_array_key_hash_promotion`, which promotes known hash-key writes.
pub(super) fn lower_mixed_key_array_set(
    ctx: &mut LoweringContext<'_, '_>,
    array: &str,
    array_value: LoweredValue,
    index: LoweredValue,
    value: LoweredValue,
    span: Span,
    key_already_diagnosed: bool,
) {
    let mixed_array_ty = PhpType::Array(Box::new(PhpType::Mixed));
    let operand_pins = crate::ir_lower::expr::pin_in_flight_owners(
        ctx,
        &[index.value, value.value],
        span,
    );
    // The helper consumes the slot's reference and hands back the one the slot will own -- the
    // same array written in place, a grown or split copy, or the hash it promoted to -- so this is
    // the consuming-mutation protocol `lower_array_key_hash_promotion` uses: a boxed owner is
    // dropped before the write, and the result is stored without releasing the old pointer, which
    // the growth may already have freed or the split left to its other owner.
    ctx.prepare_mutated_local_owner(array, array_value, mixed_array_ty.clone(), Some(span));
    let result = ctx.emit_value(
        Op::ArraySetMixedKey,
        vec![array_value.value, index.value, value.value],
        key_already_diagnosed.then_some(Immediate::Bool(true)),
        mixed_array_ty.clone(),
        Op::ArraySetMixedKey.default_effects(),
        Some(span),
    );
    // The runtime consumes one destination owner and transfers its replacement to the store.
    // Final Mixed frame storage must adopt that owner rather than retain and abandon it.
    ctx.builder.set_value_ownership(result.value, crate::ir::Ownership::Owned);
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, operand_pins, span);
    ctx.store_prepared_mutated_local(array, result, mixed_array_ty, Some(span));
    // The key cell is the caller's to release: the helper only borrows it, and every caller drops
    // the box it made (or the fresh key a call produced) after this returns.
    // The backend retains a Mixed value before the helper stores it, so a call's fresh value cell
    // is released here like any other Mixed-element write operand (`$map[$k] = value()`). A boxed
    // temporary -- an element just read with `$values[$i]` -- keeps its own reference too
    // (`$chunk[$k] = $values[$k]` in a loop that had widened `$k` leaked one box per iteration).
    release_indexed_array_write_operand(ctx, Some(&PhpType::Mixed), value, span);
    // A fresh string the box COPIED -- `$out[$key] = (string) $value` -- is still this write's
    // to drop; without it every such element leaked the cast's string. One the box took over (a
    // call result) is not.
    release_string_copied_into_box(ctx, value, span);
}

/// Returns the associative type produced by a string-key write to an indexed array.
pub(super) fn promoted_assoc_array_type(current_ty: PhpType, value_ty: PhpType) -> PhpType {
    let value_ty = normalize_array_write_element_type(value_ty.codegen_repr());
    let assoc_value_ty = match current_ty.codegen_repr() {
        PhpType::Array(elem_ty) if is_empty_indexed_array_element(elem_ty.as_ref()) => value_ty,
        PhpType::Array(elem_ty) => {
            let elem_ty = normalize_array_write_element_type(elem_ty.codegen_repr());
            if elem_ty == value_ty {
                elem_ty
            } else {
                PhpType::Mixed
            }
        }
        _ => PhpType::Mixed,
    };
    PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(assoc_value_ty),
    }
}

/// Whether `expr` contains an `eval()` whose code is a string literal (the only eval a
/// closed-world build still runs); every other eval there raises instead of running code.
fn contains_literal_eval(expr: &Expr) -> bool {
    let text = format!("{expr:?}");
    let mut rest = text.as_str();
    while let Some(at) = rest.find("\"eval\"") {
        let after = &rest[at..];
        let window_end = after.find("FunctionCall").unwrap_or(after.len()).min(240);
        if after[..window_end].contains("args: [Expr { kind: StringLiteral(") {
            return true;
        }
        rest = &after[5..];
    }
    false
}
