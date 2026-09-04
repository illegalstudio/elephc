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
pub(super) fn release_persisted_string_operand(
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
pub(super) fn lower_write_key_and_value(
    ctx: &mut LoweringContext<'_, '_>,
    index: &Expr,
    value: &Expr,
) -> (LoweredValue, LoweredValue) {
    if matches!(index.kind, ExprKind::Variable(_)) {
        let value_value = lower_expr(ctx, value);
        return (lower_expr(ctx, index), value_value);
    }
    let index_value = lower_expr(ctx, index);
    (index_value, lower_expr(ctx, value))
}

/// Puts an empty array in a container php would auto-vivify, before an index write reads it.
///
/// `load_local` answers a slot no store definitely reached with `Op::WarnedNull` — php's rule for
/// a READ, where an undefined name warns and answers null. A write TARGET is not a read: MEASURED
/// on `php -n` 8.5.6, `function f(): void { $u['k'] = 5; echo $u['k']; } f()` prints `5` and warns
/// nothing, and the same holds for `$u[] = 5`. Without this the container reached the backend as
/// `Void` and the whole program was refused.
///
/// Only an ordinary frame local, and only one the checker already typed as a container — a global
/// alias has storage of its own, and a name typed anything else is not this rule's business.
pub(super) fn vivify_undefined_container(
    ctx: &mut LoweringContext<'_, '_>,
    array: &str,
    span: Span,
) {
    // A `static` local's slot is never marked initialized by a store in THIS frame — its storage
    // is a program-global symbol that outlives the call — so `local_name_is_undefined` says yes on
    // every entry. Vivifying there threw the static's array away each time a method ran:
    // `static $q = []; $q[] = count($q);` counted 1, 1 instead of 1, 2.
    if !ctx.local_is_plain_frame_local(array) || !ctx.local_name_is_undefined(array) {
        return;
    }
    let php_type = match ctx.local_type(array).codegen_repr() {
        ty @ PhpType::AssocArray { .. } => ty,
        ty @ PhpType::Array(_) => ty,
        // A name nothing has stored to has no useful checker type; php vivifies an ARRAY there
        // whatever the key, and a string key promotes it to a hash through the ordinary path.
        _ => PhpType::Array(Box::new(PhpType::Never)),
    };
    let op = match php_type {
        PhpType::AssocArray { .. } => Op::HashNew,
        _ => Op::ArrayNew,
    };
    let empty = ctx.emit_value(
        op,
        Vec::new(),
        Some(Immediate::Capacity(0)),
        php_type.clone(),
        op.default_effects(),
        Some(span),
    );
    ctx.store_local(array, empty, php_type, Some(span));
}

/// Lowers an indexed array assignment.
pub(super) fn lower_array_assign(
    ctx: &mut LoweringContext<'_, '_>,
    array: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
) {
    vivify_undefined_container(ctx, array, span);
    let array_value = ctx.load_local(array, Some(span));
    let (mut index_value, mut value_value) = lower_write_key_and_value(ctx, index, value);
    let op = array_set_op(array_value.ir_type);
    // A literal string index always means a hash key, so promote the destination
    // to associative storage like PHP. A boxed Mixed/Union index may hold either
    // an integer or a string key (foreach loop keys are always Mixed in EIR via
    // `Op::IterCurrentKey`), so it goes through `Op::ArraySetMixedKey`, whose
    // runtime helper keeps integer keys on indexed storage (preserving indexed
    // consumers like `implode`) and promotes only string keys to a hash. This
    // stops a `foreach($arr as $k=>$v) $dst[$k]=$v` rebuild from collapsing a
    // string key onto int 0. A foreach key over a concretely-indexed array is
    // known to be int-valued, so it is left on the coerce path to avoid
    // needlessly dispatching.
    if op == Op::ArraySet && index_value.ir_type == IrType::Str {
        lower_string_key_array_promotion(ctx, array, array_value, index_value, value_value, span);
        return;
    }
    if op == Op::ArraySet
        && index_is_boxed_mixed_key(index_value.ir_type)
        && !index_is_foreach_int_key(ctx, index)
    {
        lower_mixed_key_array_set(ctx, array, array_value, index_value, value_value, span);
        return;
    }
    if op == Op::ArraySet {
        index_value = coerce_to_int_at_span(ctx, index_value, Some(index.span));
        let array_ty = ctx.builder.value_php_type(array_value.value);
        value_value = coerce_indexed_array_set_value(ctx, &array_ty, value_value, Some(value.span));
    }
    if op == Op::BufferSet {
        index_value = coerce_to_int_at_span(ctx, index_value, Some(index.span));
        let buffer_ty = ctx.builder.value_php_type(array_value.value);
        value_value = coerce_buffer_set_value(ctx, &buffer_ty, value_value, Some(value.span));
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
    ctx.emit_void(
        op,
        vec![array_value.value, index_value.value, value_value.value],
        None,
        op.default_effects(),
        Some(span),
    );
    release_persisted_string_operand(ctx, index_value, span);
    release_persisted_string_operand(ctx, value_value, span);
}

/// Coerces a buffer element write value into the scalar storage accepted by `BufferSet`.
pub(super) fn coerce_buffer_set_value(
    ctx: &mut LoweringContext<'_, '_>,
    buffer_ty: &PhpType,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    let coerced = match buffer_ty.codegen_repr() {
        PhpType::Buffer(elem_ty) => match elem_ty.codegen_repr() {
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

/// Promotes an indexed local array to a Mixed-valued associative array for string-key writes.
pub(super) fn lower_string_key_array_promotion(
    ctx: &mut LoweringContext<'_, '_>,
    array: &str,
    array_value: LoweredValue,
    index: LoweredValue,
    value: LoweredValue,
    span: Span,
) {
    let current_ty = ctx.builder.value_php_type(array_value.value);
    let value_ty = ctx.builder.value_php_type(value.value);
    let assoc_ty = promoted_assoc_array_type(current_ty, value_ty);
    ctx.prepare_mutated_local_owner(array, array_value, assoc_ty.clone(), Some(span));
    let hash = ctx.emit_value(
        Op::ArrayToHash,
        vec![array_value.value],
        None,
        assoc_ty.clone(),
        Op::ArrayToHash.default_effects(),
        Some(span),
    );
    ctx.emit_void(
        Op::HashSet,
        vec![hash.value, index.value, value.value],
        None,
        Op::HashSet.default_effects(),
        Some(span),
    );
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
/// of `lower_string_key_array_promotion`, which unconditionally promotes because
/// a literal string key is always a hash key.
pub(super) fn lower_mixed_key_array_set(
    ctx: &mut LoweringContext<'_, '_>,
    array: &str,
    array_value: LoweredValue,
    index: LoweredValue,
    value: LoweredValue,
    span: Span,
) {
    let mixed_array_ty = PhpType::Array(Box::new(PhpType::Mixed));
    let result = ctx.emit_value(
        Op::ArraySetMixedKey,
        vec![array_value.value, index.value, value.value],
        None,
        mixed_array_ty.clone(),
        Op::ArraySetMixedKey.default_effects(),
        Some(span),
    );
    ctx.store_mutated_local(array, result, mixed_array_ty, Some(span));
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

