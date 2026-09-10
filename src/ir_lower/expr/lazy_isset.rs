//! Purpose:
//! Lazy isset and empty lowering for magic-property semantics.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers `isset()` as a lazy language construct instead of an eager builtin call.
pub(super) fn lower_lazy_isset(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "isset" {
        return None;
    }
    if crate::types::call_args::has_named_args(args) || args.iter().any(is_spread_arg) {
        return None;
    }
    if args.is_empty() {
        return Some(lower_bool_literal(ctx, false, expr));
    }

    let temp_name = ctx.declare_hidden_temp(PhpType::Bool);
    let false_block = ctx.builder.create_named_block("isset.lazy_false", Vec::new());
    let merge = ctx.builder.create_named_block("isset.lazy_merge", Vec::new());
    for (idx, arg) in args.iter().enumerate() {
        let checked = lower_lazy_isset_operand(ctx, arg).unwrap_or_else(|| {
            // `isset()` never emits undefined-offset warnings, so eager array
            // operands must be lowered with the silent read variants.
            let value = if let ExprKind::ArrayAccess { array, index } = &arg.kind {
                lower_array_access_with_missing_warning(ctx, array, index, arg, false)
            } else {
                lower_expr(ctx, arg)
            };
            emit_builtin_call_value(ctx, name, vec![value.value], PhpType::Int, arg.span, None)
        });
        let then_target = if idx + 1 == args.len() {
            ctx.builder.create_named_block("isset.lazy_true", Vec::new())
        } else {
            ctx.builder.create_named_block("isset.lazy_next", Vec::new())
        };
        ctx.builder.terminate(Terminator::CondBr {
            cond: checked.value,
            then_target,
            then_args: Vec::new(),
            else_target: false_block,
            else_args: Vec::new(),
        });
        ctx.builder.position_at_end(then_target);
    }

    let true_value = lower_bool_literal(ctx, true, expr);
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, true_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(false_block);
    let false_value = lower_bool_literal(ctx, false, expr);
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, false_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    Some(take_owned_temp(ctx, &temp_name, expr.span))
}

/// Lowers a single `isset()` operand that has special lazy PHP semantics.
pub(super) fn lower_lazy_isset_operand(
    ctx: &mut LoweringContext<'_, '_>,
    arg: &Expr,
) -> Option<LoweredValue> {
    match &arg.kind {
        ExprKind::ArrayAccess { array, index } => {
            if simplexml_object_expr_class(ctx, array).is_some() {
                return Some(lower_simplexml_has_dimension(ctx, array, index, false, arg));
            }
            if array_access_expr_satisfies_array_access(ctx, array) {
                let synthetic = Expr::new(
                    ExprKind::MethodCall {
                        object: array.clone(),
                        method: "offsetExists".to_string(),
                        args: vec![(**index).clone()],
                    },
                    arg.span,
                );
                return Some(lower_expr(ctx, &synthetic));
            }
            if !array_access_expr_supports_native_isset_probe(ctx, array) {
                return None;
            }
            Some(lower_native_isset_offset_probe(ctx, array, index, arg))
        }
        ExprKind::PropertyAccess { object, property }
        | ExprKind::NullsafePropertyAccess { object, property } => {
            if simplexml_object_expr_class(ctx, object).is_some() {
                return Some(lower_simplexml_has_property(
                    ctx, object, property, false, arg,
                ));
            }
            lower_lazy_property_isset_operand(ctx, object, property, arg)
        }
        // A typed static property starts uninitialized and `isset()` must answer false there
        // rather than take the ordinary read, whose backend guard is fatal.
        ExprKind::StaticPropertyAccess { receiver, property }
            if static_property_can_be_uninitialized(ctx, receiver, property) =>
        {
            Some(lower_initialized_static_property_isset(
                ctx, receiver, property, arg,
            ))
        }
        // `isset($this)` inside a static closure always evaluates to `false`
        // because static closures have no `$this` binding. PHP allows this
        // probe and returns false; elephc must not try to load a missing slot.
        ExprKind::This if !ctx.local_slots.contains_key("this") => {
            Some(lower_bool_literal(ctx, false, arg))
        }
        _ => None,
    }
}

