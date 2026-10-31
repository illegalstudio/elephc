//! Purpose:
//! Indexed array literal and spread lowering.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.
//! - An `(array)` cast used as an element is typed by the shared `ArrayCast` classification
//!   from the type lowering attaches to the source, so the literal stores that array.

use super::*;
use crate::parser::ast::ArrayEntry;

/// Distinguishes pre-lowered array-literal items between plain elements and spread operands.
pub(super) enum SpreadItem {
    Element(LoweredValue),
    Spread(LoweredValue),
}

/// Lowers an indexed array literal.
pub(super) fn lower_array_literal(ctx: &mut LoweringContext<'_, '_>, items: &[Expr], expr: &Expr) -> LoweredValue {
    // Fast path: literals without any spread keep the original dest-first lowering so the
    // common `[1, 2, 3]` form does not reorder allocation relative to element evaluation.
    if !items.iter().any(|item| matches!(item.kind, ExprKind::Spread(_))) {
        let array_ty = array_literal_type_for_ir(ctx, items, expr);
        return lower_array_literal_without_spread(ctx, items, expr, array_ty);
    }
    // Validate boxed spreads as they are evaluated, before later element side effects.
    // Root owned items across subsequent evaluation and allocate the destination only once
    // every spread has a valid concrete container layout.
    let mut lowered: Vec<SpreadItem> = Vec::with_capacity(items.len());
    let mut roots = Vec::new();
    let mut any_assoc_spread = false;
    let pointer_elem = literal_pointer_element_type(&array_literal_type_for_ir(ctx, items, expr));
    for item in items {
        match &item.kind {
            ExprKind::Spread(inner) => {
                let source = lower_spread_source(ctx, inner, pointer_elem.as_ref());
                let source = if source.ir_type == IrType::Heap(IrHeapKind::Mixed) {
                    lower_boxed_array_spread_source(ctx, source, item.span)
                } else {
                    source
                };
                if matches!(
                    ctx.builder.value_php_type(source.value).codegen_repr(),
                    PhpType::AssocArray { .. } | PhpType::Mixed
                ) {
                    any_assoc_spread = true;
                }
                let source = root_array_literal_item(ctx, source, item.span, &mut roots);
                lowered.push(SpreadItem::Spread(source));
            }
            _ => {
                let value = lower_expr(ctx, item);
                let value = retype_pointer_literal_value(ctx, value, pointer_elem.as_ref(), item.span);
                let value = root_array_literal_item(ctx, value, item.span, &mut roots);
                lowered.push(SpreadItem::Element(value));
            }
        }
    }
    let result = if any_assoc_spread {
        lower_array_literal_as_hash_from_lowered(ctx, items, &lowered, expr)
    } else {
        lower_array_literal_as_indexed_from_lowered(ctx, items, &lowered, expr)
    };
    for slot in roots.into_iter().rev() {
        retire_owned_call_operand(ctx, slot, expr.span);
    }
    result
}

/// Roots owned literal items across later evaluation and lends their stable slot value to insertion.
pub(super) fn root_array_literal_item(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
    roots: &mut Vec<crate::ir::LocalSlotId>,
) -> LoweredValue {
    let (rooted, owner) = root_owned_call_operand(ctx, value, span);
    let Some(slot) = owner else { return rooted; };
    roots.push(slot);
    let php_type = ctx.builder.value_php_type(rooted.value);
    let value = ctx.builder.emit_load_local(slot, rooted.ir_type, php_type);
    LoweredValue { value, ir_type: ctx.builder.value_type(value) }
}

/// Validates and copies a boxed spread without leaving its expression owner live after a throw.
pub(super) fn lower_boxed_array_spread_source(
    ctx: &mut LoweringContext<'_, '_>,
    source: LoweredValue,
    span: Span,
) -> LoweredValue {
    let (source, owner) = root_owned_call_operand(ctx, source, span);
    let result = ctx.emit_value(
        Op::RuntimeCall,
        vec![source.value],
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::ArrayUnpackToHash)),
        PhpType::AssocArray { key: Box::new(PhpType::Mixed), value: Box::new(PhpType::Mixed) },
        effects_lookup::runtime_effects(),
        Some(span),
    );
    if let Some(slot) = owner {
        retire_owned_call_operand(ctx, slot, span);
    }
    result
}

/// Lowers an indexed array literal using a contextual element storage type.
pub(crate) fn lower_array_literal_with_expected_type(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
    elem_ty: PhpType,
) -> LoweredValue {
    let ExprKind::ArrayLiteral(items) = &expr.kind else {
        return lower_expr(ctx, expr);
    };
    if items.iter().any(|item| matches!(item.kind, ExprKind::Spread(_))) {
        return lower_array_literal(ctx, items, expr);
    }
    let array_ty = expected_indexed_array_literal_type(elem_ty);
    lower_array_literal_without_spread(ctx, items, expr, array_ty)
}

/// Lowers an expression bound to a typed destination (a property, a parameter, a default).
///
/// A literal of only `null`s (`[null]`, `["k" => null]`) has no pointer of its own to choose
/// pointer storage from and is built with boxed `mixed` slots, which a later pointer append
/// and read misinterpret. When the destination's element or value type is a pointer, the
/// literal takes that type instead, so each `null` is the null pointer word. A literal with a
/// spread and every other expression lower as usual and are then converted to pointer storage.
pub(crate) fn lower_expr_for_destination(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
    destination: &PhpType,
) -> LoweredValue {
    // A named argument (`f(items: [null])`) binds its value to the same destination.
    if let ExprKind::NamedArg { value, .. } = &expr.kind {
        return lower_expr_for_destination(ctx, value, destination);
    }
    // The branches of a ternary bound to a pointer array or hash share its destination.
    if let ExprKind::Ternary { condition, then_expr, else_expr } = &expr.kind {
        if literal_pointer_element_type(destination).is_some() {
            return super::ternary_cast::lower_ternary_with_destination(
                ctx,
                condition,
                then_expr,
                else_expr,
                expr,
                Some(destination),
            );
        }
    }
    match (destination.codegen_repr(), &expr.kind) {
        (PhpType::Array(elem), ExprKind::ArrayLiteral(_))
            if matches!(elem.codegen_repr(), PhpType::Pointer(_)) =>
        {
            // A literal with a spread keeps its own element type, so `[...[null]]` still
            // builds boxed cells and needs the same conversion as any other array source.
            let value = lower_array_literal_with_expected_type(ctx, expr, *elem);
            convert_array_to_pointer_storage(ctx, value, destination, expr.span)
        }
        (PhpType::AssocArray { key, value }, ExprKind::ArrayLiteralAssoc(pairs))
            if matches!(value.codegen_repr(), PhpType::Pointer(_)) =>
        {
            let hash_ty = PhpType::AssocArray {
                key,
                value: Box::new(value.codegen_repr()),
            };
            lower_assoc_array_literal_as(ctx, pairs, expr, false, hash_ty)
        }
        _ => {
            let value = lower_expr(ctx, expr);
            let value = convert_array_to_pointer_storage(ctx, value, destination, expr.span);
            // A pointer container bound to `array<mixed>` storage widens its words the way
            // boxing does, so a null element is PHP null there as well.
            let mixed_payload = match destination.codegen_repr() {
                PhpType::Array(elem) => elem.codegen_repr() == PhpType::Mixed,
                PhpType::AssocArray { value, .. } => value.codegen_repr() == PhpType::Mixed,
                _ => false,
            };
            if mixed_payload {
                ctx.widen_pointer_container_to_mixed(value, Some(expr.span))
            } else {
                value
            }
        }
    }
}

