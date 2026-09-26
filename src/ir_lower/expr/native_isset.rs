//! Purpose:
//! Native array and property isset probes.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers native array/hash `isset($array[$key])` without reading the element value.
pub(super) fn lower_native_isset_offset_probe(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
    index: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let array_value = lower_subscript_receiver_silently(ctx, array);
    if value_is_nullable(ctx, array_value.value) {
        return lower_nullable_native_isset_offset_probe(ctx, array_value, index, expr);
    }
    lower_native_isset_offset_probe_from_value(ctx, array_value, index, expr)
}

/// Lowers nullable native array/hash `isset` without evaluating the offset on null receivers.
pub(super) fn lower_nullable_native_isset_offset_probe(
    ctx: &mut LoweringContext<'_, '_>,
    array_value: LoweredValue,
    index: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![array_value.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(expr.span),
    );
    let temp_name = ctx.declare_hidden_temp(PhpType::Bool);
    let null_block = ctx
        .builder
        .create_named_block("isset.native.null", Vec::new());
    let probe_block = ctx
        .builder
        .create_named_block("isset.native.probe", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("isset.native.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: null_block,
        then_args: Vec::new(),
        else_target: probe_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(null_block);
    let false_value = emit_bool_literal(ctx, false, Some(expr.span));
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, false_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(probe_block);
    let checked = lower_native_isset_offset_probe_from_value(ctx, array_value, index, expr);
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, checked, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Lowers native array/hash `isset` once the receiver has already been evaluated.
pub(super) fn lower_native_isset_offset_probe_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    array_value: LoweredValue,
    index: &Expr,
    expr: &Expr,
) -> LoweredValue {
    match array_value.ir_type {
        IrType::Heap(IrHeapKind::Array) => {
            let mut index_value = lower_expr(ctx, index);
            let index_ty = isset_index_expr_key_type(ctx, index, index_value.value);
            if index_ty == PhpType::Int {
                index_value = coerce_array_key_to_int_at_span(ctx, index_value, Some(index.span), false);
                ctx.emit_value(
                    Op::ArrayIsset,
                    vec![array_value.value, index_value.value],
                    None,
                    PhpType::Bool,
                    Op::ArrayIsset.default_effects(),
                    Some(expr.span),
                )
            } else {
                // `isset()` is a silent probe even when the key is absent.
                let read_value = ctx.emit_value(
                    Op::ArrayGetMixedKeySilent,
                    vec![array_value.value, index_value.value],
                    None,
                    PhpType::Mixed,
                    Op::ArrayGetMixedKeySilent.default_effects(),
                    Some(expr.span),
                );
                let is_null = ctx.emit_value(
                    Op::IsNull,
                    vec![read_value.value],
                    None,
                    PhpType::Bool,
                    Op::IsNull.default_effects(),
                    Some(expr.span),
                );
                crate::ir_lower::ownership::release_if_owned(
                    ctx,
                    read_value,
                    Some(expr.span),
                );
                let zero = ctx.emit_value(
                    Op::ConstI64,
                    Vec::new(),
                    Some(Immediate::I64(0)),
                    PhpType::Int,
                    Op::ConstI64.default_effects(),
                    Some(expr.span),
                );
                ctx.emit_value(
                    Op::ICmp,
                    vec![is_null.value, zero.value],
                    Some(Immediate::CmpPredicate(crate::ir::CmpPredicate::Eq)),
                    PhpType::Bool,
                    Op::ICmp.default_effects(),
                    Some(expr.span),
                )
            }
        }
        IrType::Heap(IrHeapKind::Hash) => {
            let index_value = lower_expr(ctx, index);
            ctx.emit_value(
                Op::HashIsset,
                vec![array_value.value, index_value.value],
                None,
                PhpType::Bool,
                Op::HashIsset.default_effects(),
                Some(expr.span),
            )
        }
        _ => {
            let read_value = lower_array_access_from_value(ctx, array_value, index, expr, false);
            emit_builtin_call_value(
                ctx,
                "isset",
                vec![read_value.value],
                PhpType::Int,
                expr.span,
                None,
            )
        }
    }
}

/// Returns whether a syntactic array receiver can use a non-materializing native `isset` probe.
pub(super) fn array_access_expr_supports_native_isset_probe(
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
            property_access_expr_type_for_ir(ctx, object, property)
                .unwrap_or_else(|| infer_expr_type_syntactic(array))
        }
        ExprKind::ArrayLiteral(items) => array_literal_type_for_ir(ctx, items, array),
        ExprKind::ArrayLiteralAssoc(pairs) => assoc_array_literal_type_for_ir(ctx, pairs, array),
        _ => infer_expr_type_syntactic(array),
    }
    .codegen_repr();
    matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. })
}

/// Lowers `isset($object->property)` without performing a normal property read first.
pub(super) fn lower_lazy_property_isset_operand(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    arg: &Expr,
) -> Option<LoweredValue> {
    match property_isset_action(ctx, object, property)? {
        IssetPropertyAction::Fallback => None,
        IssetPropertyAction::Magic => {
            let object = lower_expr(ctx, object);
            Some(lower_magic_property_isset(ctx, object, property, arg))
        }
        IssetPropertyAction::AlwaysFalse => {
            lower_expr(ctx, object);
            Some(emit_bool_literal(ctx, false, Some(arg.span)))
        }
        IssetPropertyAction::Initialized => {
            let object = lower_expr(ctx, object);
            Some(lower_initialized_property_isset(ctx, object, property, arg))
        }
    }
}

