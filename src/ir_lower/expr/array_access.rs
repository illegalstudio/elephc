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
    if let Some((class_name, method, coerce_index, may_be_false)) =
        dom_collection_dimension_method(ctx, array, index)
    {
        if may_be_false {
            let receiver = if warn_on_missing {
                lower_expr(ctx, array)
            } else {
                lower_subscript_receiver_silently(ctx, array)
            };
            return lower_dom_collection_dimension_with_false(
                ctx,
                receiver,
                &class_name,
                method,
                index,
                expr,
                warn_on_missing,
                coerce_index,
            );
        }
        let synthetic = Expr::new(
            ExprKind::MethodCall {
                object: Box::new(array.clone()),
                method: method.to_string(),
                args: vec![dom_collection_dimension_argument(index, coerce_index)],
            },
            expr.span,
        );
        return lower_expr(ctx, &synthetic);
    }
    let array_value = if warn_on_missing {
        lower_expr(ctx, array)
    } else {
        lower_subscript_receiver_silently(ctx, array)
    };
    let array_type = ctx.builder.value_php_type(array_value.value);
    if crate::ir_lower::internal_extensions::simplexml_object_handler_opcode_for_type(
        ctx,
        &array_type,
        "read_dimension",
    )
    .is_some()
    {
        if value_is_nullable(ctx, array_value.value) {
            return lower_nullable_simplexml_dimension_read(ctx, array_value, index, expr, 0);
        }
        return lower_simplexml_dimension_read_from_value(ctx, array_value, index, expr, 0);
    }
    if value_is_nullable(ctx, array_value.value) {
        return lower_nullable_array_access(ctx, array_value, index, expr, warn_on_missing);
    }
    lower_array_access_from_value(ctx, array_value, index, expr, warn_on_missing)
}

/// Selects php-src's DOM collection lookup method for one dimension read.
fn dom_collection_dimension_method(
    ctx: &LoweringContext<'_, '_>,
    array: &Expr,
    index: &Expr,
) -> Option<(String, &'static str, bool, bool)> {
    let receiver_type = match &array.kind {
        ExprKind::Variable(name) => ctx
            .local_types
            .get(name)
            .cloned()
            .unwrap_or_else(|| infer_expr_type_syntactic(array)),
        ExprKind::PropertyAccess { object, property } => {
            property_access_expr_type_for_ir(ctx, object, property)
                .unwrap_or_else(|| infer_expr_type_syntactic(array))
        }
        ExprKind::NullsafePropertyAccess { object, property } => {
            nullsafe_property_access_expr_type_for_ir(ctx, object, property)
                .unwrap_or_else(|| infer_expr_type_syntactic(array))
        }
        ExprKind::MethodCall { object, method, .. } => {
            method_call_expr_type_for_ir(ctx, object, method)
                .unwrap_or_else(|| infer_expr_type_syntactic(array))
        }
        ExprKind::NullsafeMethodCall { object, method, .. } => {
            nullsafe_method_call_expr_type_for_ir(ctx, object, method)
                .unwrap_or_else(|| infer_expr_type_syntactic(array))
        }
        _ => infer_expr_type_syntactic(array),
    };
    let (class_name, may_be_false) = dom_collection_class_and_failure(&receiver_type)?;
    let numeric = index_expr_key_type(ctx, index) == PhpType::Int;
    let method = match class_name.trim_start_matches('\\') {
        "DOMNodeList" | "Dom\\NodeList" => "item",
        "Dom\\HTMLCollection" if numeric => "item",
        "Dom\\HTMLCollection" => "namedItem",
        "DOMNamedNodeMap" | "Dom\\NamedNodeMap" | "Dom\\DtdNamedNodeMap" if numeric => "item",
        "DOMNamedNodeMap" | "Dom\\NamedNodeMap" | "Dom\\DtdNamedNodeMap" => "getNamedItem",
        _ => return None,
    };
    Some((
        class_name,
        method,
        method == "item" && index_expr_key_type(ctx, index) != PhpType::Int,
        may_be_false,
    ))
}

