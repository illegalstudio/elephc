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
            if is_date_interval_undefined_property_expr(ctx, object, property) {
                let receiver = lower_expr(ctx, object);
                release_owning_receiver_temporary(ctx, receiver, arg.span);
                return Some(emit_bool_literal(ctx, false, Some(arg.span)));
            }
            lower_lazy_property_isset_operand(ctx, object, property, arg)
        }
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

/// Returns whether a property name is absent from a DateInterval-family receiver.
fn is_date_interval_undefined_property_expr(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
) -> bool {
    let Some((class_name, _)) = isset_object_expr_class(ctx, object) else {
        return false;
    };
    class_extends_class(ctx, &class_name, "DateInterval")
        && ctx
            .classes
            .get(class_name.trim_start_matches('\\'))
            .is_some_and(|class_info| class_info.visible_property(property).is_none())
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
    if let ExprKind::PropertyAccess { object, property }
        | ExprKind::NullsafePropertyAccess { object, property } = &args[0].kind
    {
        if let Some(value) =
            lower_date_period_virtual_property_empty(ctx, object, property, &args[0])
        {
            return Some(value);
        }
        if matches!(
            property_isset_action(ctx, object, property),
            Some(IssetPropertyAction::Declared)
        ) {
            return Some(lower_declared_property_empty(
                ctx,
                object,
                property,
                &args[0],
            ));
        }
    }
    if let ExprKind::StaticPropertyAccess { receiver, property } = &args[0].kind {
        if static_property_can_be_uninitialized(ctx, receiver, property) {
            return Some(lower_initialized_static_property_empty(
                ctx, receiver, property, name, &args[0],
            ));
        }
    }
    if let ExprKind::ArrayAccess { array, index } = &args[0].kind {
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

/// Evaluates `empty()` on a declared typed property without reading an uninitialized slot.
fn lower_declared_property_empty(
    ctx: &mut LoweringContext<'_, '_>,
    object_expr: &Expr,
    property: &str,
    expr: &Expr,
) -> LoweredValue {
    let object = lower_expr(ctx, object_expr);
    let property_type = property_get_result_type(ctx, object.value, property, Op::PropGet, expr);
    let property_data = ctx.intern_string(property);
    let initialized = ctx.emit_value(
        Op::PropInitialized,
        vec![object.value],
        Some(Immediate::Data(property_data)),
        PhpType::Bool,
        Op::PropInitialized.default_effects(),
        Some(expr.span),
    );
    let temp_name = ctx.declare_hidden_temp(PhpType::Bool);
    let present_block = ctx
        .builder
        .create_named_block("empty.declared.present", Vec::new());
    let absent_block = ctx
        .builder
        .create_named_block("empty.declared.absent", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("empty.declared.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: initialized.value,
        then_target: present_block,
        then_args: Vec::new(),
        else_target: absent_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(present_block);
    let value = ctx.emit_value(
        Op::PropGet,
        vec![object.value],
        Some(Immediate::Data(property_data)),
        property_type,
        Op::PropGet.default_effects(),
        Some(expr.span),
    );
    let truthy = ctx.emit_value(
        Op::IsTruthy,
        vec![value.value],
        None,
        PhpType::Bool,
        Op::IsTruthy.default_effects(),
        Some(expr.span),
    );
    let false_value = lower_bool_literal(ctx, false, expr);
    let empty = ctx.emit_value(
        Op::ICmp,
        vec![truthy.value, false_value.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Eq)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(expr.span),
    );
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, empty, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(absent_block);
    let true_value = lower_bool_literal(ctx, true, expr);
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, true_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    release_owning_receiver_temporary(ctx, object, expr.span);
    ctx.load_local(&temp_name, Some(expr.span))
}

/// Evaluates `empty()` against DatePeriod's raw virtual-property storage.
fn lower_date_period_virtual_property_empty(
    ctx: &mut LoweringContext<'_, '_>,
    object_expr: &Expr,
    property: &str,
    expr: &Expr,
) -> Option<LoweredValue> {
    let (backing, backing_type) = date_period_virtual_property_backing(ctx, object_expr, property)?;
    let object = lower_expr(ctx, object_expr);
    let initialized_data = ctx.intern_string("__elephc_initialized");
    let initialized = ctx.emit_value(
        Op::PropGet,
        vec![object.value],
        Some(Immediate::Data(initialized_data)),
        PhpType::Bool,
        Op::PropGet.default_effects(),
        Some(expr.span),
    );
    let backing_data = ctx.intern_string(backing);
    let value = ctx.emit_value(
        Op::PropGet,
        vec![object.value],
        Some(Immediate::Data(backing_data)),
        backing_type,
        Op::PropGet.default_effects(),
        Some(expr.span),
    );
    let truthy = ctx.emit_value(
        Op::IsTruthy,
        vec![value.value],
        None,
        PhpType::Bool,
        Op::IsTruthy.default_effects(),
        Some(expr.span),
    );
    let false_value = lower_bool_literal(ctx, false, expr);
    let not_initialized = ctx.emit_value(
        Op::ICmp,
        vec![initialized.value, false_value.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Eq)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(expr.span),
    );
    let not_truthy = ctx.emit_value(
        Op::ICmp,
        vec![truthy.value, false_value.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Eq)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(expr.span),
    );
    let result = ctx.emit_value(
        Op::IBitOr,
        vec![not_initialized.value, not_truthy.value],
        None,
        PhpType::Bool,
        Op::IBitOr.default_effects(),
        Some(expr.span),
    );
    release_owning_receiver_temporary(ctx, object, expr.span);
    Some(result)
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