/// Converts an indexed array bound to a pointer-array destination into pointer storage.
///
/// A literal of only `null`s held in a local (`$a = [null]`), also when a function without a
/// return type returns that local or the literal itself (`function boxed() { $a = [null];
/// return $a; }`, an `array<mixed>` at the call site), is an array of boxed Mixed cells, which a
/// pointer-array reader would take for addresses. Whether the slots are boxed
/// is a runtime property of the array's header, not of its static element type, so every source
/// whose element is not already a pointer goes through `Op::ArrayToPointerWords`: it rewrites a
/// boxed null as the null pointer word and a boxed integer as its address, and passes arrays that
/// already hold pointer words (or no elements) through. Hashes keep per-entry tags their readers
/// already check.
///
/// The op consumes one owned reference: an owning temporary (a call result) is handed over as it
/// is, and anything else (a local load) is retained first, so the local keeps its own array.
pub(super) fn convert_array_to_pointer_storage(
    ctx: &mut LoweringContext<'_, '_>,
    source: LoweredValue,
    destination: &PhpType,
    span: crate::span::Span,
) -> LoweredValue {
    let PhpType::Array(_) = destination.codegen_repr() else {
        return source;
    };
    let Some(pointer_elem) = literal_pointer_element_type(destination) else {
        return source;
    };
    match ctx.builder.value_php_type(source.value).codegen_repr() {
        PhpType::Array(elem) if !matches!(elem.codegen_repr(), PhpType::Pointer(_)) => {}
        _ => return source,
    }
    let owned = if ctx.value_is_owning_temporary(source)
        && !ctx.value_is_owned_unboxed_local_load(source.value)
    {
        source
    } else {
        crate::ir_lower::ownership::acquire_if_refcounted(ctx, source, Some(span))
    };
    ctx.emit_owned_value(
        Op::ArrayToPointerWords,
        vec![owned.value],
        None,
        PhpType::Array(Box::new(pointer_elem)),
        Op::ArrayToPointerWords.default_effects(),
        Some(span),
    )
}

/// Returns an indexed-array type for contextual literal lowering.
pub(super) fn expected_indexed_array_literal_type(elem_ty: PhpType) -> PhpType {
    PhpType::Array(Box::new(elem_ty.codegen_repr()))
}

/// Lowers a no-spread indexed array literal into the requested array storage type.
pub(super) fn lower_array_literal_without_spread(
    ctx: &mut LoweringContext<'_, '_>,
    items: &[Expr],
    expr: &Expr,
    array_ty: PhpType,
) -> LoweredValue {
    let elem_ty = indexed_array_literal_element_type(&array_ty);
    let result_ty = array_ty.clone();
    let array = ctx.emit_value(
        Op::ArrayNew,
        Vec::new(),
        Some(Immediate::Capacity(items.len() as u32)),
        array_ty,
        Op::ArrayNew.default_effects(),
        Some(expr.span),
    );
    let owner = crate::ir_lower::function::contains_deferred_error(expr)
        .then(|| publish_constructed_container(ctx, array, expr.span));
    for item in items {
        let value = lower_expr(ctx, item);
        let value = coerce_array_literal_element_to_storage_type(ctx, value, elem_ty.as_ref(), item);
        let array = owner.map(|slot| load_published_container(ctx, slot, result_ty.clone(), item.span))
            .unwrap_or(array);
        ctx.emit_void(
            Op::ArrayPush,
            vec![array.value, value.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(item.span),
        );
        crate::ir_lower::stmt::release_indexed_array_write_operand(ctx, elem_ty.as_ref(), value, item.span);
    }
    owner.map(|slot| take_published_container(ctx, slot, result_ty, expr.span)).unwrap_or(array)
}

/// Coerces an array literal element to the contextual storage type when needed.
pub(super) fn coerce_array_literal_element_to_storage_type(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    elem_ty: Option<&PhpType>,
    expr: &Expr,
) -> LoweredValue {
    let Some(elem_ty) = elem_ty else {
        return value;
    };
    let coerced = match elem_ty.codegen_repr() {
        PhpType::Int | PhpType::Bool if value.ir_type != IrType::I64 => {
            coerce_to_int(ctx, value, expr)
        }
        PhpType::Float if value.ir_type != IrType::F64 => coerce_to_float(ctx, value, expr),
        PhpType::Str if value.ir_type != IrType::Str => coerce_to_string(ctx, value, expr),
        // A packed record address or `null` stored as a pointer element is the pointer word.
        PhpType::Pointer(_) => ctx.retype_packed_address_for_pointer_sink(value, elem_ty, Some(expr.span)),
        _ => value,
    };
    // The scalar coercers release owning heap-repr sources internally (see
    // `release_coerced_source_if_owned`); releasing those here again would
    // double-free the element box. Integer string conversions also retire their
    // source, while an identity string conversion keeps the same value below.
    if coerced.value != value.value
        && !(value.ir_type == IrType::Str && coerced.ir_type == IrType::I64)
        && !coerced_source_repr_is_releasable(&ctx.builder.value_php_type(value.value))
        && ctx.value_is_owning_temporary(value)
    {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(expr.span));
    }
    coerced
}