/// Describes how `isset($object->property)` should be lowered for a known receiver class.
pub(super) enum IssetPropertyAction {
    Fallback,
    Magic,
    AlwaysFalse,
    /// A declared (typed) property slot, which can be uninitialized: probe the slot
    /// before reading it so `isset()` never raises the uninitialized-read error.
    Initialized,
}

/// Selects the PHP-visible `isset()` behavior for a statically known object property operand.
pub(super) fn property_isset_action(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
) -> Option<IssetPropertyAction> {
    let (class_name, _) = isset_object_expr_class(ctx, object)?;
    if is_builtin_stdclass_name(&class_name) {
        return Some(IssetPropertyAction::Fallback);
    }
    let class_info = ctx.classes.get(class_name.as_str())?;
    if class_info.allow_dynamic_properties {
        return Some(IssetPropertyAction::Fallback);
    }
    if property_is_accessible_for_ir(ctx, &class_name, class_info, property) {
        if let Some((index, (_, property_ty))) = class_info.visible_property(property) {
            if class_info.property_slot_is_declared(index, property)
                || (!class_info.property_slot_is_reference(index, property)
                    && property_ty.codegen_repr() == PhpType::Mixed)
            {
                return Some(IssetPropertyAction::Initialized);
            }
        }
        return Some(IssetPropertyAction::Fallback);
    }
    if class_method_signature(ctx, &class_name, &php_symbol_key("__isset")).is_some() {
        Some(IssetPropertyAction::Magic)
    } else {
        Some(IssetPropertyAction::AlwaysFalse)
    }
}

/// Returns the single receiver class and whether that receiver may be null.
pub(super) fn isset_object_expr_class(ctx: &LoweringContext<'_, '_>, object: &Expr) -> Option<(String, bool)> {
    let ty = match &object.kind {
        ExprKind::Variable(name) => ctx.local_type(name),
        ExprKind::This => PhpType::Object(ctx.current_class.clone()?),
        ExprKind::NewObject { class_name, .. } => PhpType::Object(class_name.to_string()),
        ExprKind::NewDynamicObject { fallback_class, .. } => {
            PhpType::Object(fallback_class.to_string())
        }
        ExprKind::FunctionCall { name, .. } => ctx
            .functions
            .get(name.as_str())
            .map(|sig| sig.return_type.clone())
            .unwrap_or_else(|| infer_expr_type_syntactic(object)),
        _ => infer_expr_type_syntactic(object),
    };
    let (class_name, nullable) = singular_object_class(&ty)?;
    normalized_class_name(class_name).map(|name| (name, nullable))
}

/// Returns whether a named property can use normal `isset()` value probing.
pub(super) fn property_is_accessible_for_ir(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    class_info: &crate::types::ClassInfo,
    property: &str,
) -> bool {
    if class_info.visible_property(property).is_none() {
        return false;
    }
    class_info
        .property_visibilities
        .get(property)
        .is_none_or(|visibility| {
            let declaring_class = class_info
                .property_declaring_classes
                .get(property)
                .map(String::as_str)
                .unwrap_or(class_name);
            ir_can_access_member(ctx, declaring_class, visibility)
        })
}

/// Checks PHP member visibility from the current lowering class scope.
pub(super) fn ir_can_access_member(
    ctx: &LoweringContext<'_, '_>,
    declaring_class: &str,
    visibility: &Visibility,
) -> bool {
    match visibility {
        Visibility::Public => true,
        Visibility::Private => ctx
            .current_class
            .as_deref()
            .is_some_and(|current| same_php_class_name(current, declaring_class)),
        Visibility::Protected => ctx.current_class.as_deref().is_some_and(|current| {
            same_php_class_name(current, declaring_class)
                || class_extends_class(ctx, current, declaring_class)
        }),
    }
}

/// Returns true when two class metadata names match PHP's case-insensitive class lookup.
pub(super) fn same_php_class_name(left: &str, right: &str) -> bool {
    php_symbol_key(left.trim_start_matches('\\')) == php_symbol_key(right.trim_start_matches('\\'))
}

/// Lowers a magic `__isset($name)` call and coerces the result to PHP boolean semantics.
pub(super) fn lower_magic_property_isset(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    arg: &Expr,
) -> LoweredValue {
    if value_is_nullable(ctx, object.value) {
        return lower_nullable_magic_property_isset(ctx, object, property, arg);
    }
    let args = vec![Expr::new(
        ExprKind::StringLiteral(property.to_string()),
        arg.span,
    )];
    let result =
        lower_method_call_with_receiver(ctx, object, "__isset", &args, Op::MethodCall, arg);
    ctx.truthy_consuming(result, Some(arg.span))
}

/// Lowers `__isset` for nullable receivers, returning false instead of calling on null.
pub(super) fn lower_nullable_magic_property_isset(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    arg: &Expr,
) -> LoweredValue {
    let temp_name = ctx.declare_hidden_temp(PhpType::Bool);
    let null_block = ctx
        .builder
        .create_named_block("isset.property.null", Vec::new());
    let call_block = ctx
        .builder
        .create_named_block("isset.property.call", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("isset.property.merge", Vec::new());
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![object.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(arg.span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: null_block,
        then_args: Vec::new(),
        else_target: call_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(null_block);
    let false_value = emit_bool_literal(ctx, false, Some(arg.span));
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, false_value, arg.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(call_block);
    let args = vec![Expr::new(
        ExprKind::StringLiteral(property.to_string()),
        arg.span,
    )];
    let result =
        lower_method_call_with_receiver(ctx, object, "__isset", &args, Op::MethodCall, arg);
    let result = ctx.truthy_consuming(result, Some(arg.span));
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, result, arg.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    ctx.load_local(&temp_name, Some(arg.span))
}
