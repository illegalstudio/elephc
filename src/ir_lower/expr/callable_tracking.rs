//! Purpose:
//! Static callable and reflection binding tracking plus callable-array assignment.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Resolves a literal first-class callable expression to a static local binding.
pub(crate) fn static_callable_binding_for_expr(
    ctx: &LoweringContext<'_, '_>,
    expr: &Expr,
) -> Option<StaticCallableBinding> {
    match &expr.kind {
        ExprKind::StringLiteral(name) => resolve_static_string_callable(ctx, name),
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => {
            resolve_static_string_callable(ctx, name.as_str())
        }
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }) => {
            resolve_static_method_callable(ctx, receiver.clone(), method.clone())
        }
        ExprKind::ArrayLiteral(items) => static_array_callable_descriptor_target(ctx, items)
            .or_else(|| instance_array_callable_target(ctx, items)),
        ExprKind::FirstClassCallable(CallableTarget::Method { object, method }) => {
            resolve_instance_method_callable(ctx, object, method.clone(), false)
        }
        ExprKind::Variable(name) => ctx.static_callable_local(name),
        _ => None,
    }
}

/// Returns the reflected class captured by a statically-known `ReflectionClass` expression.
pub(crate) fn reflection_class_binding_for_expr(
    ctx: &LoweringContext<'_, '_>,
    expr: &Expr,
) -> Option<String> {
    reflection_class_new_instance_reflected_class(ctx, expr)
}

/// Returns the reflected function captured by a statically-known `ReflectionFunction` expression.
pub(crate) fn reflection_function_binding_for_expr(
    ctx: &LoweringContext<'_, '_>,
    expr: &Expr,
) -> Option<String> {
    reflection_function_reflected_target(ctx, expr)
}

/// Returns the reflected property captured by a statically-known `ReflectionProperty` expression.
pub(crate) fn reflection_property_binding_for_expr(
    ctx: &LoweringContext<'_, '_>,
    expr: &Expr,
) -> Option<(String, String)> {
    reflection_property_reflected_target(ctx, expr)
}

/// Returns the reflected method captured by a statically-known `ReflectionMethod` expression.
pub(crate) fn reflection_method_binding_for_expr(
    ctx: &LoweringContext<'_, '_>,
    expr: &Expr,
) -> Option<(String, String)> {
    reflection_method_reflected_target(ctx, expr)
}

/// Returns a safe static argument array that can be replayed for reflection forwarding.
pub(crate) fn reflection_arg_array_binding_for_expr(expr: &Expr) -> Option<Vec<Expr>> {
    let args = reflection_class_new_instance_args_value_without_locals(expr)?;
    if args.iter().all(reflection_arg_expr_can_track) {
        Some(args)
    } else {
        None
    }
}

/// Returns true when replaying an argument expression cannot duplicate side effects.
pub(super) fn reflection_arg_expr_can_track(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_) => true,
        ExprKind::Negate(inner) => matches!(
            &inner.kind,
            ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_)
        ),
        ExprKind::NamedArg { value, .. } => reflection_arg_expr_can_track(value),
        ExprKind::ArrayLiteral(items) => items.iter().all(reflection_arg_expr_can_track),
        ExprKind::ArrayLiteralAssoc(entries) => entries.iter().all(|(key, value)| {
            reflection_arg_array_key_can_track(key) && reflection_arg_expr_can_track(value)
        }),
        _ => false,
    }
}

/// Returns true when an associative array key is stable enough for replay.
pub(super) fn reflection_arg_array_key_can_track(expr: &Expr) -> bool {
    matches!(
        expr.kind,
        ExprKind::StringLiteral(_)
            | ExprKind::IntLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::FloatLiteral(_)
    )
}

/// EIR value and callable binding produced by a callable-array assignment.
pub(crate) struct LoweredCallableArrayAssignment {
    pub(crate) value: LoweredValue,
    pub(crate) target: StaticCallableBinding,
}

/// Lowers a callable-array assignment while preserving its PHP array value.
pub(crate) fn lower_callable_array_for_assignment(
    ctx: &mut LoweringContext<'_, '_>,
    value: &Expr,
    target: Option<&StaticCallableBinding>,
) -> Option<LoweredCallableArrayAssignment> {
    let ExprKind::ArrayLiteral(items) = &value.kind else {
        return None;
    };
    let StaticCallableBinding::InstanceMethod {
        object,
        method,
        signature,
        ..
    } = target? else {
        return None;
    };
    let receiver = lower_expr(ctx, object);
    let receiver_ty = ctx.builder.value_php_type(receiver.value);
    let hidden_name = ctx.declare_hidden_temp(receiver_ty.clone());
    let receiver = ctx.store_local(&hidden_name, receiver, receiver_ty, Some(object.span));
    let array = lower_callable_array_literal_with_receiver(ctx, items, value, receiver);
    let hidden_object = Expr::new(ExprKind::Variable(hidden_name), object.span);
    let target = StaticCallableBinding::InstanceMethod {
        object: Box::new(hidden_object),
        method: method.clone(),
        signature: signature.clone(),
        direct_call: true,
    };
    Some(LoweredCallableArrayAssignment { value: array, target })
}

