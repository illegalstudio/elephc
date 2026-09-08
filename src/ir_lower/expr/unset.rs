//! Purpose:
//! Direct unset lowering for locals, arrays, properties, and magic methods.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers supported `unset(...)` targets without evaluating them as ordinary call args.
pub(super) fn lower_unset_locals(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if !args.iter().all(|arg| unset_target_supported(ctx, arg)) {
        return None;
    }
    let null = lower_null(ctx, expr);
    for arg in args {
        match &arg.kind {
            ExprKind::Variable(name) => {
                ctx.unset_local(name, null, Some(arg.span));
            }
            ExprKind::ArrayAccess { array, index } => {
                lower_unset_array_access(ctx, array, index, arg);
            }
            ExprKind::PropertyAccess { object, property }
            | ExprKind::NullsafePropertyAccess { object, property } => {
                lower_unset_property_access(ctx, object, property, arg);
            }
            _ => {}
        }
        if ctx.builder.insertion_block_is_terminated() {
            break;
        }
    }
    if !ctx.builder.insertion_block_is_terminated() {
        crate::ir_lower::ownership::collect_cycles(ctx, Some(expr.span));
    }
    Some(null)
}

/// Returns true when an `unset(...)` target has direct EIR lowering.
pub(super) fn unset_target_supported(ctx: &LoweringContext<'_, '_>, arg: &Expr) -> bool {
    match &arg.kind {
        ExprKind::Variable(_) => true,
        ExprKind::ArrayAccess { array, .. } => {
            unset_array_access_has_object_receiver(ctx, array)
                || unset_array_access_has_local_array_receiver(ctx, array)
        }
        ExprKind::PropertyAccess { object, property }
        | ExprKind::NullsafePropertyAccess { object, property } => {
            unset_property_access_has_direct_lowering(ctx, object, property)
        }
        _ => false,
    }
}

/// Returns true when an array-access unset receiver is a plain array/hash local whose element the
/// EIR backend can remove.
///
/// Associative arrays remove the element directly; packed indexed arrays are converted to a hash at
/// the unset site (PHP `unset()` leaves a sparse array). By-reference locals are excluded: their
/// storage is aliased to a caller whose static type would no longer match after a representation
/// change.
pub(super) fn unset_array_access_has_local_array_receiver(
    ctx: &LoweringContext<'_, '_>,
    array: &Expr,
) -> bool {
    let ExprKind::Variable(name) = &array.kind else {
        return false;
    };
    if ctx.is_ref_bound_local(name) {
        return false;
    }
    matches!(
        ctx.local_type(name).codegen_repr(),
        PhpType::AssocArray { .. } | PhpType::Array(_)
    )
}

/// Returns true when an array-access unset receiver is a static ArrayAccess object.
pub(super) fn unset_array_access_has_object_receiver(
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

/// Lowers `unset($array[$key])`, dispatching on the receiver kind.
///
/// An associative-array local removes the element in place through `Op::HashUnset`. A packed
/// indexed-array local is first converted to a hash (PHP keeps the surviving keys without
/// renumbering) and then removed. An `ArrayAccess` object dispatches to its `offsetUnset($key)`
/// method like before. By-reference array locals fall through to the object path.
pub(super) fn lower_unset_array_access(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
    index: &Expr,
    expr: &Expr,
) {
    if let ExprKind::Variable(name) = &array.kind {
        if !ctx.is_ref_bound_local(name) {
            match ctx.local_type(name).codegen_repr() {
                PhpType::AssocArray { .. } => {
                    lower_unset_hash_element(ctx, name, array.span, index, expr);
                    return;
                }
                PhpType::Array(elem_ty) => {
                    let elem_ty = if *elem_ty == PhpType::Never {
                        PhpType::Mixed
                    } else {
                        *elem_ty
                    };
                    lower_unset_indexed_element(ctx, name, elem_ty, array.span, index, expr);
                    return;
                }
                _ => {}
            }
        }
    }
    let synthetic = Expr::new(
        ExprKind::MethodCall {
            object: Box::new(array.clone()),
            method: "offsetUnset".to_string(),
            args: vec![index.clone()],
        },
        expr.span,
    );
    lower_expr(ctx, &synthetic);
}

/// Lowers `unset($hash[$key])` for an associative-array local as a `HashUnset` instruction.
///
/// Loads the array local, lowers the key, and emits the removal. The backend (`lower_hash_unset`)
/// copy-on-write splits the table, releases the removed key/value payloads, and stores the unique
/// table pointer back into the local slot, so no explicit store-back is needed here.
pub(super) fn lower_unset_hash_element(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    array_span: Span,
    index: &Expr,
    expr: &Expr,
) {
    let array_value = ctx.load_local(name, Some(array_span));
    let index_value = lower_expr(ctx, index);
    ctx.emit_void(
        Op::HashUnset,
        vec![array_value.value, index_value.value],
        None,
        Op::HashUnset.default_effects(),
        Some(expr.span),
    );
}

/// Lowers `unset($arr[$key])` for a packed indexed-array local.
///
/// PHP's `unset()` removes a key without renumbering, so the array can no longer be a contiguous
/// packed list (e.g. `unset([1,2,3][1])` leaves keys `0` and `2`). The local is converted to a hash
/// (`Op::ArrayToHash`) and retyped as `AssocArray<Int, T>`, after which the element is removed
/// through `HashUnset`. Subsequent uses of the local therefore see the associative representation.
pub(super) fn lower_unset_indexed_element(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    elem_ty: PhpType,
    array_span: Span,
    index: &Expr,
    expr: &Expr,
) {
    let array_value = ctx.load_local(name, Some(array_span));
    let assoc_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Int),
        value: Box::new(elem_ty),
    };
    let hash = ctx.emit_value(
        Op::ArrayToHash,
        vec![array_value.value],
        None,
        assoc_ty.clone(),
        Op::ArrayToHash.default_effects(),
        Some(array_span),
    );
    ctx.store_mutated_local(name, hash, assoc_ty, Some(array_span));
    lower_unset_hash_element(ctx, name, array_span, index, expr);
}

