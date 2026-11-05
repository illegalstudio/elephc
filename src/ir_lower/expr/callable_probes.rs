//! Purpose:
//! Static callable probes and eval-backed call_user_func fallbacks.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers direct function/static-method first-class callable probes for `is_callable()`.
pub(super) fn lower_static_is_callable(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "is_callable" || args.len() != 1 {
        return None;
    }
    if crate::types::call_args::has_named_args(args) || args.iter().any(is_spread_arg) {
        return None;
    }
    // Eval can declare callable targets after static metadata has been built.
    if ctx.has_eval_barrier() {
        return None;
    }
    // An array callable whose receiver is CData hits php-src's method-call fatal while
    // probing the method name; it does not merely return false. Only a two-element array
    // with a string method reaches that lookup.
    if let ExprKind::ArrayLiteral(items) = &args[0].kind {
        if let [receiver, method] = items.as_slice() {
            if is_string_expr(ctx, method) && is_ffi_cdata_expr(ctx, receiver) {
                // PHP evaluates both array elements before the method lookup fails.
                for value in [receiver, method] {
                    let value = lower_expr(ctx, value);
                    if ctx.value_is_owning_temporary(value) {
                        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(expr.span));
                    }
                }
                return Some(crate::ir_lower::stmt::lower_throw_access_error_expr_as(
                    ctx,
                    "Object of type FFI\\CData does not support method calls",
                    expr.span,
                    PhpType::Bool,
                ));
            }
            if is_string_expr(ctx, method)
                && may_hold_ffi_cdata(ctx, receiver)
                && ctx.classes.contains_key("FFI\\CData")
            {
                return Some(lower_mixed_cdata_array_is_callable(
                    ctx,
                    receiver,
                    method,
                    expr,
                ));
            }
        }
    }
    // php-src's `get_closure` handler makes a function-pointer CData callable while scalar and
    // non-function CData stay non-callable. The bridge owns that type check, so route the
    // statically-typed receiver through the prelude adapter instead of the generic runtime
    // object lookup (which only sees the public `__invoke` method).
    let ffi_cdata_argument = is_ffi_cdata_expr(ctx, &args[0]);
    if ffi_cdata_argument {
        let call = Expr::new(
            ExprKind::StaticMethodCall {
                receiver: StaticReceiver::Named(Name::from("FFI".to_string())),
                method: "__elephc_is_callable".to_string(),
                args: vec![args[0].clone()],
            },
            expr.span,
        );
        return Some(lower_expr(ctx, &call));
    }
    // A `mixed` slot can still hold a CData. php-src decides callability from the CData's
    // declared C type, not from the prelude's public `__invoke` method, so branch on the
    // runtime class before the generic object probe.
    if may_hold_ffi_cdata(ctx, &args[0]) && ctx.classes.contains_key("FFI\\CData") {
        let value = lower_expr(ctx, &args[0]);
        let value_ty = ctx.builder.value_php_type(value.value).codegen_repr();
        let value_local = ctx.declare_owned_hidden_temp(value_ty.clone());
        ctx.store_local(&value_local, value, value_ty, Some(expr.span));
        let class = ctx.intern_class_name("FFI\\CData");
        let probe = ctx.load_local(&value_local, Some(expr.span));
        let is_cdata = ctx.emit_value(
            Op::InstanceOf,
            vec![probe.value],
            Some(Immediate::Data(class)),
            PhpType::Bool,
            Op::InstanceOf.default_effects(),
            Some(expr.span),
        );
        let cdata_block = ctx.builder.create_named_block("is_callable.cdata", Vec::new());
        let generic_block = ctx.builder.create_named_block("is_callable.generic", Vec::new());
        let merge = ctx.builder.create_named_block("is_callable.merge", Vec::new());
        let temp = ctx.declare_owned_hidden_temp(PhpType::Bool);
        ctx.builder.terminate(Terminator::CondBr {
            cond: is_cdata.value,
            then_target: cdata_block,
            then_args: Vec::new(),
            else_target: generic_block,
            else_args: Vec::new(),
        });

        ctx.builder.position_at_end(cdata_block);
        let call = Expr::new(
            ExprKind::StaticMethodCall {
                receiver: StaticReceiver::Named(Name::from("FFI".to_string())),
                method: "__elephc_is_callable".to_string(),
                args: vec![Expr::new(ExprKind::Variable(value_local.clone()), expr.span)],
            },
            expr.span,
        );
        let cdata_result = lower_expr(ctx, &call);
        store_value_into_temp(ctx, &temp, PhpType::Bool, cdata_result, expr.span);
        branch_to(ctx, merge);

        ctx.builder.position_at_end(generic_block);
        let value = ctx.load_local(&value_local, Some(expr.span));
        let generic_result = ctx.emit_value(
            Op::RuntimeCall,
            vec![value.value],
            Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(
                crate::ir::RuntimeFnId::IsCallable,
            ))),
            PhpType::Bool,
            crate::ir_lower::effects_lookup::runtime_effects(),
            Some(expr.span),
        );
        store_value_into_temp(ctx, &temp, PhpType::Bool, generic_result, expr.span);
        branch_to(ctx, merge);

        ctx.builder.position_at_end(merge);
        return Some(take_owned_temp(ctx, &temp, expr.span));
    }
    match &args[0].kind {
        ExprKind::FirstClassCallable(
            CallableTarget::Function(_) | CallableTarget::StaticMethod { .. },
        ) => Some(emit_bool_literal(ctx, true, Some(expr.span))),
        ExprKind::ArrayLiteral(items) => {
            let is_callable = static_array_callable_is_callable(ctx, items)?;
            Some(emit_bool_literal(ctx, is_callable, Some(expr.span)))
        }
        ExprKind::Variable(name) => ctx.static_callable_local(name).map(|target| {
            emit_bool_literal(
                ctx,
                static_callable_binding_is_callable(ctx, &target),
                Some(expr.span),
            )
        }),
        _ => None,
    }
}

