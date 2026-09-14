//! Purpose:
//! Callable descriptor invocation and signature resolution.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers `call_user_func*` for receiver-bound first-class callables through `expr_call`.
pub(super) fn lower_instance_callable_call_user_func(
    ctx: &mut LoweringContext<'_, '_>,
    callback_expr: &Expr,
    callback: StaticCallableBinding,
    callback_args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let result_type = static_callable_return_type(ctx, &callback);
    let signature = instance_callable_signature(&callback).cloned();
    let mut operands = vec![lower_expr(ctx, callback_expr).value];
    operands.extend(lower_args_with_signature(ctx, signature.as_ref(), callback_args));
    Some(ctx.emit_value(
        Op::ExprCall,
        operands,
        callable_profile_immediate(),
        result_type,
        Op::ExprCall.default_effects(),
        Some(expr.span),
    ))
}

/// Lowers dynamic `call_user_func()` callbacks through descriptor invocation.
pub(super) fn lower_dynamic_call_user_func(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "call_user_func" || args.is_empty() {
        return None;
    }
    if matches!(args[0].kind, ExprKind::NamedArg { .. } | ExprKind::Spread(_)) {
        return None;
    }
    let signature = callable_descriptor_signature_for_expr(ctx, &args[0]);
    let callback = lower_expr(ctx, &args[0]);
    if descriptor_callback_php_type_supported(&ctx.builder.value_php_type(callback.value).codegen_repr()) {
        return lower_call_user_func_descriptor_invoke_from_value(
            ctx,
            callback,
            &args[1..],
            signature.as_ref(),
            expr,
        );
    }
    if crate::types::call_args::has_named_args(&args[1..]) || args[1..].iter().any(is_spread_arg) {
        return None;
    }
    let mut operands = Vec::with_capacity(args.len());
    operands.push(callback.value);
    operands.extend(lower_args(ctx, &args[1..]));
    Some(ctx.emit_value(
        Op::ExprCall,
        operands,
        callable_profile_immediate(),
        PhpType::Mixed,
        Op::ExprCall.default_effects(),
        Some(expr.span),
    ))
}

/// Lowers dynamic `call_user_func_array()` through the descriptor-invoker EIR path.
pub(super) fn lower_dynamic_call_user_func_array(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "call_user_func_array" {
        return None;
    }
    let [callback_expr, arg_array_expr] = args else {
        return None;
    };
    if crate::types::call_args::has_named_args(args) || args.iter().any(is_spread_arg) {
        return None;
    }
    let signature = callable_descriptor_signature_for_expr(ctx, callback_expr);
    let callback = lower_expr(ctx, callback_expr);
    guard_owned_descriptor_callback(ctx, callback, expr.span);
    let arg_array = lower_descriptor_invoker_arg_array_for_call_user_func_array(
        ctx,
        arg_array_expr,
        signature.as_ref(),
    )
    .unwrap_or_else(|| lower_expr(ctx, arg_array_expr));
    Some(emit_callable_descriptor_invoke(
        ctx,
        callback,
        arg_array,
        PhpType::Mixed,
        expr.span,
    ))
}

/// Returns the callable signature available to descriptor-invoker argument lowering.
pub(super) fn callable_descriptor_signature_for_expr(
    ctx: &LoweringContext<'_, '_>,
    callback: &Expr,
) -> Option<FunctionSig> {
    match &callback.kind {
        ExprKind::Ternary { then_expr, else_expr, .. } => {
            let left = callable_descriptor_signature_for_expr(ctx, then_expr)?;
            let right = callable_descriptor_signature_for_expr(ctx, else_expr)?;
            compatible_descriptor_signature(left, &right)
        }
        ExprKind::ShortTernary { value, default } => {
            let left = callable_descriptor_signature_for_expr(ctx, value)?;
            let right = callable_descriptor_signature_for_expr(ctx, default)?;
            compatible_descriptor_signature(left, &right)
        }
        ExprKind::Variable(name) => ctx
            .callable_param_signature(name)
            .cloned()
            .or_else(|| ctx.static_callable_local(name).and_then(|target| {
                signature_for_static_callable_binding(ctx, target)
            })),
        _ => static_callable_binding_for_expr(ctx, callback)
            .and_then(|target| signature_for_static_callable_binding(ctx, target))
            .or_else(|| invokable_object_signature_for_expr(ctx, callback)),
    }
}

/// Returns the `__invoke` signature for an invokable object callback expression.
pub(super) fn invokable_object_signature_for_expr(
    ctx: &LoweringContext<'_, '_>,
    callback: &Expr,
) -> Option<FunctionSig> {
    let class_name = instance_callable_object_class(ctx, callback)?;
    class_method_signature(ctx, &class_name, "__invoke").cloned()
}

