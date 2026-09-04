//! Purpose:
//! Indexed array literal and spread lowering.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

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
    // Spread-containing literals: lower every item value in source order first so PHP-visible side
    // effects happen in order, then inspect each spread source's actual IR type to decide whether
    // the destination must be associative (hash) storage. Dest allocation is pure, so emitting it
    // after source evaluation preserves observable behavior.
    let mut lowered: Vec<SpreadItem> = Vec::with_capacity(items.len());
    let mut any_assoc_spread = false;
    for item in items {
        match &item.kind {
            ExprKind::Spread(inner) => {
                let source = lower_expr(ctx, inner);
                if matches!(
                    ctx.builder.value_php_type(source.value).codegen_repr(),
                    PhpType::AssocArray { .. }
                ) {
                    any_assoc_spread = true;
                }
                lowered.push(SpreadItem::Spread(source));
            }
            _ => {
                let value = lower_expr(ctx, item);
                lowered.push(SpreadItem::Element(value));
            }
        }
    }
    if any_assoc_spread {
        lower_array_literal_as_hash_from_lowered(ctx, items, &lowered, expr)
    } else {
        lower_array_literal_as_indexed_from_lowered(ctx, items, &lowered, expr)
    }
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
    let array = ctx.emit_value(
        Op::ArrayNew,
        Vec::new(),
        Some(Immediate::Capacity(items.len() as u32)),
        array_ty,
        Op::ArrayNew.default_effects(),
        Some(expr.span),
    );
    for item in items {
        let value = lower_expr(ctx, item);
        let value = coerce_array_literal_element_to_storage_type(ctx, value, elem_ty.as_ref(), item);
        ctx.emit_void(
            Op::ArrayPush,
            vec![array.value, value.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(item.span),
        );
        crate::ir_lower::stmt::release_indexed_array_write_operand(ctx, elem_ty.as_ref(), value, item.span);
    }
    array
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
        _ => value,
    };
    // The scalar coercers release owning heap-repr sources internally (see
    // `release_coerced_source_if_owned`); releasing those here again would
    // double-free the element box. This caller-side release only covers the
    // remaining reprs (e.g. an owned string temp narrowed through `StrToI`).
    if coerced.value != value.value
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
    let hash_ty = assoc_array_literal_type_from_spreads(ctx, items, expr);
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
        let promoted = ctx.emit_value(
            Op::ArrayToHash,
            vec![source.value],
            None,
            PhpType::AssocArray {
                key: Box::new(PhpType::Int),
                value: Box::new(PhpType::Mixed),
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

/// Returns the indexed-array type that the EIR backend can faithfully materialize.
pub(crate) fn array_literal_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    items: &[Expr],
    expr: &Expr,
) -> PhpType {
    if items.is_empty() {
        return fallback_expr_type(expr);
    }
    let mut elem_ty = array_literal_element_type_for_ir(ctx, &items[0]);
    for item in items.iter().skip(1) {
        elem_ty = merge_ir_indexed_element_type(
            elem_ty,
            array_literal_element_type_for_ir(ctx, item),
        );
    }
    PhpType::Array(Box::new(elem_ty))
}

/// Returns the best EIR storage element type for one indexed-array literal item.
pub(super) fn array_literal_element_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    item: &Expr,
) -> PhpType {
    match &item.kind {
        ExprKind::Null => PhpType::Mixed,
        ExprKind::Spread(inner) => match array_literal_element_type_for_ir(ctx, inner).codegen_repr() {
            // A spread of an empty/unknown array (`array<never>`, e.g. a `$x = []` local or a
            // bare-`array`-returning method) contributes no element constraint, so widen its
            // Void/Never element to Mixed rather than collapsing the outer literal to
            // `array<never>` — which would normalize to a `Void` element and emit an unsupported
            // `array_push for PHP type Void` (`[...$acc, ...$this->more()]`).
            PhpType::Array(elem) => match elem.codegen_repr() {
                PhpType::Void | PhpType::Never => PhpType::Mixed,
                other => other,
            },
            _ => PhpType::Mixed,
        },
        ExprKind::ArrayLiteral(items) => array_literal_type_for_ir(ctx, items, item).codegen_repr(),
        ExprKind::ArrayLiteralAssoc(pairs) => assoc_array_literal_type_for_ir(ctx, pairs, item),
        ExprKind::ConstRef(name) => ctx
            .constant_value(name.as_str())
            .map(|(_, ty)| ir_array_storage_type(ty))
            .unwrap_or_else(|| ir_array_storage_type(infer_expr_type_syntactic(item))),
        // A name the lowering has seen no store for is an UNDEFINED read: php answers null with
        // a warning, and a null element lives in a Mixed cell — exactly what `[null]` above
        // produces. The syntactic fallback answers `Int` for any expression it does not
        // recognise, which stamped `[$undefined]` as `array<int>` and then refused the whole
        // program with `unsupported EIR backend feature: array_push for PHP type Void`.
        ExprKind::Variable(name) => match ctx.local_types.get(name).cloned() {
            Some(ty) => ir_array_storage_type(ty),
            None => PhpType::Mixed,
        },
        ExprKind::FunctionCall { name, .. } => {
            let canonical = name.as_str();
            if let Some(sig) = ctx.functions.get(canonical) {
                return ir_array_storage_type(sig.return_type.clone());
            }
            if let Some(sig) = ctx.extern_functions.get(canonical) {
                return ir_array_storage_type(sig.return_type.clone());
            }
            // A BUILTIN is neither of those, and the syntactic fallback below cannot know one:
            // it answered `Str` for `json_decode()`, so `[json_decode("1")]` became
            // `array<string>` and `gettype()` reported the DECLARATION rather than the value.
            // The checker already decided this call's type and keyed it by span.
            if let Some(ty) = ctx.builtin_call_types.get(&item.span) {
                return ir_array_storage_type(ty.clone());
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
        ExprKind::NullsafeMethodCall { object, method, .. } => {
            nullsafe_method_call_expr_type_for_ir(ctx, object, method)
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
        _ => ir_array_storage_type(infer_expr_type_syntactic(item)),
    }
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