/// Returns the checker-visible PHP type of an expression when lowering knows it directly.
fn static_expr_type(ctx: &LoweringContext<'_, '_>, value: &Expr) -> Option<PhpType> {
    match &value.kind {
        ExprKind::Variable(name) => Some(ctx.local_type(name).clone()),
        ExprKind::FunctionCall { name, .. } => {
            ctx.functions.get(name.as_str()).map(|sig| sig.return_type.clone())
        }
        _ => None,
    }
}

/// Returns whether an expression is statically known to hold `FFI\CData`.
pub(super) fn is_ffi_cdata_expr(ctx: &LoweringContext<'_, '_>, value: &Expr) -> bool {
    matches!(
        static_expr_type(ctx, value)
            .unwrap_or_else(|| crate::ir_lower::expr::fallback_expr_type(value).codegen_repr()),
        PhpType::Object(name)
            if name.trim_start_matches('\\').eq_ignore_ascii_case("FFI\\CData")
    )
}

/// Returns whether an expression is statically known to be a string method name.
fn is_string_expr(ctx: &LoweringContext<'_, '_>, value: &Expr) -> bool {
    matches!(value.kind, ExprKind::StringLiteral(_))
        || matches!(
            static_expr_type(ctx, value)
                .unwrap_or_else(|| crate::ir_lower::expr::fallback_expr_type(value).codegen_repr()),
            PhpType::Str
        )
}

/// Returns whether a `mixed`/union slot can hold `FFI\CData` at run time.
pub(super) fn may_hold_ffi_cdata(ctx: &LoweringContext<'_, '_>, value: &Expr) -> bool {
    if let ExprKind::Variable(name) = &value.kind {
        if ctx.static_callable_local(name).is_some() {
            return false;
        }
    }
    matches!(
        static_expr_type(ctx, value)
            .unwrap_or_else(|| crate::ir_lower::expr::fallback_expr_type(value).codegen_repr()),
        PhpType::Mixed | PhpType::Union(_)
    )
}