/// Keeps a descriptor signature only when two runtime branches have the same callable ABI.
pub(super) fn compatible_descriptor_signature(left: FunctionSig, right: &FunctionSig) -> Option<FunctionSig> {
    (left == *right).then_some(left)
}

/// Extracts a callable signature from a statically understood callable binding.
pub(super) fn signature_for_static_callable_binding(
    ctx: &LoweringContext<'_, '_>,
    target: StaticCallableBinding,
) -> Option<FunctionSig> {
    match target {
        StaticCallableBinding::UserFunction(name) => ctx.functions.get(&name).cloned(),
        StaticCallableBinding::ExternFunction(name) => ctx
            .extern_functions
            .get(&name)
            .map(function_sig_from_extern_for_descriptor),
        StaticCallableBinding::Builtin(_) => None,
        StaticCallableBinding::Closure { signature, .. } => Some(signature),
        StaticCallableBinding::StaticMethod { receiver, method }
        | StaticCallableBinding::StaticMethodDescriptor { receiver, method } => {
            static_method_implementation_signature(ctx, &receiver, &method).cloned()
        }
        StaticCallableBinding::InstanceMethod { signature, .. } => Some(signature),
    }
}

/// Converts an extern signature into the PHP-facing descriptor invoker signature.
pub(super) fn function_sig_from_extern_for_descriptor(sig: &ExternFunctionSig) -> FunctionSig {
    FunctionSig {
        params: sig.params.clone(),
        param_type_exprs: vec![None; sig.params.len()],
        param_attributes: Vec::new(),
        defaults: vec![None; sig.params.len()],
        return_type: sig.return_type.clone(),
        declared_return: true,
        by_ref_return: false,
        ref_params: vec![false; sig.params.len()],
        declared_params: vec![true; sig.params.len()],
        variadic: None,
        deprecation: None,
    }
}

/// Builds an invoker argument array that preserves by-reference literal variables.
pub(super) fn lower_descriptor_invoker_arg_array_for_call_user_func_array(
    ctx: &mut LoweringContext<'_, '_>,
    arg_array: &Expr,
    sig: Option<&FunctionSig>,
) -> Option<LoweredValue> {
    if let ExprKind::ArrayLiteralAssoc(pairs) = &arg_array.kind {
        return Some(lower_assoc_array_literal_with_guard(ctx, pairs, arg_array, true));
    }
    let ExprKind::ArrayLiteral(items) = &arg_array.kind else {
        return None;
    };
    if items.iter().any(is_spread_arg) || !items.iter().enumerate().any(|(index, item)| {
        invoker_ref_arg_variable(ctx, sig, index, item).is_some()
    }) {
        return None;
    }

    let elem_ty = PhpType::Mixed;
    let array_ty = PhpType::Array(Box::new(elem_ty.clone()));
    let array = ctx.emit_value(
        Op::ArrayNew,
        Vec::new(),
        Some(Immediate::Capacity(items.len() as u32)),
        array_ty.clone(),
        Op::ArrayNew.default_effects(),
        Some(arg_array.span),
    );
    guard_descriptor_container(ctx, array, arg_array.span);
    for (index, item) in items.iter().enumerate() {
        let value = if let Some(var_name) = invoker_ref_arg_variable(ctx, sig, index, item) {
            lower_invoker_ref_arg_marker(ctx, var_name, item.span)
        } else {
            let value = lower_expr(ctx, item);
            coerce_variadic_tail_value(ctx, value, &array_ty, item.span)
        };
        ctx.emit_void(
            Op::ArrayPush,
            vec![array.value, value.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(item.span),
        );
        ctx.refresh_argument_array_guard(array, item.span);
        crate::ir_lower::stmt::release_indexed_array_write_operand(ctx, Some(&elem_ty), value, item.span);
    }
    Some(array)
}

/// Returns true when `call_user_func()` must keep runtime descriptor semantics.
pub(super) fn call_user_func_should_use_descriptor(
    ctx: &LoweringContext<'_, '_>,
    callback: &Expr,
    args: &[Expr],
    sig: Option<&FunctionSig>,
) -> bool {
    let has_named_or_spread =
        crate::types::call_args::has_named_args(args) || args.iter().any(is_spread_arg);
    if has_named_or_spread {
        return true;
    }
    if call_user_func_has_incompatible_ref_marker_arg(ctx, args, sig) {
        return false;
    }
    if sig.is_some_and(|sig| sig.ref_params.iter().any(|is_ref| *is_ref)) {
        return true;
    }
    match &callback.kind {
        ExprKind::ArrayLiteral(_)
        | ExprKind::ArrayLiteralAssoc(_)
        | ExprKind::Closure { .. }
        | ExprKind::NewObject { .. }
        | ExprKind::NewDynamicObject { .. }
        | ExprKind::Ternary { .. }
        | ExprKind::ShortTernary { .. }
        | ExprKind::FirstClassCallable(CallableTarget::Method { .. }) => true,
        ExprKind::Variable(name) => {
            if let Some(target) = ctx.static_callable_local(name) {
                return matches!(
                    target,
                    StaticCallableBinding::Closure { .. }
                        | StaticCallableBinding::StaticMethodDescriptor { .. }
                        | StaticCallableBinding::InstanceMethod { .. }
                );
            }
            matches!(
                ctx.local_type(name).codegen_repr(),
                PhpType::Callable | PhpType::Array(_) | PhpType::Object(_)
            )
        }
        _ => false,
    }
}

/// Returns true when direct descriptor ref markers cannot represent an argument.
pub(super) fn call_user_func_has_incompatible_ref_marker_arg(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
    sig: Option<&FunctionSig>,
) -> bool {
    let Some(sig) = sig else {
        return false;
    };
    args.iter().enumerate().any(|(index, arg)| {
        if !sig.ref_params.get(index).copied().unwrap_or(false) {
            return false;
        }
        let ExprKind::Variable(name) = &arg.kind else {
            return false;
        };
        !invoker_ref_arg_storage_compatible(ctx, sig, index, name)
    })
}

/// Lowers `call_user_func()` into a descriptor invoke when the callback value is supported.
pub(super) fn lower_call_user_func_descriptor_invoke(
    ctx: &mut LoweringContext<'_, '_>,
    callback_expr: &Expr,
    args: &[Expr],
    sig: Option<&FunctionSig>,
    expr: &Expr,
) -> Option<LoweredValue> {
    let callback = lower_expr(ctx, callback_expr);
    if !descriptor_callback_php_type_supported(&ctx.builder.value_php_type(callback.value).codegen_repr()) {
        return None;
    }
    lower_call_user_func_descriptor_invoke_from_value(ctx, callback, args, sig, expr)
}

/// Emits `CallableDescriptorInvoke` for an already evaluated `call_user_func()` callback.
pub(super) fn lower_call_user_func_descriptor_invoke_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    callback: LoweredValue,
    args: &[Expr],
    sig: Option<&FunctionSig>,
    expr: &Expr,
) -> Option<LoweredValue> {
    guard_owned_descriptor_callback(ctx, callback, expr.span);
    let arg_container = lower_descriptor_invoker_arg_container_for_call_user_func(ctx, args, sig, expr.span)?;
    let result_type = sig
        .map(|sig| normalize_value_php_type(sig.return_type.codegen_repr()))
        .unwrap_or(PhpType::Mixed);
    Some(emit_callable_descriptor_invoke(
        ctx,
        callback,
        arg_container,
        result_type,
        expr.span,
    ))
}