/// Returns one DOM collection class and whether its result may be the legacy `false` sentinel.
fn dom_collection_class_and_failure(ty: &PhpType) -> Option<(String, bool)> {
    const DOM_COLLECTIONS: &[&str] = &[
        "DOMNodeList",
        "Dom\\NodeList",
        "Dom\\HTMLCollection",
        "DOMNamedNodeMap",
        "Dom\\NamedNodeMap",
        "Dom\\DtdNamedNodeMap",
    ];

    match ty {
        PhpType::Object(class_name)
            if DOM_COLLECTIONS.contains(&class_name.trim_start_matches('\\')) =>
        {
            Some((class_name.clone(), false))
        }
        PhpType::Union(members) => {
            let mut class_name = None;
            let mut may_be_false = false;
            for member in members {
                match member {
                    PhpType::Object(name)
                        if DOM_COLLECTIONS.contains(&name.trim_start_matches('\\')) =>
                    {
                        if class_name
                            .as_deref()
                            .is_some_and(|existing| existing != name.as_str())
                        {
                            return None;
                        }
                        class_name = Some(name.clone());
                    }
                    PhpType::False => may_be_false = true,
                    _ => return None,
                }
            }
            class_name.map(|name| (name, may_be_false))
        }
        _ => None,
    }
}