/// Lowers `is_callable([$value, "method"])` when `$value` may hold CData.
///
/// The receiver is evaluated once, the runtime class decides between php-src's method-call
/// fatal and the ordinary array-callable probe, and the hidden temporary keeps evaluation
/// order intact.
fn lower_mixed_cdata_array_is_callable(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &Expr,
    method: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let value = lower_expr(ctx, receiver);
    let value_ty = ctx.builder.value_php_type(value.value).codegen_repr();
    let value_local = ctx.declare_owned_hidden_temp(value_ty.clone());
    ctx.store_local(&value_local, value, value_ty, Some(expr.span));
    let method_value = lower_expr(ctx, method);
    let method_ty = ctx.builder.value_php_type(method_value.value).codegen_repr();
    let method_local = ctx.declare_owned_hidden_temp(method_ty.clone());
    ctx.store_local(&method_local, method_value, method_ty, Some(expr.span));
    let class = ctx.intern_class_name("FFI\\CData");
    let probe = ctx.load_local(&value_local, Some(expr.span));
    let is_cdata = ctx.emit_value(
        Op::InstanceOf,
        vec![probe.value],
        Some(Immediate::Data(class)),
        PhpType::Bool,
        Op::InstanceOf.default_effects(),
        Some(expr.span),
    );
    let cdata_block = ctx.builder.create_named_block("is_callable.cdata_array", Vec::new());
    let generic_block = ctx.builder.create_named_block("is_callable.array", Vec::new());
    let merge = ctx.builder.create_named_block("is_callable.merge", Vec::new());
    let temp = ctx.declare_owned_hidden_temp(PhpType::Bool);
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_cdata.value,
        then_target: cdata_block,
        then_args: Vec::new(),
        else_target: generic_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(cdata_block);
    let thrown = crate::ir_lower::stmt::lower_throw_access_error_expr_as(
        ctx,
        "Object of type FFI\\CData does not support method calls",
        expr.span,
        PhpType::Bool,
    );
    store_value_into_temp(ctx, &temp, PhpType::Bool, thrown, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(generic_block);
    let array = Expr::new(
        ExprKind::ArrayLiteral(vec![
            Expr::new(ExprKind::Variable(value_local), expr.span),
            Expr::new(ExprKind::Variable(method_local), expr.span),
        ]),
        expr.span,
    );
    let array = lower_expr(ctx, &array);
    let result = ctx.emit_value(
        Op::RuntimeCall,
        vec![array.value],
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(
            crate::ir::RuntimeFnId::IsCallable,
        ))),
        PhpType::Bool,
        crate::ir_lower::effects_lookup::runtime_effects(),
        Some(expr.span),
    );
    store_value_into_temp(ctx, &temp, PhpType::Bool, result, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp, expr.span)
}

/// Returns whether straight-line callable-local metadata represents a public callable.
pub(super) fn static_callable_binding_is_callable(
    ctx: &LoweringContext<'_, '_>,
    target: &StaticCallableBinding,
) -> bool {
    match target {
        StaticCallableBinding::StaticMethod { receiver, method }
        | StaticCallableBinding::StaticMethodDescriptor { receiver, method } => {
            static_receiver_class_name(ctx, receiver)
                .is_some_and(|class_name| static_method_callback_is_callable(ctx, &class_name, method))
        }
        StaticCallableBinding::UserFunction(_)
        | StaticCallableBinding::ExternFunction(_)
        | StaticCallableBinding::Builtin(_)
        | StaticCallableBinding::Closure { .. }
        | StaticCallableBinding::InstanceMethod { .. } => true,
    }
}