/// Lowers a spread-containing indexed-array literal whose spread sources are all indexed arrays.
pub(super) fn lower_array_literal_as_indexed_from_lowered(
    ctx: &mut LoweringContext<'_, '_>,
    items: &[Expr],
    lowered: &[SpreadItem],
    expr: &Expr,
) -> LoweredValue {
    let array_ty = array_literal_type_for_ir(ctx, items, expr);
    let elem_ty = indexed_array_literal_element_type(&array_ty);
    let array = ctx.emit_value(
        Op::ArrayNew,
        Vec::new(),
        Some(Immediate::Capacity(items.len() as u32)),
        array_ty,
        Op::ArrayNew.default_effects(),
        Some(expr.span),
    );
    for (item, value) in items.iter().zip(lowered.iter()) {
        match value {
            SpreadItem::Spread(source) => {
                lower_indexed_array_spread_into_array(ctx, array, *source, elem_ty.as_ref(), item.span);
            }
            SpreadItem::Element(value) => {
                ctx.emit_void(Op::ArrayPush, vec![array.value, value.value], None, Op::ArrayPush.default_effects(), Some(item.span));
                crate::ir_lower::stmt::release_indexed_array_write_operand(ctx, elem_ty.as_ref(), *value, item.span);
            }
        }
    }
    array
}

/// Lowers a spread-containing array literal with at least one associative spread as a hash.
pub(super) fn lower_array_literal_as_hash_from_lowered(
    ctx: &mut LoweringContext<'_, '_>,
    items: &[Expr],
    lowered: &[SpreadItem],
    expr: &Expr,
) -> LoweredValue {
    let hash_ty =
        assoc_array_literal_type_from_spreads(ctx, items, expr, &spread_sources_storage(ctx, items));
    let value_ty = match hash_ty.codegen_repr() {
        PhpType::AssocArray { value, .. } => value.codegen_repr(),
        _ => PhpType::Mixed,
    };
    let hash = ctx.emit_value(
        Op::HashNew,
        Vec::new(),
        Some(Immediate::Capacity(items.len() as u32)),
        hash_ty,
        Op::HashNew.default_effects(),
        Some(expr.span),
    );
    for (item, value) in items.iter().zip(lowered.iter()) {
        match value {
            SpreadItem::Spread(source) => {
                lower_hash_spread_into_hash_from_value(ctx, hash, *source, item.span);
            }
            SpreadItem::Element(value) => {
                ctx.emit_void(
                    Op::RuntimeCall,
                    vec![hash.value, value.value],
                    None,
                    effects_lookup::runtime_effects(),
                    Some(item.span),
                );
                release_value_after_retaining_insert(ctx, Some(&value_ty), *value, item.span);
            }
        }
    }
    hash
}

/// Lowers an array literal that mixes spreads with explicit keys.
///
/// The destination is always a hash: an explicit key means the result cannot be packed storage,
/// and a spread means the entry count is not known until it runs. Each entry is lowered where it
/// appears, because the ORDER is observable -- a spread's elements take the next free integer
/// key at the point the spread sits, so `["a" => 1, ...$v, 2]` and `[...$v, "a" => 1, 2]` do not
/// agree on which key the trailing `2` gets.
///
/// The three entry forms reuse what the two single-shape paths already do: a spread goes through
/// `lower_hash_spread_into_hash_from_value` (which promotes an indexed source first), a keyed
/// entry emits `Op::HashSet` as `lower_assoc_array_literal` does, and a bare value appends
/// through the same runtime call the hash path uses for a spread-literal element.
pub(super) fn lower_mixed_array_literal(
    ctx: &mut LoweringContext<'_, '_>,
    entries: &[ArrayEntry],
    expr: &Expr,
) -> LoweredValue {
    let hash_ty = assoc_array_literal_type_from_entries(ctx, entries, expr);
    let value_ty = match hash_ty.codegen_repr() {
        PhpType::AssocArray { value, .. } => value.codegen_repr(),
        _ => PhpType::Mixed,
    };
    let pointer_elem = literal_pointer_element_type(&hash_ty);
    let hash = ctx.emit_value(
        Op::HashNew,
        Vec::new(),
        Some(Immediate::Capacity(entries.len() as u32)),
        hash_ty,
        Op::HashNew.default_effects(),
        Some(expr.span),
    );
    for entry in entries {
        match entry {
            ArrayEntry::Spread(source) => {
                let span = source.span;
                let source = lower_spread_source(ctx, source, pointer_elem.as_ref());
                // A boxed source (a declared `array` parameter, a `mixed` value) has to be
                // unboxed and validated before the hash spread reads its layout.
                let source = if source.ir_type == IrType::Heap(IrHeapKind::Mixed) {
                    lower_boxed_array_spread_source(ctx, source, span)
                } else {
                    source
                };
                lower_hash_spread_into_hash_from_value(ctx, hash, source, span);
            }
            ArrayEntry::Keyed(key, value) => {
                let span = key.span;
                let reference_source = match &value.kind {
                    ExprKind::Reference(inner) => Some(inner.as_ref()),
                    _ => None,
                };
                let value_expr = reference_source.unwrap_or(value);
                let key = lower_expr(ctx, key);
                let (key, key_owner) = root_owned_call_operand(ctx, key, span);
                let value_span = value_expr.span;
                let value = lower_expr(ctx, value_expr);
                let value = retype_pointer_literal_value(ctx, value, pointer_elem.as_ref(), value_span);
                ctx.emit_void(
                    Op::HashSet,
                    vec![hash.value, key.value, value.value],
                    None,
                    Op::HashSet.default_effects(),
                    Some(span),
                );
                if let Some(source) = reference_source {
                    let cell = ctx.emit_value(
                        Op::LoadArrayElemRefCell,
                        vec![hash.value, key.value],
                        None,
                        PhpType::Mixed,
                        Op::LoadArrayElemRefCell.default_effects(),
                        Some(value_span),
                    );
                    crate::ir_lower::expr::assoc_array_literals::bind_reference_literal_source(
                        ctx,
                        source,
                        cell,
                        PhpType::Mixed,
                        value_span,
                    );
                }
                if let Some(owner) = key_owner {
                    retire_owned_call_operand(ctx, owner, span);
                }
            }
            ArrayEntry::Value(value) => {
                let span = value.span;
                let value = lower_expr(ctx, value);
                let value = retype_pointer_literal_value(ctx, value, pointer_elem.as_ref(), span);
                ctx.emit_void(
                    Op::RuntimeCall,
                    vec![hash.value, value.value],
                    None,
                    effects_lookup::runtime_effects(),
                    Some(span),
                );
                release_value_after_retaining_insert(ctx, Some(&value_ty), value, span);
            }
        }
    }
    hash
}