/// Lowers `empty($obj->magicProp)` with PHP's overloaded-property semantics:
/// `empty` consults `__isset` first and only evaluates `__get` when `__isset`
/// is truthy, so an unset virtual property is empty without ever reading it.
/// Returns `None` for operands the eager `empty` builtin already handles (plain
/// variables, declared properties, array elements), letting that path run.
pub(super) fn lower_lazy_empty(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "empty" {
        return None;
    }
    if args.len() != 1
        || crate::types::call_args::has_named_args(args)
        || args.iter().any(is_spread_arg)
    {
        return None;
    }
    if let ExprKind::ArrayAccess { array, index } = &args[0].kind {
        if simplexml_object_expr_class(ctx, array).is_some() {
            let exists = lower_simplexml_has_dimension(ctx, array, index, true, &args[0]);
            return Some(invert_bool_value(ctx, exists, expr.span));
        }
        let value = lower_array_access_with_missing_warning(ctx, array, index, &args[0], false);
        return Some(emit_builtin_call_value(
            ctx,
            name,
            vec![value.value],
            PhpType::Bool,
            expr.span,
            None,
        ));
    }
    if let ExprKind::PropertyAccess { object, property }
    | ExprKind::NullsafePropertyAccess { object, property } = &args[0].kind
    {
        if simplexml_object_expr_class(ctx, object).is_some() {
            let exists = lower_simplexml_has_property(ctx, object, property, true, &args[0]);
            return Some(invert_bool_value(ctx, exists, expr.span));
        }
    }
    // A typed static property that is still uninitialized is EMPTY in PHP, and reading it the
    // ordinary way to find that out is fatal — the same reason `isset()` and `??` need the
    // slot probe. Uninitialized answers true without the read.
    if let ExprKind::StaticPropertyAccess { receiver, property } = &args[0].kind {
        if static_property_can_be_uninitialized(ctx, receiver, property) {
            return Some(lower_initialized_static_property_empty(
                ctx, receiver, property, name, &args[0],
            ));
        }
    }
    // The instance twin of the static arm above: an uninitialized typed slot is EMPTY, and the
    // ordinary read that would say so raises. `property_isset_action` already decides whether the
    // slot is a declared one worth probing — the same decision `isset()` makes, reused rather than
    // re-derived so the two constructs cannot drift on which properties they consider declared.
    if let ExprKind::PropertyAccess { object, property } = &args[0].kind {
        if matches!(
            property_isset_action(ctx, object, property),
            Some(IssetPropertyAction::Initialized)
        ) {
            let object = lower_expr(ctx, object);
            return Some(lower_initialized_property_empty(
                ctx, object, property, name, &args[0],
            ));
        }
    }
    let (exists_call, get_call) = lazy_empty_magic_property_calls(ctx, &args[0])?;

    let temp_name = ctx.declare_hidden_temp(PhpType::Bool);
    let present_block = ctx.builder.create_named_block("empty.present", Vec::new());
    let absent_block = ctx.builder.create_named_block("empty.absent", Vec::new());
    let merge = ctx.builder.create_named_block("empty.merge", Vec::new());

    // `__isset(prop)` decides whether the property is considered set at all.
    let exists = lower_expr(ctx, &exists_call);
    ctx.builder.terminate(Terminator::CondBr {
        cond: exists.value,
        then_target: present_block,
        then_args: Vec::new(),
        else_target: absent_block,
        else_args: Vec::new(),
    });

    // Set: empty is the emptiness of the `__get` value (reuses the eager builtin).
    ctx.builder.position_at_end(present_block);
    let get_value = lower_expr(ctx, &get_call);
    let empty_name = ctx.intern_function_name(name);
    let empty_value = ctx.emit_value(
        Op::LanguageConstructCall,
        vec![get_value.value],
        Some(Immediate::Data(empty_name)),
        PhpType::Bool,
        effects_lookup::language_construct_effects(name),
        Some(expr.span),
    );
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, empty_value, expr.span);
    branch_to(ctx, merge);

    // Not set: empty is true and `__get` is never called.
    ctx.builder.position_at_end(absent_block);
    let true_value = lower_bool_literal(ctx, true, expr);
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, true_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    Some(ctx.load_local(&temp_name, Some(expr.span)))
}