/// Lowers a legacy DOM collection dimension while preserving its `false` fallback semantics.
fn lower_dom_collection_dimension_with_false(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: LoweredValue,
    class_name: &str,
    method: &str,
    index: &Expr,
    expr: &Expr,
    warn_on_missing: bool,
    coerce_index: bool,
) -> LoweredValue {
    let result_type = dom_collection_method_result_type(ctx, class_name, method)
        .unwrap_or_else(|| fallback_expr_type(expr));
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
    let false_value = emit_bool_literal(ctx, false, Some(expr.span));
    let is_false = ctx.emit_value(
        Op::StrictEq,
        vec![receiver.value, false_value.value],
        None,
        PhpType::Bool,
        Op::StrictEq.default_effects(),
        Some(expr.span),
    );
    let false_block = ctx
        .builder
        .create_named_block("dom.collection.dimension.false", Vec::new());
    let object_block = ctx
        .builder
        .create_named_block("dom.collection.dimension.object", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("dom.collection.dimension.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_false.value,
        then_target: false_block,
        then_args: Vec::new(),
        else_target: object_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(false_block);
    let fallback = lower_array_access_from_value(ctx, receiver, index, expr, warn_on_missing);
    store_value_into_temp(ctx, &temp_name, result_type.clone(), fallback, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(object_block);
    let item = lower_dom_collection_method_from_value(
        ctx,
        receiver,
        class_name,
        method,
        index,
        expr,
        coerce_index,
        result_type.clone(),
    );
    store_value_into_temp(ctx, &temp_name, result_type, item, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Returns the checked result type of one DOM collection lookup method.
fn dom_collection_method_result_type(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    method: &str,
) -> Option<PhpType> {
    class_method_signature(ctx, class_name, &php_symbol_key(method))
        .map(|signature| normalize_value_php_type(signature.return_type.clone()))
}

/// Lowers a typed DOM collection lookup from an already-evaluated object receiver.
fn lower_dom_collection_method_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: LoweredValue,
    class_name: &str,
    method: &str,
    index: &Expr,
    expr: &Expr,
    coerce_index: bool,
    result_type: PhpType,
) -> LoweredValue {
    let opcode = crate::ir_lower::internal_extensions::method_opcode(ctx, class_name, method)
        .expect("DOM collection dimensions require a registered native method");
    let signature = class_method_signature(ctx, class_name, &php_symbol_key(method)).cloned();
    let argument = dom_collection_dimension_argument(index, coerce_index);
    let arguments = lower_internal_extension_args(ctx, signature.as_ref(), &[argument], false);
    let mut operands = Vec::with_capacity(arguments.len() + 1);
    operands.push(receiver.value);
    operands.extend(arguments.iter().copied());
    let result = crate::ir_lower::internal_extensions::emit_call(
        ctx,
        opcode,
        crate::ir_lower::internal_extensions::FLAG_RECEIVER
            | internal_extension_result_flags(&result_type),
        operands,
        result_type,
        expr.span,
    );
    release_owned_call_arg_temporaries_with_signature(
        ctx,
        &arguments,
        Some(result.value),
        &ReturnArgAlias::Unknown,
        signature.as_ref(),
        expr.span,
    );
    release_owning_receiver_temporary(ctx, receiver, expr.span);
    result
}

/// Builds a synthetic DOM collection lookup argument without duplicating source evaluation.
fn dom_collection_dimension_argument(index: &Expr, coerce_index: bool) -> Expr {
    if !coerce_index {
        return index.clone();
    }
    Expr::new(
        ExprKind::Cast {
            target: CastType::Int,
            expr: Box::new(index.clone()),
        },
        index.span,
    )
}

/// Returns the concrete DOM named-map class carried by `ty`, when known.
///
/// The special dimension handler is deliberately limited to concrete classes:
/// a mixed receiver must continue through generic PHP dispatch rather than
/// inventing an error message for a runtime class it does not know.
pub(crate) fn dom_named_node_map_class(ty: &PhpType) -> Option<String> {
    match ty {
        PhpType::Object(class_name)
            if matches!(
                class_name.trim_start_matches('\\'),
                "DOMNamedNodeMap" | "Dom\\NamedNodeMap" | "Dom\\DtdNamedNodeMap"
            ) => Some(class_name.trim_start_matches('\\').to_string()),
        _ => None,
    }
}

/// Returns the concrete DOM named-map class and whether the receiver may be null.
///
/// The declaration registry exposes DTD map properties as `Map|null`, even
/// though attached document types normally return a map. Writes therefore need
/// a runtime null branch: PHP autovivifies null but rejects the live map.
pub(crate) fn dom_named_node_map_receiver(ty: &PhpType) -> Option<(String, bool)> {
    if let Some(class_name) = dom_named_node_map_class(ty) {
        return Some((class_name, false));
    }
    let PhpType::Union(members) = ty else {
        return None;
    };
    let mut map_class = None;
    let mut saw_null = false;
    for member in members {
        match member {
            PhpType::Void => saw_null = true,
            member => {
                let class_name = dom_named_node_map_class(member)?;
                if map_class.replace(class_name).is_some() {
                    return None;
                }
            }
        }
    }
    saw_null.then_some((map_class?, true))
}

/// Returns the known DOM named-map class for a subscript receiver expression.
pub(crate) fn dom_named_node_map_dimension_class(
    ctx: &LoweringContext<'_, '_>,
    array: &Expr,
) -> Option<String> {
    let receiver_type = match &array.kind {
        ExprKind::Variable(name) => ctx
            .local_types
            .get(name)
            .cloned()
            .unwrap_or_else(|| infer_expr_type_syntactic(array)),
        ExprKind::PropertyAccess { object, property } => {
            property_access_expr_type_for_ir(ctx, object, property)
                .unwrap_or_else(|| infer_expr_type_syntactic(array))
        }
        _ => infer_expr_type_syntactic(array),
    };
    dom_named_node_map_class(&receiver_type)
}

/// Returns the DOM named-map class plus nullability for a subscript receiver.
pub(crate) fn dom_named_node_map_dimension_receiver(
    ctx: &LoweringContext<'_, '_>,
    array: &Expr,
) -> Option<(String, bool)> {
    let receiver_type = match &array.kind {
        ExprKind::Variable(name) => ctx
            .local_types
            .get(name)
            .cloned()
            .unwrap_or_else(|| infer_expr_type_syntactic(array)),
        ExprKind::PropertyAccess { object, property } => {
            property_access_expr_type_for_ir(ctx, object, property)
                .unwrap_or_else(|| infer_expr_type_syntactic(array))
        }
        _ => infer_expr_type_syntactic(array),
    };
    dom_named_node_map_receiver(&receiver_type)
}

/// Throws PHP's read-only-DOM-map `Error` without exposing a fake ArrayAccess API.
///
/// php-src performs this check at the dimension operation itself. Building the
/// ordinary `Error` object preserves catchability, cleanup, and target-neutral
/// exception lowering across every supported backend.
pub(crate) fn lower_dom_named_node_map_dimension_error(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    span: Span,
) {
    let error = Expr::new(
        ExprKind::NewObject {
            class_name: Name::unqualified("Error"),
            args: vec![Expr::new(
                ExprKind::StringLiteral(format!(
                    "Cannot use object of type {class_name} as array"
                )),
                span,
            )],
        },
        span,
    );
    let throwing = Expr::new(ExprKind::Throw(Box::new(error)), span);
    lower_expr(ctx, &throwing);
}

/// Lowers a SimpleXML dimension read after its receiver is evaluated exactly once.
pub(crate) fn lower_simplexml_dimension_read_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: LoweredValue,
    index: &Expr,
    expr: &Expr,
    access_mode: i64,
) -> LoweredValue {
    let receiver_type = ctx.builder.value_php_type(receiver.value);
    let opcode = crate::ir_lower::internal_extensions::simplexml_object_handler_opcode_for_type(
        ctx,
        &receiver_type,
        "read_dimension",
    )
    .expect("SimpleXML dimension lowering requires the locked read handler");
    let wrapper_type = crate::ir_lower::internal_extensions::simplexml_object_result_type(
        ctx,
        &receiver_type,
    )
    .expect("SimpleXML dimension lowering requires one exact wrapper class");
    let append = matches!(index.kind, ExprKind::ArrayAppend);
    let index_value = lower_simplexml_offset(ctx, index);
    let read_mode = lower_int_literal(ctx, access_mode, expr);
    let result_type = if append { wrapper_type.clone() } else { nullable_result_type(wrapper_type) };
    let result = crate::ir_lower::internal_extensions::emit_call(
        ctx,
        opcode,
        crate::ir_lower::internal_extensions::FLAG_RECEIVER
            | crate::ir_lower::internal_extensions::FLAG_WRAPPER_RESULT
            | if append { crate::ir_lower::internal_extensions::FLAG_ARRAY_APPEND_OFFSET } else { 0 },
        vec![receiver.value, index_value.value, read_mode.value],
        result_type,
        expr.span,
    );
    if ctx.value_is_owning_temporary(index_value) {
        crate::ir_lower::ownership::release_if_owned(ctx, index_value, Some(index.span));
    }
    stabilize_borrowed_result_and_release_receiver(ctx, receiver, result, expr.span)
}

/// Lowers a nested-assignment SimpleXML parent with `BP_VAR_W` semantics.
///
/// A nullable static wrapper result keeps PHP's lazy offset evaluation on the
/// null branch, while a live wrapper asks the native handler to materialize one
/// missing numeric element before the subsequent dimension write.
pub(crate) fn lower_simplexml_dimension_read_for_write_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: LoweredValue,
    index: &Expr,
    expr: &Expr,
) -> LoweredValue {
    if value_is_nullable(ctx, receiver.value) {
        return lower_nullable_simplexml_dimension_read(ctx, receiver, index, expr, 1);
    }
    lower_simplexml_dimension_read_from_value(ctx, receiver, index, expr, 1)
}

/// Lowers a nullable SimpleXML dimension without evaluating its offset for null receivers.
fn lower_nullable_simplexml_dimension_read(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: LoweredValue,
    index: &Expr,
    expr: &Expr,
    access_mode: i64,
) -> LoweredValue {
    let wrapper_type = crate::ir_lower::internal_extensions::simplexml_object_result_type(
        ctx,
        &ctx.builder.value_php_type(receiver.value),
    )
    .expect("nullable SimpleXML dimension lowering requires one wrapper class");
    let result_type = nullable_result_type(wrapper_type);
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![receiver.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(expr.span),
    );
    let null_block = ctx.builder.create_named_block("simplexml.dimension.null", Vec::new());
    let read_block = ctx.builder.create_named_block("simplexml.dimension.read", Vec::new());
    let merge = ctx.builder.create_named_block("simplexml.dimension.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: null_block,
        then_args: Vec::new(),
        else_target: read_block,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(null_block);
    let null = lower_boxed_null(ctx, expr);
    store_value_into_temp(ctx, &temp_name, result_type.clone(), null, expr.span);
    branch_to(ctx, merge);
    ctx.builder.position_at_end(read_block);
    let read = lower_simplexml_dimension_read_from_value(ctx, receiver, index, expr, access_mode);
    store_value_into_temp(ctx, &temp_name, result_type, read, expr.span);
    branch_to(ctx, merge);
    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Evaluates one SimpleXML offset once, preserving integer offsets and stringifying the rest.
pub(crate) fn lower_simplexml_offset(
    ctx: &mut LoweringContext<'_, '_>,
    index: &Expr,
) -> LoweredValue {
    if matches!(index.kind, ExprKind::ArrayAppend) {
        return lower_null(ctx, index);
    }
    let index_value = lower_expr(ctx, index);
    if matches!(ctx.builder.value_php_type(index_value.value).codegen_repr(), PhpType::Void | PhpType::Int) {
        index_value
    } else {
        coerce_to_string_at_span(ctx, index_value, Some(index.span))
    }
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
    let array_type = ctx.builder.value_php_type(array);
    if let Some((class_name, true)) = dom_collection_class_and_failure(&array_type) {
        if let Some(result_type) = dom_collection_method_result_type(ctx, &class_name, "item") {
            return result_type;
        }
    }
    match array_type.codegen_repr() {
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