/// Returns the hash type for a mixed literal, merging every entry's value type.
///
/// A spread contributes its SOURCE's element type, exactly as it does for a spread-only literal;
/// a keyed entry and a bare value contribute the value's own type.
pub(super) fn assoc_array_literal_type_from_entries(
    ctx: &LoweringContext<'_, '_>,
    entries: &[ArrayEntry],
    expr: &Expr,
) -> PhpType {
    // An entry of only pointers, pointer spreads and `null`s is a pointer hash, with or without a
    // `null`: the merge below sees a spread entry's element type where it expects the source's
    // container, so it would widen every spread to `mixed`.
    // A keyed or bare value is typed once for the pointer probe and the merge below, so a value
    // holding a nested merge is scanned once per nesting level. A spread keeps the two typings
    // it always had: the probe reads its source's storage, the merge below the entry's.
    let value_types: Vec<Option<PhpType>> = entries
        .iter()
        .map(|entry| match entry {
            ArrayEntry::Spread(_) => None,
            ArrayEntry::Keyed(_, value) | ArrayEntry::Value(value) => {
                Some(array_literal_element_type_for_ir(ctx, value))
            }
        })
        .collect();
    if let Some(pointer) = pointer_literal_entries_element_type(
        ctx,
        entries.iter().zip(&value_types).map(|(entry, value_ty)| match entry {
            ArrayEntry::Spread(source) => (spread_entry_source(source), true, None),
            ArrayEntry::Keyed(_, value) | ArrayEntry::Value(value) => (value, false, value_ty.clone()),
        }),
        false,
    ) {
        return PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(pointer),
        };
    }
    let mut value_ty = PhpType::Never;
    for (entry, entry_value_ty) in entries.iter().zip(value_types) {
        let next = match (entry, entry_value_ty) {
            (ArrayEntry::Spread(inner), _) => {
                spread_storage_probe_element_type(&spread_source_storage_type_for_ir(ctx, inner))
            }
            (_, Some(ty)) => ty.codegen_repr(),
            (ArrayEntry::Keyed(_, value) | ArrayEntry::Value(value), None) => {
                array_literal_element_type_for_ir(ctx, value).codegen_repr()
            }
        };
        value_ty = crate::ir_lower::expr::assoc_array_literals::merge_ir_assoc_value_type(
            value_ty, next,
        );
    }
    if matches!(value_ty, PhpType::Never) {
        return fallback_expr_type(expr);
    }
    PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(value_ty),
    }
}

/// Lowers a single already-lowered spread operand into a hash destination, handling both
/// associative and indexed source storage. Associative sources flatten directly through
/// `__rt_hash_spread`; indexed sources are first promoted to hash storage so the same
/// reindexing path applies.
pub(super) fn lower_hash_spread_into_hash_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    hash: LoweredValue,
    source: LoweredValue,
    span: crate::span::Span,
) {
    let source_is_hash = matches!(
        ctx.builder.value_php_type(source.value).codegen_repr(),
        PhpType::AssocArray { .. }
    );
    let spread_source = if source_is_hash {
        source
    } else {
        // `Op::ArrayToHash` CONSUMES its operand: its promote path abandons the source indexed
        // array for a freshly built hash and decrefs it (`lower_array_to_hash`). A spread source
        // is usually a borrowed LOCAL, so handing it over unowned releases the caller's only
        // reference -- `[...$idx, ...$assoc]` left `$idx` reading `array(0) {}` while its
        // elements were still intact, and reusing it crashed. Acquiring first is the same ledger
        // `lower_array_set_mixed_key` documents: the promote path consumes the `+1`, and the
        // local keeps the reference it started with.
        //
        // The acquire stays unconditional; an OWNING TEMPORARY source is balanced by releasing it
        // after the spread instead (below). Skipping the acquire for a temporary is the wrong half
        // of the ledger -- the promotion's decref then consumes the only reference a BORROWED
        // local has, which segfaults -- so the two cases are separated at the release, not here.
        let owned_source = crate::ir_lower::ownership::acquire_if_refcounted(ctx, source, Some(span));
        // A pointer hash takes the promoted elements as raw pointer words: a `mixed` promotion
        // would box each word into a cell that the pointer hash then reads as the address.
        let promoted_value = match ctx.builder.value_php_type(hash.value).codegen_repr() {
            PhpType::AssocArray { value, .. } if matches!(*value, PhpType::Pointer(_)) => *value,
            _ => PhpType::Mixed,
        };
        let promoted = ctx.emit_value(
            Op::ArrayToHash,
            vec![owned_source.value],
            None,
            PhpType::AssocArray {
                key: Box::new(PhpType::Int),
                value: Box::new(promoted_value),
            },
            Op::ArrayToHash.default_effects(),
            Some(span),
        );
        LoweredValue {
            value: promoted.value,
            ir_type: IrType::Heap(IrHeapKind::Hash),
        }
    };
    ctx.emit_void(
        Op::HashSpread,
        vec![hash.value, spread_source.value],
        None,
        Op::HashSpread.default_effects(),
        Some(span),
    );
    if ctx.value_is_owning_temporary(spread_source) {
        crate::ir_lower::ownership::release_if_owned(ctx, spread_source, Some(span));
    }
    // The promoted hash is a DIFFERENT value from the one the caller handed us, so releasing it
    // above says nothing about the original array. An owning temporary source -- a call result, a
    // nested literal -- has no other owner once the promotion has consumed the reference the
    // acquire added, so it is released here, exactly as the indexed sibling releases its own
    // source. Without this, `[...f(), "k" => 1]` leaked one array per evaluation while the
    // borrowed-local form stayed clean.
    if spread_source.value != source.value && ctx.value_is_owning_temporary(source) {
        crate::ir_lower::ownership::release_if_owned(ctx, source, Some(span));
    }
}