/// Protects an owned callback before argument evaluation can throw or replace its source property.
pub(super) fn guard_owned_descriptor_callback(
    ctx: &mut LoweringContext<'_, '_>, callback: LoweredValue, span: Span,
) {
    if ctx.value_is_owning_temporary(callback) && !ctx.has_call_argument_guard(callback.value) {
        guard_descriptor_container(ctx, callback, span);
    }
}

/// Invokes a descriptor and consumes callback/argument owners while protecting its result from cleanup throws.
pub(super) fn emit_callable_descriptor_invoke(
    ctx: &mut LoweringContext<'_, '_>,
    callback: LoweredValue,
    arg_container: LoweredValue,
    result_type: PhpType,
    span: Span,
) -> LoweredValue {
    let owns_callback = ctx.value_is_owning_temporary(callback);
    guard_owned_descriptor_callback(ctx, callback, span);
    let owns_arguments = ctx.value_is_owning_temporary(arg_container);
    if owns_arguments && !ctx.has_call_argument_guard(arg_container.value) {
        guard_descriptor_container(ctx, arg_container, span);
    }
    let result = ctx.emit_value(
        Op::CallableDescriptorInvoke,
        vec![callback.value, arg_container.value],
        callable_profile_immediate(),
        result_type,
        Op::CallableDescriptorInvoke.default_effects(),
        Some(span),
    );
    let guard_result = (owns_arguments || owns_callback) && ctx.value_is_owning_temporary(result);
    if guard_result {
        guard_descriptor_container(ctx, result, span);
    }
    if owns_arguments {
        ctx.unguard_call_argument(arg_container.value, span);
        crate::ir_lower::ownership::release_if_owned(ctx, arg_container, Some(span));
    }
    if owns_callback {
        ctx.unguard_call_argument(callback.value, span);
        crate::ir_lower::ownership::release_if_owned(ctx, callback, Some(span));
    }
    if guard_result {
        ctx.unguard_call_argument(result.value, span);
    }
    result
}