/// Lowers one SimpleXML property `isset`/`empty` probe through php-src's handler.
fn lower_simplexml_has_property(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    check_empty: bool,
    expr: &Expr,
) -> LoweredValue {
    let may_be_failure = simplexml_object_expr_class(ctx, object)
        .is_some_and(|(_, may_be_failure)| may_be_failure);
    let receiver = lower_expr(ctx, object);
    if may_be_failure || value_is_nullable(ctx, receiver.value) {
        return lower_nullable_simplexml_has_property(
            ctx,
            receiver,
            property,
            check_empty,
            expr,
        );
    }
    lower_simplexml_has_property_from_value(ctx, receiver, property, check_empty, expr)
}

/// Emits a non-failing SimpleXML property existence probe and releases its receiver temp.
fn lower_simplexml_has_property_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: LoweredValue,
    property: &str,
    check_empty: bool,
    expr: &Expr,
) -> LoweredValue {
    let opcode = crate::ir_lower::internal_extensions::simplexml_object_handler_opcode_for_type(
        ctx,
        &ctx.builder.value_php_type(receiver.value),
        "has_property",
    )
    .expect("SimpleXML property probe requires the locked has handler");
    let name = lower_string_literal(ctx, property, expr);
    let check_empty = lower_bool_literal(ctx, check_empty, expr);
    let result = crate::ir_lower::internal_extensions::emit_call(
        ctx,
        opcode,
        crate::ir_lower::internal_extensions::FLAG_RECEIVER,
        vec![receiver.value, name.value, check_empty.value],
        PhpType::Bool,
        expr.span,
    );
    release_owning_receiver_temporary(ctx, receiver, expr.span);
    result
}

/// Returns false for a failed SimpleXML receiver without probing or evaluating a name.
fn lower_nullable_simplexml_has_property(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: LoweredValue,
    property: &str,
    check_empty: bool,
    expr: &Expr,
) -> LoweredValue {
    let temp_name = ctx.declare_hidden_temp(PhpType::Bool);
    let null_block = ctx
        .builder
        .create_named_block("simplexml.has_property.null", Vec::new());
    let probe_block = ctx
        .builder
        .create_named_block("simplexml.has_property.probe", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("simplexml.has_property.merge", Vec::new());
    let is_failure = simplexml_receiver_is_failure(ctx, receiver.value, expr.span);
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_failure.value,
        then_target: null_block,
        then_args: Vec::new(),
        else_target: probe_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(null_block);
    release_owning_receiver_temporary(ctx, receiver, expr.span);
    let absent = emit_bool_literal(ctx, false, Some(expr.span));
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, absent, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(probe_block);
    let present =
        lower_simplexml_has_property_from_value(ctx, receiver, property, check_empty, expr);
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, present, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    ctx.load_local(&temp_name, Some(expr.span))
}

/// Lowers one SimpleXML dimension `isset`/`empty` probe through php-src's handler.
fn lower_simplexml_has_dimension(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
    index: &Expr,
    check_empty: bool,
    expr: &Expr,
) -> LoweredValue {
    let may_be_failure = simplexml_object_expr_class(ctx, array)
        .is_some_and(|(_, may_be_failure)| may_be_failure);
    let receiver = lower_expr(ctx, array);
    if may_be_failure || value_is_nullable(ctx, receiver.value) {
        return lower_nullable_simplexml_has_dimension(ctx, receiver, index, check_empty, expr);
    }
    lower_simplexml_has_dimension_from_value(ctx, receiver, index, check_empty, expr)
}

/// Emits a non-failing SimpleXML dimension existence probe and releases its operands.
fn lower_simplexml_has_dimension_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: LoweredValue,
    index: &Expr,
    check_empty: bool,
    expr: &Expr,
) -> LoweredValue {
    let receiver_type = ctx.builder.value_php_type(receiver.value);
    let opcode = crate::ir_lower::internal_extensions::simplexml_object_handler_opcode_for_type(
        ctx,
        &receiver_type,
        "has_dimension",
    )
    .expect("SimpleXML dimension probe requires the locked has handler");
    let index_value = lower_simplexml_offset(ctx, index);
    let check_empty = lower_bool_literal(ctx, check_empty, expr);
    let result = crate::ir_lower::internal_extensions::emit_call(
        ctx,
        opcode,
        crate::ir_lower::internal_extensions::FLAG_RECEIVER,
        vec![receiver.value, index_value.value, check_empty.value],
        PhpType::Bool,
        expr.span,
    );
    if ctx.value_is_owning_temporary(index_value) {
        crate::ir_lower::ownership::release_if_owned(ctx, index_value, Some(index.span));
    }
    release_owning_receiver_temporary(ctx, receiver, expr.span);
    result
}