/// Lowers static-string `call_user_func*` forms to direct call opcodes when possible.
pub(super) fn lower_static_call_user_func(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    match php_symbol_key(name.trim_start_matches('\\')).as_str() {
        "call_user_func" => {
            let callback_expr = args.first()?;
            let callback_args = &args[1..];
            if callback_args.is_empty() && is_static_get_defined_vars_callback(callback_expr) {
                return Some(lower_visible_defined_vars(ctx, expr));
            }
            if let Some(value) = lower_static_class_introspection_spread_call(
                ctx,
                callback_expr,
                callback_args,
                expr,
            ) {
                return Some(value);
            }
            let signature = callable_descriptor_signature_for_expr(ctx, callback_expr);
            if call_user_func_should_use_descriptor(ctx, callback_expr, callback_args, signature.as_ref()) {
                return lower_call_user_func_descriptor_invoke(
                    ctx,
                    callback_expr,
                    callback_args,
                    signature.as_ref(),
                    expr,
                );
            }
            if let Some(callback) = instance_call_user_func_callback(ctx, callback_expr) {
                return lower_instance_callable_call_user_func(
                    ctx,
                    callback_expr,
                    callback,
                    callback_args,
                    expr,
                );
            }
            if let Some(callback) = static_call_user_func_callback(ctx, callback_expr) {
                return lower_static_callable_call(ctx, callback, callback_args, expr);
            }
            lower_eval_call_user_func_fallback(ctx, callback_expr, callback_args, expr)
        }
        "call_user_func_array" => {
            let [callback_arg, arg_array] = args else {
                return None;
            };
            if is_static_get_defined_vars_callback(callback_arg)
                && static_call_user_func_array_args(arg_array).is_some_and(|args| args.is_empty())
            {
                return Some(lower_visible_defined_vars(ctx, expr));
            }
            let unpacked = Expr::new(ExprKind::Spread(Box::new(arg_array.clone())), arg_array.span);
            if let Some(value) = lower_static_class_introspection_spread_call(
                ctx, callback_arg, &[unpacked], expr,
            ) {
                return Some(value);
            }
            if matches!(arg_array.kind, ExprKind::ArrayLiteralAssoc(_))
                && static_callable_binding_for_expr(ctx, callback_arg)
                    .is_some_and(|target| matches!(target, StaticCallableBinding::InstanceMethod { .. }))
            {
                return None;
            }
            if let Some(callback_args) = static_call_user_func_array_args(arg_array) {
                if let Some(callback) = instance_call_user_func_callback(ctx, callback_arg) {
                    return lower_instance_callable_call_user_func(
                        ctx,
                        callback_arg,
                        callback,
                        &callback_args,
                        expr,
                    );
                }
                if let Some(callback) = static_call_user_func_callback(ctx, callback_arg) {
                    return lower_static_callable_call(ctx, callback, &callback_args, expr);
                }
            }
            let ffi_dynamic_callback = match &callback_arg.kind {
                ExprKind::DynamicPropertyAccess { object, .. } => match &object.kind {
                    ExprKind::Variable(receiver) => matches!(
                        ctx.local_type(receiver),
                        PhpType::Object(class)
                            if class.trim_start_matches('\\').eq_ignore_ascii_case("FFI")
                    ),
                    _ => false,
                },
                _ => false,
            };
            if ffi_dynamic_callback || matches!(
                crate::ir_lower::expr::fallback_expr_type(callback_arg).codegen_repr(),
                PhpType::Object(name) if name.trim_start_matches('\\').eq_ignore_ascii_case("FFI\\CData")
            ) {
                // Lower the FFI callback once into a hidden CData local, then invoke its
                // `__invoke` method directly. Re-reading the dynamic property inside a plain
                // `ExprCall` keeps the syntactic Mixed type and falls back to the descriptor
                // invoker.
                let callback = lower_expr(ctx, callback_arg);
                let callback_ty = ctx.builder.value_php_type(callback.value).codegen_repr();
                let temp = crate::names::generated_local_name(&format!(
                    "__elephc_ffi_callback_{}_{}",
                    expr.span.line, expr.span.col
                ));
                ctx.declare_local(&temp, callback_ty.clone());
                ctx.store_local(&temp, callback, callback_ty, Some(expr.span));
                let callee = Expr::new(ExprKind::Variable(temp), expr.span);
                let call = Expr::new(
                    ExprKind::StaticMethodCall {
                        receiver: StaticReceiver::Named(Name::from("FFI".to_string())),
                        method: "__elephc_invoke_array".to_string(),
                        args: vec![callee, arg_array.clone()],
                    },
                    expr.span,
                );
                return Some(lower_expr(ctx, &call));
            }
            lower_eval_call_user_func_array_fallback(ctx, callback_arg, arg_array, expr)
        }
        _ => None,
    }
}