/// Lowers an indexed-array spread by appending each source element to the destination.
pub(super) fn lower_indexed_array_spread_into_array(
    ctx: &mut LoweringContext<'_, '_>,
    array: LoweredValue,
    source: LoweredValue,
    container_elem_ty: Option<&PhpType>,
    span: crate::span::Span,
) {
    let source_elem_ty = match ctx.builder.value_php_type(source.value).codegen_repr() {
        PhpType::Array(elem_ty) => elem_ty.codegen_repr(),
        _ => PhpType::Mixed,
    };
    let len = ctx.emit_value(
        Op::ArrayLen,
        vec![source.value],
        None,
        PhpType::Int,
        Op::ArrayLen.default_effects(),
        Some(span),
    );
    let zero = emit_i64_at_span(ctx, 0, span);
    let header = ctx.builder.create_named_block("array.spread.next", vec![(IrType::I64, PhpType::Int)]);
    let body = ctx.builder.create_named_block("array.spread.body", Vec::new());
    let exit = ctx.builder.create_named_block("array.spread.exit", Vec::new());
    ctx.builder.terminate(Terminator::Br { target: header, args: vec![zero.value] });

    ctx.builder.position_at_end(header);
    let index = ctx.builder.block_param(header, 0);
    let has_next = ctx.emit_value(
        Op::ICmp,
        vec![index, len.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Slt)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: has_next.value,
        then_target: body,
        then_args: Vec::new(),
        else_target: exit,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(body);
    let value = ctx.emit_value(
        Op::ArrayGet,
        vec![source.value, index],
        None,
        source_elem_ty,
        Op::ArrayGet.default_effects(),
        Some(span),
    );
    ctx.emit_void(
        Op::ArrayPush,
        vec![array.value, value.value],
        None,
        Op::ArrayPush.default_effects(),
        Some(span),
    );
    ctx.refresh_argument_array_guard(array, span);
    crate::ir_lower::stmt::release_indexed_array_write_operand(ctx, container_elem_ty, value, span);
    let one = emit_i64_at_span(ctx, 1, span);
    let next = ctx.emit_value(
        Op::IAdd,
        vec![index, one.value],
        None,
        PhpType::Int,
        Op::IAdd.default_effects(),
        Some(span),
    );
    ctx.builder.terminate(Terminator::Br { target: header, args: vec![next.value] });

    ctx.builder.position_at_end(exit);
    if ctx.value_is_owning_temporary(source) {
        crate::ir_lower::ownership::release_if_owned(ctx, source, Some(span));
    }
}

/// Emits an integer constant at a specific source span.
pub(super) fn emit_i64_at_span(
    ctx: &mut LoweringContext<'_, '_>,
    value: i64,
    span: crate::span::Span,
) -> LoweredValue {
    ctx.emit_value(
        Op::ConstI64,
        Vec::new(),
        Some(Immediate::I64(value)),
        PhpType::Int,
        Op::ConstI64.default_effects(),
        Some(span),
    )
}

/// Returns the element type from an indexed-array literal type.
pub(super) fn indexed_array_literal_element_type(array_ty: &PhpType) -> Option<PhpType> {
    match array_ty.codegen_repr() {
        PhpType::Array(elem) => Some(elem.codegen_repr()),
        _ => None,
    }
}

/// Releases an inserted temporary when the container retained or copied its payload.
/// Callable arrays keep raw descriptor pointers today, so the inserted owner stays alive.
pub(super) fn release_value_after_retaining_insert(
    ctx: &mut LoweringContext<'_, '_>,
    container_elem_ty: Option<&PhpType>,
    value: LoweredValue,
    span: crate::span::Span,
) {
    if matches!(
        container_elem_ty.map(PhpType::codegen_repr),
        Some(PhpType::Mixed | PhpType::Callable)
    ) {
        return;
    }
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
}

/// Returns, per item, the codegen storage of a spread's source (`None` for a plain item), typed
/// once so the shape check, the pointer probe and the element merge of a literal share it.
pub(super) fn spread_sources_storage(
    ctx: &LoweringContext<'_, '_>,
    items: &[Expr],
) -> Vec<Option<PhpType>> {
    items
        .iter()
        .map(|item| match &item.kind {
            ExprKind::Spread(inner) => {
                Some(spread_source_storage_type_for_ir(ctx, inner).codegen_repr())
            }
            _ => None,
        })
        .collect()
}

/// Returns literal storage, allowing hash keys when a spread can contain associative entries.
pub(crate) fn array_literal_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    items: &[Expr],
    expr: &Expr,
) -> PhpType {
    if items.is_empty() {
        return fallback_expr_type(expr);
    }
    // Each spread source's storage is typed once and read by the shape check, the pointer probe
    // and the ordinary merge below: a source holding a nested merge (`$m ?? [...$inner, null]`)
    // would otherwise be scanned three times per level.
    let spread_storage = spread_sources_storage(ctx, items);
    if spread_storage
        .iter()
        .flatten()
        .any(|storage| matches!(storage, PhpType::AssocArray { .. } | PhpType::Mixed))
    {
        return assoc_array_literal_type_from_spreads(ctx, items, expr, &spread_storage);
    }
    // Likewise each item is typed once for the pointer probe and the ordinary merge: an item
    // holding a nested merge (`$m ?? [$inner, null]`) would otherwise be scanned twice per level.
    let item_types: Vec<PhpType> = items
        .iter()
        .zip(&spread_storage)
        .map(|(item, storage)| match storage {
            Some(storage) => spread_storage_element_type(storage),
            None => array_literal_element_type_for_ir(ctx, item),
        })
        .collect();
    let probe_types: Vec<Option<PhpType>> = item_types
        .iter()
        .zip(&spread_storage)
        .map(|(item_ty, storage)| match storage {
            Some(storage) => Some(spread_storage_probe_element_type(storage)),
            None => Some(item_ty.clone()),
        })
        .collect();
    if let Some(pointer) = pointer_literal_element_type(ctx, items, &probe_types) {
        return PhpType::Array(Box::new(pointer));
    }
    let mut item_types = item_types.into_iter();
    let mut elem_ty = item_types.next().expect("a non-empty literal has a first item");
    for item_ty in item_types {
        elem_ty = merge_ir_indexed_element_type(elem_ty, item_ty);
    }
    PhpType::Array(Box::new(elem_ty))
}

/// Returns true for an array or hash type whose elements are pointer words.
pub(crate) fn is_pointer_array_destination(destination: &PhpType) -> bool {
    literal_pointer_element_type(destination).is_some()
}

/// Returns the pointer element type of an array or hash type, if its elements are pointers.
///
/// The element is erased too, so a declared `array<ptr<T>|null>` (a union element) counts.
pub(super) fn literal_pointer_element_type(literal_ty: &PhpType) -> Option<PhpType> {
    let elem = match literal_ty.codegen_repr() {
        PhpType::Array(elem) => elem.codegen_repr(),
        PhpType::AssocArray { value, .. } => value.codegen_repr(),
        _ => return None,
    };
    matches!(elem, PhpType::Pointer(_)).then_some(elem)
}