/// Returns false for a failed SimpleXML dimension receiver without evaluating the offset.
fn lower_nullable_simplexml_has_dimension(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: LoweredValue,
    index: &Expr,
    check_empty: bool,
    expr: &Expr,
) -> LoweredValue {
    let temp_name = ctx.declare_hidden_temp(PhpType::Bool);
    let null_block = ctx
        .builder
        .create_named_block("simplexml.has_dimension.null", Vec::new());
    let probe_block = ctx
        .builder
        .create_named_block("simplexml.has_dimension.probe", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("simplexml.has_dimension.merge", Vec::new());
    let is_failure = simplexml_receiver_is_failure(ctx, receiver.value, expr.span);
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_failure.value,
        then_target: null_block,
        then_args: Vec::new(),
        else_target: probe_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(null_block);
    let absent = emit_bool_literal(ctx, false, Some(expr.span));
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, absent, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(probe_block);
    let present =
        lower_simplexml_has_dimension_from_value(ctx, receiver, index, check_empty, expr);
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, present, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    ctx.load_local(&temp_name, Some(expr.span))
}

/// Inverts one already-boolean EIR value.
fn invert_bool_value(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    let zero = emit_i64_at_span(ctx, 0, span);
    ctx.emit_value(
        Op::ICmp,
        vec![value.value, zero.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Eq)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(span),
    )
}

/// For an `empty()` operand that is an overloaded (magic) property access,
/// returns the `(__isset, __get)` synthetic call expressions PHP would evaluate.
/// The property name is a string literal, so reusing it for both calls is
/// side-effect free. Returns `None` for any other operand shape.
pub(super) fn lazy_empty_magic_property_calls(
    ctx: &LoweringContext<'_, '_>,
    arg: &Expr,
) -> Option<(Expr, Expr)> {
    match &arg.kind {
        ExprKind::PropertyAccess { object, property } => {
            property_existence_magic_class(ctx, object, property, "__isset")?;
            let key = Expr::new(ExprKind::StringLiteral(property.clone()), arg.span);
            let exists = Expr::new(
                ExprKind::MethodCall {
                    object: object.clone(),
                    method: "__isset".to_string(),
                    args: vec![key.clone()],
                },
                arg.span,
            );
            let get = Expr::new(
                ExprKind::MethodCall {
                    object: object.clone(),
                    method: "__get".to_string(),
                    args: vec![key],
                },
                arg.span,
            );
            Some((exists, get))
        }
        ExprKind::NullsafePropertyAccess { object, property } => {
            property_existence_magic_class(ctx, object, property, "__isset")?;
            let key = Expr::new(ExprKind::StringLiteral(property.clone()), arg.span);
            let exists = Expr::new(
                ExprKind::NullsafeMethodCall {
                    object: object.clone(),
                    method: "__isset".to_string(),
                    args: vec![key.clone()],
                },
                arg.span,
            );
            let get = Expr::new(
                ExprKind::NullsafeMethodCall {
                    object: object.clone(),
                    method: "__get".to_string(),
                    args: vec![key],
                },
                arg.span,
            );
            Some((exists, get))
        }
        _ => None,
    }
}

/// Returns the class whose `magic` method (`__isset`/`__unset`) should handle
/// property existence/removal: a property that cannot be accessed normally on an
/// object whose class declares the magic method.
pub(super) fn property_existence_magic_class(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    magic: &str,
) -> Option<String> {
    let class_name = instance_callable_object_class(ctx, object)?;
    let class_info = ctx.classes.get(&class_name)?;
    if property_is_accessible_for_ir(ctx, &class_name, class_info, property) {
        return None;
    }
    class_method_signature(ctx, &class_name, &php_symbol_key(magic)).map(|_| class_name)
}

/// Resolves the precise checker type carried by a potential SimpleXML receiver expression.
fn simplexml_object_expr_type(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
) -> Option<PhpType> {
    let object_type = match &object.kind {
        ExprKind::Variable(name) => ctx.local_type(name),
        ExprKind::This => PhpType::Object(ctx.current_class.clone()?),
        ExprKind::NewObject { class_name, .. } => PhpType::Object(class_name.to_string()),
        ExprKind::NewDynamicObject { fallback_class, .. } => {
            PhpType::Object(fallback_class.to_string())
        }
        ExprKind::FunctionCall { name, .. } => ctx
            .functions
            .get(name.as_str())
            .map(|sig| sig.return_type.clone())?,
        ExprKind::PropertyAccess { object, property } => {
            property_access_expr_type_for_ir(ctx, object, property)?
        }
        ExprKind::NullsafePropertyAccess { object, property } => {
            nullsafe_property_access_expr_type_for_ir(ctx, object, property)?
        }
        ExprKind::MethodCall { object, method, .. } => {
            method_call_expr_type_for_ir(ctx, object, method)?
        }
        ExprKind::NullsafeMethodCall { object, method, .. } => {
            nullsafe_method_call_expr_type_for_ir(ctx, object, method)?
        }
        ExprKind::StaticMethodCall {
            receiver, method, ..
        } => static_method_call_expr_type_for_ir(ctx, receiver, method)?,
        _ => infer_expr_type_syntactic(object),
    };
    Some(object_type)
}

/// Returns the exact SimpleXML receiver class and whether its expression may fail.
pub(crate) fn simplexml_object_expr_class(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
) -> Option<(String, bool)> {
    if let Some(object_type) = simplexml_object_expr_type(ctx, object) {
        if let Some(PhpType::Object(class_name)) =
            crate::ir_lower::internal_extensions::simplexml_object_result_type(ctx, &object_type)
        {
            let may_be_failure = matches!(
                object_type,
                PhpType::Union(ref members)
                    if members.iter().any(|member| {
                        matches!(member, PhpType::Void | PhpType::Never | PhpType::False)
                    })
            );
            return normalized_class_name(&class_name)
                .map(|class_name| (class_name, may_be_failure));
        }
    }
    match &object.kind {
        ExprKind::PropertyAccess { object, .. } => simplexml_object_expr_class(ctx, object),
        ExprKind::NullsafePropertyAccess { object, .. } => {
            simplexml_object_expr_class(ctx, object).map(|(class_name, _)| (class_name, true))
        }
        ExprKind::ArrayAccess { array, .. } => {
            simplexml_object_expr_class(ctx, array).map(|(class_name, _)| (class_name, true))
        }
        _ => None,
    }
}