/// Recognizes the PHP-permitted explicit callback spelling for `get_defined_vars()`.
fn is_static_get_defined_vars_callback(callback: &Expr) -> bool {
    let ExprKind::StringLiteral(name) = &callback.kind else {
        return false;
    };
    php_symbol_key(name.trim_start_matches('\\')) == "get_defined_vars"
}

/// Specializes statically selected class introspection before generic descriptor dispatch.
fn lower_static_class_introspection_spread_call(
    ctx: &mut LoweringContext<'_, '_>,
    callback_expr: &Expr,
    callback_args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let callback = static_call_user_func_callback(ctx, callback_expr)?;
    let StaticCallableBinding::Builtin(function_name) = &callback else {
        return None;
    };
    if !matches!(
        php_symbol_key(function_name.trim_start_matches('\\')).as_str(),
        "get_class_vars" | "get_class_methods"
    ) {
        return None;
    }
    lower_static_callable_call(ctx, callback, callback_args, expr)
}

/// Lowers unresolved string callbacks after an eval barrier through the eval function table.
pub(super) fn lower_eval_call_user_func_fallback(
    ctx: &mut LoweringContext<'_, '_>,
    callback_expr: &Expr,
    callback_args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if !ctx.has_eval_barrier() || !plain_positional_call_args(callback_args) {
        return None;
    }
    let ExprKind::StringLiteral(callback_name) = &callback_expr.kind else {
        return None;
    };
    if callback_name.contains("::")
        || resolve_static_string_callable(ctx, callback_name).is_some()
    {
        return None;
    }
    let dynamic_name = php_symbol_key(callback_name.trim_start_matches('\\'));
    let data = ctx.intern_function_name(&dynamic_name);
    let operands = lower_args(ctx, callback_args);
    Some(ctx.emit_value(
        Op::EvalFunctionCall,
        operands,
        Some(Immediate::Data(data)),
        PhpType::Mixed,
        Op::EvalFunctionCall.default_effects(),
        Some(expr.span),
    ))
}

/// Lowers unresolved `call_user_func_array()` string callbacks through the eval table.
pub(super) fn lower_eval_call_user_func_array_fallback(
    ctx: &mut LoweringContext<'_, '_>,
    callback_expr: &Expr,
    arg_array: &Expr,
    expr: &Expr,
) -> Option<LoweredValue> {
    if !ctx.has_eval_barrier() {
        return None;
    }
    let ExprKind::StringLiteral(callback_name) = &callback_expr.kind else {
        return None;
    };
    if callback_name.contains("::")
        || resolve_static_string_callable(ctx, callback_name).is_some()
    {
        return None;
    }
    let dynamic_name = php_symbol_key(callback_name.trim_start_matches('\\'));
    let data = ctx.intern_function_name(&dynamic_name);
    let arg_array = lower_expr(ctx, arg_array);
    let arg_array = coerce_eval_function_arg_array(ctx, arg_array, expr.span);
    Some(ctx.emit_value(
        Op::EvalFunctionCallArray,
        vec![arg_array.value],
        Some(Immediate::Data(data)),
        PhpType::Mixed,
        Op::EvalFunctionCallArray.default_effects(),
        Some(expr.span),
    ))
}

/// Boxes a post-barrier dynamic-call argument container for the eval bridge ABI.
pub(super) fn coerce_eval_function_arg_array(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    if matches!(
        ctx.builder.value_php_type(value.value).codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return value;
    }
    ctx.emit_value(
        Op::MixedBox,
        vec![value.value],
        None,
        PhpType::Mixed,
        Op::MixedBox.default_effects(),
        Some(span),
    )
}