/// Returns the raw-word element type of an array or hash type: a pointer word, as
/// `literal_pointer_element_type` answers, or a buffer handle.
///
/// Branch merges use it to keep an array of buffers as word storage across a ternary, `?:` or
/// `match`; the pointer-word conversions keep asking `literal_pointer_element_type`, since a
/// buffer handle never comes boxed from a null literal.
pub(super) fn literal_raw_word_element_type(literal_ty: &PhpType) -> Option<PhpType> {
    let elem = match literal_ty.codegen_repr() {
        PhpType::Array(elem) => elem.codegen_repr(),
        PhpType::AssocArray { value, .. } => value.codegen_repr(),
        _ => return None,
    };
    matches!(elem, PhpType::Pointer(_) | PhpType::Buffer(_)).then_some(elem)
}

/// Rewrites a packed record address or `null` stored into a pointer-element literal into the word.
fn retype_pointer_literal_value(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    pointer_elem: Option<&PhpType>,
    span: crate::span::Span,
) -> LoweredValue {
    match pointer_elem {
        Some(elem) => ctx.retype_packed_address_for_pointer_sink(value, elem, Some(span)),
        None => value,
    }
}

/// Returns true for an array or hash literal with no items.
///
/// A branch merge treats `[]` as a shape-only branch of an array of buffers: it adds no element
/// and is lowered against the merged buffer-array type.
pub(super) fn is_empty_literal(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => items.is_empty(),
        ExprKind::ArrayLiteralAssoc(pairs) => pairs.is_empty(),
        _ => false,
    }
}

/// Returns true for an array literal whose every element (or value) is a literal `null`.
pub(super) fn is_null_only_literal(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => {
            !items.is_empty() && items.iter().all(|item| matches!(item.kind, ExprKind::Null))
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            !pairs.is_empty() && pairs.iter().all(|(_, value)| matches!(value.kind, ExprKind::Null))
        }
        _ => false,
    }
}

/// Returns the source of a mixed-literal spread entry, which holds the `ExprKind::Spread` node.
fn spread_entry_source(entry: &Expr) -> &Expr {
    match &entry.kind {
        ExprKind::Spread(source) => source,
        _ => entry,
    }
}

/// Lowers a spread source, giving a literal of only `null`s pointer storage when the enclosing
/// literal holds pointers, so its elements are null pointer words rather than boxed nulls.
fn lower_spread_source(
    ctx: &mut LoweringContext<'_, '_>,
    source: &Expr,
    pointer_elem: Option<&PhpType>,
) -> LoweredValue {
    let literal = spread_entry_source(source);
    let Some(elem) = pointer_elem.filter(|_| is_null_only_literal(literal)) else {
        return lower_expr(ctx, source);
    };
    let source = literal;
    match &source.kind {
        ExprKind::ArrayLiteral(items) => {
            lower_array_literal_without_spread(ctx, items, source, PhpType::Array(Box::new(elem.clone())))
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            let hash_ty = match assoc_array_literal_type_for_ir(ctx, pairs, source) {
                PhpType::AssocArray { key, .. } => PhpType::AssocArray { key, value: Box::new(elem.clone()) },
                other => other,
            };
            super::assoc_array_literals::lower_assoc_array_literal_as(ctx, pairs, source, false, hash_ty)
        }
        _ => lower_expr(ctx, source),
    }
}

/// Returns the pointer element type of literal values that are pointers and `null`s, if any.
///
/// A `null` beside pointers is stored as the null pointer word, and a pointer element read is
/// `?ptr<T>`, so `[$p, null]` keeps raw pointer storage instead of widening every element to a
/// boxed `mixed` (where `$a[0]->x` could no longer reach the record). Literals without a `null`
/// return `None` and keep the ordinary merge.
///
/// `value_types` holds, per value and in order, the type the caller already gave it (`Some`:
/// `array_literal_element_type_for_ir` for a plain value, `spread_storage_probe_element_type` of
/// the source storage for a spread), so the value is not typed again here; a `None` is typed by
/// the probe itself.
pub(super) fn pointer_literal_element_type<'e>(
    ctx: &LoweringContext<'_, '_>,
    values: impl IntoIterator<Item = &'e Expr>,
    value_types: &[Option<PhpType>],
) -> Option<PhpType> {
    pointer_literal_entries_element_type(
        ctx,
        values.into_iter().zip(value_types).map(|(value, ty)| match &value.kind {
            ExprKind::Spread(source) => (source.as_ref(), true, ty.clone()),
            _ => (value, false, ty.clone()),
        }),
        true,
    )
}

/// Returns the pointer element type of literal entries given as `(value, is_spread)` pairs.
///
/// A spread contributes its source's element type, and a spread of a literal holding only
/// `null`s contributes a `null`, as the type checker's literal merge does. With `require_null`
/// a literal without any `null` returns `None`, leaving it to the caller's ordinary merge.
///
/// Callers merge the same entries again when this returns `None`, so an entry comes with the
/// type they already gave it (`Some`) and is not typed twice; the `null` check and a nested
/// literal value (a container, never a pointer) are decided syntactically. Both keep deeply
/// nested literals and merges linear per level. An entry without a type (a spread) is typed here.
pub(super) fn pointer_literal_entries_element_type<'e>(
    ctx: &LoweringContext<'_, '_>,
    entries: impl IntoIterator<Item = (&'e Expr, bool, Option<PhpType>)>,
    require_null: bool,
) -> Option<PhpType> {
    let entries: Vec<(&'e Expr, bool, Option<PhpType>)> = entries.into_iter().collect();
    let is_null_entry = |value: &Expr, is_spread: bool| {
        (!is_spread && matches!(value.kind, ExprKind::Null))
            || (is_spread && is_null_only_literal(value))
    };
    if require_null && !entries.iter().any(|(value, is_spread, _)| is_null_entry(value, *is_spread)) {
        return None;
    }
    let mut has_null = false;
    let mut pointer: Option<PhpType> = None;
    for (value, is_spread, value_ty) in entries {
        if is_null_entry(value, is_spread) {
            has_null = true;
            continue;
        }
        if !is_spread && matches!(value.kind, ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_)) {
            return None;
        }
        let elem_ty = match value_ty {
            Some(ty) => ty,
            None if is_spread => {
                spread_storage_probe_element_type(&spread_source_storage_type_for_ir(ctx, value))
            }
            None => array_literal_element_type_for_ir(ctx, value),
        };
        if !matches!(elem_ty, PhpType::Pointer(_)) {
            return None;
        }
        pointer = Some(match pointer {
            Some(merged) => merge_ir_indexed_element_type(merged, elem_ty),
            None => elem_ty,
        });
    }
    pointer.filter(|merged| (has_null || !require_null) && matches!(merged, PhpType::Pointer(_)))
}

