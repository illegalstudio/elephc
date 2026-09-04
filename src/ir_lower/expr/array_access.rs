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
        if value_is_nullable(ctx, array_value.value) {
            return lower_nullable_array_access(ctx, array_value, index, expr, true);
        }
        return lower_array_access_from_value(ctx, array_value, index, expr, true);
    };
    let index_value = lower_expr(ctx, index);
    // The hash lookup normalizes its own key, so only the indexed slot arithmetic needs an
    // int-coerced one.
    let index_value = if op == Op::ArrayGetForWrite {
        coerce_to_int_at_span(ctx, index_value, Some(index.span))
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

/// Lowers array, hash, string, or ArrayAccess indexing with configurable
/// undefined-offset warning behavior for native indexed-array reads. Suppressed
/// warnings propagate through the whole subscript chain: PHP's `isset()` and `??`
/// are silent for every level of `$a[1][2][3]`, not just the outermost read.
pub(super) fn lower_array_access_with_missing_warning(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
    index: &Expr,
    expr: &Expr,
    warn_on_missing: bool,
) -> LoweredValue {
    let array_value = if warn_on_missing {
        lower_expr(ctx, array)
    } else {
        lower_subscript_receiver_silently(ctx, array)
    };
    // The twin of the property-access arm: PHP reads an offset through a null base rather than
    // refusing, raising `Trying to access array offset on null` and answering NULL — MEASURED on
    // `php -n` 8.5.6. `warn_on_missing` is false exactly in the probe constructs, which is what
    // keeps `isset($n['k'])` silent. The INDEX is still lowered, because PHP evaluates it.
    if warn_on_missing
        && !ctx.in_null_probe()
        && value_is_definitely_null(ctx, array_value.value)
    {
        lower_expr(ctx, index);
        return ctx.emit_warned_null(
            "Warning: Trying to access array offset on null\n",
            Some(expr.span),
        );
    }
    // A SCALAR base is the same php event with a different word: `Trying to access array offset
    // on false / true / int / float`, answering NULL. elephc refused the whole program with
    // `Cannot index non-array` — the worst possible response to a warning php survives.
    if let Some(base) = scalar_offset_base(ctx, array_value.value) {
        // php reads the index either way: it is an ordinary expression that happens to sit
        // inside a probe, and only the CHAIN ROOT is what `isset()` tolerates.
        lower_expr(ctx, index);
        // Inside `isset()` / `??` php raises NOTHING and answers the same null — the probe
        // constructs exist to name storage that may not be there. Indexing the scalar anyway
        // answered `true` for `isset(false['k'])` and then crashed.
        if warn_on_missing && !ctx.in_null_probe() {
            return emit_scalar_offset_warning(ctx, array_value, base, expr);
        }
        return lower_null(ctx, expr);
    }
    if value_is_nullable(ctx, array_value.value) {
        return lower_nullable_array_access(ctx, array_value, index, expr, warn_on_missing);
    }
    lower_array_access_from_value(ctx, array_value, index, expr, warn_on_missing)
}

/// Which scalar php names in `Trying to access array offset on <word>`.
#[derive(Clone, Copy)]
enum ScalarOffsetBase {
    /// The word is settled while lowering: `false`, `int`, `float`.
    Known(&'static str),
    /// A `bool` whose VALUE decides the word. php names what the variable actually holds, and a
    /// literal `true` types as plain `bool` here, so this is the ordinary spelling of `true`.
    BoolAtRuntime,
}

/// Returns the scalar php would name for an offset read through `value`, if any.
///
/// A STRING base is not one — `"abc"[1]` is a legal read handled elsewhere — and neither is
/// `Mixed`, which carries its type at run time and is dispatched by the runtime helpers.
fn scalar_offset_base(
    ctx: &LoweringContext<'_, '_>,
    value: crate::ir::ValueId,
) -> Option<ScalarOffsetBase> {
    match ctx.builder.value_php_type(value) {
        PhpType::False => Some(ScalarOffsetBase::Known("false")),
        PhpType::Int => Some(ScalarOffsetBase::Known("int")),
        PhpType::Float => Some(ScalarOffsetBase::Known("float")),
        PhpType::Bool => Some(ScalarOffsetBase::BoolAtRuntime),
        _ => None,
    }
}

/// Emits php's offset-on-scalar warning and its NULL answer.
///
/// The `bool` case is a two-armed branch rather than one message, because php names the VALUE:
/// one site produces different text for a variable holding `true` and one holding `false`. Both
/// arms answer null, so nothing needs merging — the value is produced after the join.
fn emit_scalar_offset_warning(
    ctx: &mut LoweringContext<'_, '_>,
    array_value: LoweredValue,
    base: ScalarOffsetBase,
    expr: &Expr,
) -> LoweredValue {
    if let ScalarOffsetBase::Known(word) = base {
        return ctx.emit_warned_null(
            &format!("Warning: Trying to access array offset on {word}\n"),
            Some(expr.span),
        );
    }
    let true_block = ctx.builder.create_named_block("offset_on_true", Vec::new());
    let false_block = ctx.builder.create_named_block("offset_on_false", Vec::new());
    let merge = ctx.builder.create_named_block("offset_on_bool_done", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: array_value.value,
        then_target: true_block,
        then_args: Vec::new(),
        else_target: false_block,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(true_block);
    ctx.emit_warned_null(
        "Warning: Trying to access array offset on true\n",
        Some(expr.span),
    );
    branch_to(ctx, merge);
    ctx.builder.position_at_end(false_block);
    ctx.emit_warned_null(
        "Warning: Trying to access array offset on false\n",
        Some(expr.span),
    );
    branch_to(ctx, merge);
    ctx.builder.position_at_end(merge);
    lower_null(ctx, expr)
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
    // "Silently" reaches the undefined-variable warning too: `isset($a[$b])` PROBES `$a` and
    // READS `$b`, and PHP warns about `$b` alone. Without this the probe's own spine raised the
    // warning the construct exists to avoid.
    lower_null_probe_chain(ctx, array)
}

/// Lowers the CHAIN a null probe examines, without PHP's undefined-variable warning.
///
/// `isset($x)`, `empty($x)` and `$x ?? "d"` exist to ask whether storage was ever named, so PHP
/// raises nothing for the chain itself — MEASURED on `php -n` 8.5.6, which prints only the
/// result for all three. What sits INSIDE the chain stays an ordinary read: the same PHP warns
/// about `$b` in `isset($a[$b])`, and about `$y` in `$x ?? $y`. Array operands reach here
/// through `lower_subscript_receiver_silently`, which descends the chain and leaves the index
/// outside the spine — that split is what keeps the two halves of the rule apart.
pub(super) fn lower_null_probe_chain(
    ctx: &mut LoweringContext<'_, '_>,
    chain: &Expr,
) -> LoweredValue {
    // A subscript link DESCENDS rather than being silenced whole: its own receiver continues
    // the chain while its index steps back out and is read normally. Silencing the link
    // outright would take the index with it, and `isset($a[$b]->p)` warns about `$b` in PHP.
    if let ExprKind::ArrayAccess { array, index } = &chain.kind {
        return lower_array_access_with_missing_warning(ctx, array, index, chain, false);
    }
    ctx.enter_probe_spine();
    let value = lower_expr(ctx, chain);
    ctx.leave_probe_spine();
    value
}

/// Lowers array access once the receiver is already evaluated.
pub(super) fn lower_array_access_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    array_value: LoweredValue,
    index: &Expr,
    expr: &Expr,
    warn_on_missing: bool,
) -> LoweredValue {
    let mut index_value = lower_expr(ctx, index);
    let op = match array_value.ir_type {
        IrType::Heap(IrHeapKind::Array) => {
            let index_ty = lowered_index_expr_key_type(ctx, index, index_value.value);
            // A genuinely boxed Mixed key is materialized by array codegen. Do not coerce it
            // here: string keys would become integer zero, while checked integer loop counters
            // use I64 and therefore still take the ordinary coercion path below.
            let index_is_mixed = matches!(index_value.ir_type, IrType::Heap(IrHeapKind::Mixed));
            if index_is_mixed {
                if warn_on_missing {
                    Op::ArrayGet
                } else {
                    Op::ArrayGetSilent
                }
            } else if index_ty == PhpType::Int {
                index_value = coerce_to_int_at_span(ctx, index_value, Some(index.span));
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
            index_value = coerce_to_int_at_span(ctx, index_value, Some(index.span));
            Op::StrCharAt
        }
        _ => Op::RuntimeCall,
    };
    let result_type = array_access_result_type(ctx, array_value.value, op, expr);
    let mut operands = vec![array_value.value, index_value.value];
    if matches!(op, Op::RuntimeCall) {
        let warning_flag = emit_bool_literal(ctx, warn_on_missing, Some(expr.span));
        operands.push(warning_flag.value);
    }
    let result = ctx.emit_value(
        op,
        operands,
        None,
        result_type,
        op.default_effects(),
        Some(expr.span),
    );
    // An owning boxed index temporary (e.g. `$B[$i + 1]` on the mixed-key read
    // path) is consumed by the read without any runtime refcount operation on
    // the key, and the result is freshly allocated storage that never aliases
    // it — release it here or it leaks per read (issue #500). Int-coerced
    // index paths rebound `index_value` to a non-owning raw cast, so the
    // owning-temporary gate makes this a no-op for them.
    release_coerced_source_if_owned(ctx, index_value, Some(index.span));
    // Array access consumes an owning receiver produced by an earlier read,
    // call, or one-shot temp. Preserve borrowed string/callable payloads before
    // dropping that receiver; boxed and retained container reads are already
    // independent and must not be acquired twice.
    stabilize_borrowed_result_and_release_receiver(ctx, array_value, result, expr.span)
}

/// Lowers nullable receiver indexing without evaluating the index on a null receiver.
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
    let null_value = lower_boxed_null(ctx, expr);
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
    if value_is_nullable(ctx, receiver.value) {
        return lower_nullable_array_access(ctx, receiver, index, expr, true);
    }
    lower_array_access_from_value(ctx, receiver, index, expr, true)
}

/// Returns the statically-known key type for an array index expression.
/// Used to decide between Op::ArrayGet (int key) and Op::ArrayGetMixedKey.
pub(crate) fn index_expr_key_type(_ctx: &LoweringContext<'_, '_>, index: &Expr) -> PhpType {
    let ty = infer_expr_type_syntactic(index);
    normalized_array_key_type(index, ty)
}

/// Refines a read key's syntactic type from its lowered SSA value when it is definitely a string.
pub(super) fn lowered_index_expr_key_type(
    ctx: &LoweringContext<'_, '_>,
    index: &Expr,
    index_value: ValueId,
) -> PhpType {
    let syntactic = index_expr_key_type(ctx, index);
    if syntactic == PhpType::Int && ctx.builder.value_php_type(index_value) == PhpType::Str {
        return normalized_array_key_type(index, PhpType::Str);
    }
    syntactic
}

/// Refines an `isset` key from its lowered value, including boxed Mixed keys.
pub(super) fn isset_index_expr_key_type(
    ctx: &LoweringContext<'_, '_>,
    index: &Expr,
    index_value: ValueId,
) -> PhpType {
    let syntactic = index_expr_key_type(ctx, index);
    if syntactic != PhpType::Int {
        return syntactic;
    }
    let lowered = ctx.builder.value_php_type(index_value);
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
            PhpType::Array(elem_ty) => {
                array_access_element_result_type(normalize_value_php_type(*elem_ty))
            }
            _ => fallback_expr_type(expr),
        },
        Op::HashGet | Op::HashGetSilent => match ctx.builder.value_php_type(array).codegen_repr() {
            PhpType::AssocArray { value, .. } => {
                array_access_element_result_type(normalize_value_php_type(*value))
            }
            _ => fallback_expr_type(expr),
        },
        Op::BufferGet => match ctx.builder.value_php_type(array).codegen_repr() {
            PhpType::Buffer(elem_ty) => normalize_value_php_type(*elem_ty),
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
pub(crate) fn array_access_element_result_type(element_ty: PhpType) -> PhpType {
    if crate::codegen::sentinels::null_repr_is_tagged() && matches!(element_ty, PhpType::Int) {
        PhpType::TaggedScalar
    } else {
        element_ty
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
        PhpType::Mixed => PhpType::Mixed,
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
        _ => infer_expr_type_syntactic(array),
    };
    type_satisfies_array_access_for_ir(ctx, &ty)
}