/// Returns true when a property unset target can be lowered without normal property storage support.
pub(super) fn unset_property_access_has_direct_lowering(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
) -> bool {
    matches!(
        property_unset_action(ctx, object, property),
        Some(
            UnsetPropertyAction::Declared
                | UnsetPropertyAction::DatePeriodVirtual
                | UnsetPropertyAction::Magic
                | UnsetPropertyAction::Noop
                | UnsetPropertyAction::RemoveDynamic
        )
    )
}

/// Lowers `unset($object->property)` for magic and no-op property targets.
pub(super) fn lower_unset_property_access(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    expr: &Expr,
) {
    match property_unset_action(ctx, object, property) {
        Some(UnsetPropertyAction::Declared) => {
            let object = lower_expr(ctx, object);
            let property_data = ctx.intern_string(property);
            ctx.emit_void(
                Op::PropUnset,
                vec![object.value],
                Some(Immediate::Data(property_data)),
                Op::PropUnset.default_effects(),
                Some(expr.span),
            );
            release_owning_receiver_temporary(ctx, object, expr.span);
        }
        Some(UnsetPropertyAction::RemoveDynamic) => {
            let object = lower_expr(ctx, object);
            let property_data = ctx.intern_string(property);
            ctx.emit_void(
                Op::PropUnset,
                vec![object.value],
                Some(Immediate::Data(property_data)),
                Op::PropUnset.default_effects(),
                Some(expr.span),
            );
            release_owning_receiver_temporary(ctx, object, expr.span);
        }
        Some(UnsetPropertyAction::DatePeriodVirtual) => {
            let class_name = isset_object_expr_class(ctx, object)
                .map(|(class_name, _)| class_name)
                .unwrap_or_else(|| "DatePeriod".to_string());
            let object = lower_expr(ctx, object);
            release_owning_receiver_temporary(ctx, object, expr.span);
            let message = format!("Cannot unset {}::${}", class_name, property);
            crate::ir_lower::stmt::lower_throw_access_error(ctx, &message, expr.span);
        }
        Some(UnsetPropertyAction::Magic) => {
            let object = lower_expr(ctx, object);
            lower_magic_property_unset(ctx, object, property, expr);
        }
        Some(UnsetPropertyAction::Noop) => {
            lower_expr(ctx, object);
        }
        Some(UnsetPropertyAction::Fallback) | None => {}
    }
}

/// Describes how `unset($object->property)` should be lowered for a known receiver class.
pub(super) enum UnsetPropertyAction {
    Fallback,
    Declared,
    DatePeriodVirtual,
    Magic,
    Noop,
    RemoveDynamic,
}

/// Selects the PHP-visible `unset()` behavior for a statically known object property operand.
pub(super) fn property_unset_action(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
) -> Option<UnsetPropertyAction> {
    let (class_name, _) = isset_object_expr_class(ctx, object)?;
    if class_extends_class(ctx, &class_name, "DatePeriod")
        && matches!(
            property,
            "start"
                | "current"
                | "end"
                | "interval"
                | "recurrences"
                | "include_start_date"
                | "include_end_date"
        )
    {
        return Some(UnsetPropertyAction::DatePeriodVirtual);
    }
    if is_builtin_stdclass_name(&class_name) {
        return Some(UnsetPropertyAction::RemoveDynamic);
    }
    let class_info = ctx.classes.get(class_name.as_str())?;
    if property_is_accessible_for_ir(ctx, &class_name, class_info, property) {
        return Some(if class_info.visible_property_is_declared(property) {
            UnsetPropertyAction::Declared
        } else {
            UnsetPropertyAction::Fallback
        });
    }
    if class_info.allow_dynamic_properties && class_info.visible_property(property).is_none() {
        return Some(dynamic_property_unset_action(ctx, &class_name));
    }
    if class_method_signature(ctx, &class_name, &php_symbol_key("__unset")).is_some() {
        Some(UnsetPropertyAction::Magic)
    } else {
        Some(UnsetPropertyAction::Noop)
    }
}

/// Lowers a magic `__unset($name)` call, guarding nullable receivers as a no-op.
pub(super) fn lower_magic_property_unset(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    expr: &Expr,
) {
    if value_is_nullable(ctx, object.value) {
        lower_nullable_magic_property_unset(ctx, object, property, expr);
        return;
    }
    let args = vec![Expr::new(
        ExprKind::StringLiteral(property.to_string()),
        expr.span,
    )];
    lower_method_call_with_receiver(ctx, object, "__unset", &args, Op::MethodCall, expr);
}

/// Lowers `__unset` for nullable receivers, doing nothing when the receiver is null.
pub(super) fn lower_nullable_magic_property_unset(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    expr: &Expr,
) {
    let null_block = ctx
        .builder
        .create_named_block("unset.property.null", Vec::new());
    let call_block = ctx
        .builder
        .create_named_block("unset.property.call", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("unset.property.merge", Vec::new());
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![object.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(expr.span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: null_block,
        then_args: Vec::new(),
        else_target: call_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(null_block);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(call_block);
    let args = vec![Expr::new(
        ExprKind::StringLiteral(property.to_string()),
        expr.span,
    )];
    lower_method_call_with_receiver(ctx, object, "__unset", &args, Op::MethodCall, expr);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
}