/// Returns the EIR storage type of an `(array)` cast used as a literal element.
///
/// Classification goes through `ArrayCast`, using the source type `lower_array_cast` reads
/// off the lowered inner value. A syntactic `Int` guess stamps the literal `array<int>` and
/// either stores a property hash in that slot or refuses to compile the associative form.
pub(super) fn array_cast_literal_element_type(
    ctx: &LoweringContext<'_, '_>,
    inner: &Expr,
) -> PhpType {
    ir_array_storage_type(crate::types::array_cast_result_type(
        &super::array_cast_source::array_cast_source_type_for_ir(ctx, inner),
    ))
}

/// Returns the best EIR storage element type for one indexed-array literal item.
pub(super) fn array_literal_element_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    item: &Expr,
) -> PhpType {
    if let Some(storage) = nullsafe_chain::result_storage_type(item) {
        return storage;
    }
    match &item.kind {
        // A reference entry's payload is a boxed Mixed cell owned by the managed reference, so
        // the hash storage stays dynamic regardless of the aliased place's declared type.
        ExprKind::Reference(_) => PhpType::Mixed,
        ExprKind::Null | ExprKind::Throw(_) => PhpType::Mixed,
        ExprKind::Cast {
            target: CastType::Array,
            expr: inner,
        } => array_cast_literal_element_type(ctx, inner),
        ExprKind::Ternary { .. }
        | ExprKind::ShortTernary { .. }
        | ExprKind::Match { .. }
        | ExprKind::NullCoalesce { .. }
        | ExprKind::Assignment { .. } => {
            // Keep the element stamp and ownership consistent with the lowered merge temp: the
            // syntactic fallback widens an int/bool pair to one scalar the boxed temp is cast to.
            ir_array_storage_type(materialized_expr_type_for_merge(ctx, item))
        }
        ExprKind::Spread(inner) => {
            spread_storage_element_type(&spread_source_storage_type_for_ir(ctx, inner).codegen_repr())
        }
        ExprKind::ArrayLiteral(items) => array_literal_type_for_ir(ctx, items, item).codegen_repr(),
        ExprKind::ArrayLiteralAssoc(pairs) => assoc_array_literal_type_for_ir(ctx, pairs, item),
        ExprKind::ConstRef(name) => ctx
            .constant_value(name.as_str())
            .map(|(_, ty)| ir_array_storage_type(ty))
            .unwrap_or_else(|| ir_array_storage_type(infer_expr_type_syntactic(item))),
        ExprKind::ScopedConstantAccess { receiver, name } => {
            scoped_constant_value_type_for_ir(ctx, receiver, name, item)
        }
        ExprKind::Variable(name) => ir_array_storage_type(
            ctx.local_types
                .get(name)
                .cloned()
                .unwrap_or_else(|| infer_expr_type_syntactic(item)),
        ),
        ExprKind::FunctionCall { name, .. } => {
            let canonical = name.as_str();
            if let Some(sig) = ctx.functions.get(canonical) {
                return ir_array_storage_type(eir_user_function_return_type(sig));
            }
            if let Some(sig) = ctx.extern_functions.get(canonical) {
                return ir_array_storage_type(sig.return_type.clone());
            }
            // A builtin is in neither map, and the syntactic fallback below answers `Int` for
            // every name outside its hand-written allowlist -- which stamps the literal
            // `array<int>` and makes lowering cast the element to match (#1096). The checker
            // already asked the registry contract about this very call.
            if let Some(resolved) = builtin_call_result_type_for_ir(ctx, item.span) {
                return resolved;
            }
            ir_array_storage_type(infer_expr_type_syntactic(item))
        }
        // Calls must use declared EIR return metadata rather than the syntactic `Int` fallback,
        // or an object result is cast into an incorrectly stamped scalar array.
        ExprKind::MethodCall { object, method, .. } => {
            method_call_expr_type_for_ir(ctx, object, method)
                .and_then(materializable_array_element_type)
                .unwrap_or_else(|| ir_array_storage_type(infer_expr_type_syntactic(item)))
        }
        ExprKind::StaticMethodCall { receiver, method, .. } => {
            static_method_call_expr_type_for_ir(ctx, receiver, method)
                .and_then(materializable_array_element_type)
                .unwrap_or_else(|| ir_array_storage_type(infer_expr_type_syntactic(item)))
        }
        ExprKind::ArrayAccess { array, .. } => array_access_expr_value_type_for_ir(ctx, array)
            .unwrap_or_else(|| ir_array_storage_type(infer_expr_type_syntactic(item))),
        ExprKind::PropertyAccess { object, property } => property_access_expr_type_for_ir(
            ctx,
            object,
            property,
        )
        .unwrap_or_else(|| ir_array_storage_type(infer_expr_type_syntactic(item))),
        // A static buffer or pointer slot holds a raw word the syntactic `int` would cast.
        ExprKind::StaticPropertyAccess { receiver, property } => {
            static_property_word_type(ctx, receiver, property, item)
                .map(ir_array_storage_type)
                .unwrap_or_else(|| ir_array_storage_type(infer_expr_type_syntactic(item)))
        }
        _ => ir_array_storage_type(infer_expr_type_syntactic(item)),
    }
}

/// Returns the element an indexed literal takes from a spread whose source has `storage`.
///
/// A spread of an empty/unknown array (`array<never>`, e.g. a `$x = []` local or a
/// bare-`array`-returning method) contributes no element constraint, so its Void/Never element
/// widens to Mixed rather than collapsing the outer literal to `array<never>` (which would
/// normalize to a `Void` element and emit an unsupported `array_push for PHP type Void`, as in
/// `[...$acc, ...$this->more()]`).
fn spread_storage_element_type(storage: &PhpType) -> PhpType {
    match storage.codegen_repr() {
        PhpType::Array(elem) => match elem.codegen_repr() {
            PhpType::Void | PhpType::Never => PhpType::Mixed,
            other => other,
        },
        PhpType::AssocArray { value, .. } => value.codegen_repr(),
        _ => PhpType::Mixed,
    }
}