/// Lowers a callable-array literal after its receiver has already been captured.
pub(super) fn lower_callable_array_literal_with_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    items: &[Expr],
    expr: &Expr,
    receiver: LoweredValue,
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
    ctx.emit_void(
        Op::ArrayPush,
        vec![array.value, receiver.value],
        None,
        Op::ArrayPush.default_effects(),
        Some(expr.span),
    );
    crate::ir_lower::stmt::release_indexed_array_write_operand(ctx, elem_ty.as_ref(), receiver, expr.span);
    for item in items.iter().skip(1) {
        let value = lower_expr(ctx, item);
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

/// Resolves a static callable array literal as a descriptor-backed static method.
pub(super) fn static_array_callable_descriptor_target(
    ctx: &LoweringContext<'_, '_>,
    items: &[Expr],
) -> Option<StaticCallableBinding> {
    static_array_callable_parts(ctx, items).map(|(receiver, method)| {
        StaticCallableBinding::StaticMethodDescriptor { receiver, method }
    })
}

/// Resolves a literal `[$object, "method"]` callable array as an instance method.
pub(super) fn instance_array_callable_target(
    ctx: &LoweringContext<'_, '_>,
    items: &[Expr],
) -> Option<StaticCallableBinding> {
    let [object, method_expr] = items else {
        return None;
    };
    let ExprKind::StringLiteral(method) = &method_expr.kind else {
        return None;
    };
    resolve_instance_method_callable(ctx, object, method.clone(), true)
}

/// Resolves the named static receiver and method from a static callable array literal.
pub(super) fn static_array_callable_parts(
    ctx: &LoweringContext<'_, '_>,
    items: &[Expr],
) -> Option<(StaticReceiver, String)> {
    let [class_expr, method_expr] = items else {
        return None;
    };
    let class_name = static_callable_class_name(ctx, class_expr)?;
    let ExprKind::StringLiteral(method) = &method_expr.kind else {
        return None;
    };
    let class_name = lookup_folded_name(ctx.classes.keys(), class_name.trim_start_matches('\\'))?;
    let receiver = StaticReceiver::Named(Name::from(class_name));
    static_method_implementation_signature(ctx, &receiver, method)?;
    Some((receiver, method.clone()))
}

/// Extracts a compile-time class name for a static callable array.
pub(super) fn static_callable_class_name(
    ctx: &LoweringContext<'_, '_>,
    class_expr: &Expr,
) -> Option<String> {
    match &class_expr.kind {
        ExprKind::StringLiteral(name) => Some(name.clone()),
        ExprKind::ClassConstant { receiver } => static_receiver_class_name(ctx, receiver),
        _ => None,
    }
}

/// Returns the static `is_callable()` result for a literal static-method callback array.
pub(super) fn static_array_callable_is_callable(
    ctx: &LoweringContext<'_, '_>,
    items: &[Expr],
) -> Option<bool> {
    let [class_expr, method_expr] = items else {
        return None;
    };
    let ExprKind::StringLiteral(method) = &method_expr.kind else {
        return None;
    };
    if !crate::codegen_support::callable_dispatch::runtime_method_callable_visible(method) {
        return Some(false);
    }
    let class_name = static_callable_class_name(ctx, class_expr)?;
    Some(static_method_callback_is_callable(ctx, &class_name, method))
}

/// Returns true when a compile-time class/method pair names a public static method.
pub(super) fn static_method_callback_is_callable(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    method: &str,
) -> bool {
    let Some(class_name) = lookup_folded_name(ctx.classes.keys(), class_name.trim_start_matches('\\')) else {
        return false;
    };
    let Some(class_info) = ctx.classes.get(&class_name) else {
        return false;
    };
    let method_key = php_symbol_key(method);
    if !class_info.static_methods.contains_key(&method_key) {
        return false;
    }
    class_info.static_method_visibilities.get(&method_key) == Some(&Visibility::Public)
}

/// Converts a static `call_user_func_array()` argument array into call arguments.
pub(super) fn static_call_user_func_array_args(arg_array: &Expr) -> Option<Vec<Expr>> {
    match &arg_array.kind {
        ExprKind::ArrayLiteral(items) => Some(items.clone()),
        ExprKind::ArrayLiteralAssoc(pairs) => static_call_user_func_array_assoc_args(pairs),
        _ => None,
    }
}

/// Converts literal associative callback arrays into positional or named call args.
pub(super) fn static_call_user_func_array_assoc_args(pairs: &[(Expr, Expr)]) -> Option<Vec<Expr>> {
    let mut args = Vec::with_capacity(pairs.len());
    for (key, value) in pairs {
        match &key.kind {
            ExprKind::StringLiteral(name) => {
                args.push(Expr::new(
                    ExprKind::NamedArg {
                        name: name.clone(),
                        value: Box::new(value.clone()),
                    },
                    value.span,
                ));
            }
            ExprKind::IntLiteral(_) => args.push(value.clone()),
            _ => return None,
        }
    }
    Some(args)
}