/// Returns the element the pointer probe and the hash merges take from a spread whose source
/// has `storage`: the container's own element, unwidened, or Mixed for anything else.
pub(super) fn spread_storage_probe_element_type(storage: &PhpType) -> PhpType {
    match storage.codegen_repr() {
        PhpType::Array(elem) => elem.codegen_repr(),
        PhpType::AssocArray { value, .. } => value.codegen_repr(),
        _ => PhpType::Mixed,
    }
}

/// Returns the full storage type of a spread source, rather than the element type contributed
/// by a call when it appears as an ordinary literal value.
///
/// Desugared argument-introspection calls can synthesize `array_slice()` nodes that share the
/// source call's span. The builtin element-type helper therefore sees the checker's type for
/// `func_get_args()` at that span and mistakes the spread container for an element. Resolve the
/// builtin's own contract without a checker span here so static spread classification agrees
/// with the runtime value lowered below.
/// Shared by literal and call-argument unpacking; ordinary elements retain their logical type.
pub(super) fn spread_source_storage_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    source: &Expr,
) -> PhpType {
    if let ExprKind::ErrorSuppress(inner) = &source.kind {
        return spread_source_storage_type_for_ir(ctx, inner);
    }
    if let ExprKind::FunctionCall { name, args } = &source.kind {
        let canonical = name.as_str();
        if let Some(sig) = ctx.functions.get(canonical) {
            return ir_array_storage_type(eir_user_function_return_type(sig));
        }
        if let Some(sig) = ctx.extern_functions.get(canonical) {
            return ir_array_storage_type(sig.return_type.clone());
        }
        if let Some(result) = super::function_calls::resolve_registry_builtin_result_type(
            ctx,
            canonical,
            args,
            &[],
            Span::dummy(),
            None,
        ) {
            return ir_array_storage_type(result);
        }
    }
    let source_ty = array_literal_element_type_for_ir(ctx, source);
    if matches!(source.kind, ExprKind::ArrayAccess { .. }) {
        // A typed container read can miss, but that logical null does not change its native
        // storage into a hash. Unpacking still checks the actual value and rejects the miss.
        // Keep ordinary literal elements nullable so a missing object read remains PHP null.
        let non_null = strip_void_from_union(source_ty.clone());
        if matches!(non_null.codegen_repr(), PhpType::Array(_) | PhpType::AssocArray { .. }) {
            return non_null;
        }
    }
    source_ty
}

/// Returns the EIR array storage type for a resolved element type, or `None` when the type
/// cannot be an array element. A `Void`/`Never` method return (a value-less call whose result
/// is nonetheless collected into a literal) has no array-element representation — stamping it
/// would emit an unsupported `array_push for PHP type Void` — so the caller keeps its syntactic
/// fallback for that degenerate case, exactly as before this arm existed (no regression).
pub(super) fn materializable_array_element_type(return_type: PhpType) -> Option<PhpType> {
    let stored = ir_array_storage_type(return_type);
    match stored.codegen_repr() {
        PhpType::Void | PhpType::Never => None,
        _ => Some(stored),
    }
}

/// Returns the array-element storage type for a builtin call the checker already resolved.
///
/// `infer_expr_type_syntactic` answers a builtin call from a hand-written allowlist of names and
/// falls back to `Int` for every name it does not list, so an array-returning or bool-returning
/// builtin outside that list stamped its literal `array<int>` and lowering converted the element
/// to match: `(int)` of a non-empty array is `1`, and so is `(int)true` (issue #1096).
/// A user function is looked up in `ctx.functions` before this point and a builtin is in neither
/// that map nor `ctx.extern_functions`, which is why only builtins ever reached the fallback.
///
/// The key is the call's OWN span, which is the node lowering is looking at, so unlike
/// `call_user_func()` there is no way for a different callee to be recorded under it. A
/// synthesized node (line 0) is left to the fallback rather than risk sharing a key with an
/// unrelated prelude call.
///
/// # Why a non-scalar result is stamped `Mixed` rather than taken verbatim
///
/// This map holds the CHECKER's type. The value the call actually produces is typed by
/// `resolve_registry_builtin_result_type`, which overrides the checker for a `Declared` or
/// `Shared` result contract and re-derives its own for a `Checked` one whose runtime target
/// rejects the checked type against the operands it was really given. The two disagree, and not
/// rarely: `array_map()`'s hook answers `array<int>` for a callback declared `: int` while its
/// EIR result is a boxed `Mixed` cell.
///
/// A stamp that disagrees is only harmless when the push CONVERTS, and
/// `coerce_array_literal_element_to_storage_type` converts exactly the scalar stamps —
/// `Int`, `Bool`, `Float`, `Str`. A heap stamp is stored as-is, so an `array<array<int>>` stamp
/// over a boxed `Mixed` value reads the box's header as an array: `[array_map(fn(int $x): int =>
/// $x * 2, [1, 2, 3])][0][1]` printed a pointer.
///
/// So a scalar result is taken precisely and anything else is stamped `Mixed`, which is the one
/// element type that is right whatever the call's own contract answers: a concrete value is boxed
/// into it on the way in, and a `Mixed` value is stored as-is. It is also what a literal mixing a
/// builtin call with a plain element already merged to, which is why `[array_slice(...), [9, 9]]`
/// was correct all along while `[array_slice(...)]` alone was not.
pub(super) fn builtin_call_result_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    span: Span,
) -> Option<PhpType> {
    if span.line == 0 {
        return None;
    }
    let checked = ctx.builtin_call_types.get(&span).cloned()?;
    // A pointer result (`ptr_null()`, or `array_pop` of pointers, unboxed to its word after the
    // call) is stored as the pointer word, not boxed into a `mixed` slot; a buffer handle from
    // `array_pop` of buffers is stored as its word the same way.
    if crate::types::is_nullable_pointer_type(&checked)
        || matches!(checked, PhpType::Pointer(_) | PhpType::Buffer(_))
    {
        return Some(checked.codegen_repr());
    }
    match materializable_array_element_type(checked)? {
        scalar @ (PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Str) => Some(scalar),
        _ => Some(PhpType::Mixed),
    }
}

/// Returns the EIR array storage metadata type, preserving PHP resources.
pub(crate) fn ir_array_storage_type(php_type: PhpType) -> PhpType {
    let php_type = normalize_value_php_type(php_type);
    if matches!(php_type, PhpType::Resource(_)) {
        php_type
    } else {
        php_type.codegen_repr()
    }
}

/// Merges indexed-array element types for EIR storage metadata.
pub(crate) fn merge_ir_indexed_element_type(left: PhpType, right: PhpType) -> PhpType {
    ir_array_storage_type(PhpType::widen_array_branch_element(left, right))
}
