//! Purpose:
//! Core instance-method dispatch and Closure rebinding methods.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers an object method call.
pub(super) fn lower_method_call(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    method: &str,
    args: &[Expr],
    op: Op,
    expr: &Expr,
) -> LoweredValue {
    // A statically-decided private/protected method access from an inaccessible
    // scope raises a catchable `Error` in PHP rather than a compile-time error,
    // but the receiver expression must still be evaluated first.
    let throw_access_message = if op == Op::MethodCall {
        ctx.throw_access_sites.get(&expr.span).and_then(|info| {
            if let ThrowAccessKind::PrivateMethod {
                visibility,
                class_name,
                method: m,
            } = &info.kind
            {
                Some(format!(
                    "Call to {} method {}::{}() from global scope",
                    visibility, class_name, m
                ))
            } else {
                None
            }
        })
    } else {
        None
    };
    let object_expr = object;
    let object = lower_member_receiver(ctx, object_expr, expr);
    if op == Op::MethodCall && args.is_empty()
        && ctx.classes.values().any(|class| class.deferred_property_default_error.is_some())
        && matches!(php_symbol_key(method).as_str(),
            "getdefaultproperties" | "getstaticproperties" | "newinstancewithoutconstructor")
    {
        if is_reflection_class_construction_receiver(ctx, object.value) {
            let owns_receiver = ctx.value_is_owning_temporary(object)
                && !ctx.value_is_owned_unboxed_local_load(object.value);
            if owns_receiver {
                ctx.begin_argument_guard_scope();
                ctx.guard_call_argument(object, 0, expr.span);
                ctx.end_argument_guard_scope();
            }
            // Inspect metadata without consuming an inline reflector needed by the real call.
            let borrowed = ctx.emit_value(Op::Borrow, vec![object.value], None,
                ctx.builder.value_php_type(object.value).clone(), Op::Borrow.default_effects(),
                Some(expr.span));
            let name = lower_property_get_from_value(ctx, borrowed, "__name", Op::PropGet, expr);
            crate::ir_lower::property_default_errors::for_dynamic_class(ctx, name, expr.span);
            if owns_receiver { ctx.unguard_call_argument(object.value, expr.span); }
        }
    }
    if let Some(message) = throw_access_message {
        release_owning_receiver_temporary(ctx, object, expr.span);
        return crate::ir_lower::stmt::lower_throw_access_error_expr(ctx, &message, expr.span);
    }
    if op == Op::MethodCall && value_is_definitely_null(ctx, object.value) {
        let null_value = lower_null(ctx, expr);
        terminate_method_call_on_null(ctx, method);
        return null_value;
    }
    if op == Op::MethodCall {
        if let Some(value) =
            lower_reflection_function_invoke_call(ctx, Some(object_expr), method, args, expr)
        {
            return value;
        }
        if let Some(value) =
            lower_reflection_method_invoke_call(ctx, Some(object_expr), method, args, expr)
        {
            return value;
        }
    }
    if op == Op::MethodCall
        && (value_is_nullable(ctx, object.value)
            || value_may_carry_container_miss(ctx, object.value))
    {
        return lower_nullable_regular_method_call(ctx, object, method, args, expr);
    }
    if op == Op::MethodCall && is_reflection_class_new_instance_call(ctx, object.value, method) {
        return lower_reflection_class_new_instance(ctx, Some(object_expr), object, args, expr);
    }
    if op == Op::MethodCall && is_reflection_class_new_instance_args_call(ctx, object.value, method)
    {
        return lower_reflection_class_new_instance_args(
            ctx,
            Some(object_expr),
            object,
            args,
            expr,
        );
    }
    if op == Op::MethodCall
        && is_reflection_class_new_instance_without_constructor_call(ctx, object.value, method)
    {
        return lower_reflection_class_new_instance_without_constructor(ctx, object, args, expr);
    }
    if op == Op::MethodCall {
        if let Some(value) = lower_reflection_class_static_property_value_call(
            ctx,
            Some(object_expr),
            method,
            args,
            expr,
        ) {
            return value;
        }
    }
    if op == Op::MethodCall {
        if let Some(value) =
            lower_reflection_class_member_list_call(ctx, Some(object_expr), method, args, expr)
        {
            return value;
        }
    }
    if op == Op::MethodCall && reflection_property_reflected_target(ctx, object_expr).is_some() {
        // Specialized property calls still evaluate a reflector receiver. Keep its
        // independent owner through argument/guard throws and retire it on success.
        let pins = pin_in_flight_owners(ctx, &[object.value], expr.span);
        let result = lower_reflection_property_value_call(ctx, Some(object_expr), method, args, expr);
        unpin_in_flight_owners(ctx, pins, expr.span);
        if let Some(value) = result {
            return stabilize_borrowed_result_and_release_receiver(ctx, object, value, expr.span);
        }
    }
    if matches!(
        ctx.builder.value_php_type(object.value).codegen_repr(),
        PhpType::Callable
    ) {
        if let Some(result) =
            lower_closure_bind_method(ctx, object_expr, &object, method, args, expr)
        {
            return result;
        }
    }
    if let Some(call) = enum_interface_calls::lower_enum_interface_call(ctx, object, method, args, op, expr) {
        return call;
    }
    let magic_args;
    let (dispatch_method, args) = if let Some(args) =
        magic_call_dispatch_args(ctx, object.value, method, args, object_expr.span)
    {
        magic_args = args;
        ("__call", magic_args.as_slice())
    } else {
        (method, args)
    };
    let result_type = method_call_result_type(ctx, object.value, dispatch_method, op, expr);
    let sig = method_call_argument_signature(ctx, object_expr, object.value, dispatch_method);
    promote_pdo_binding_ref_argument(ctx, object.value, dispatch_method, args);
    // A by-reference-returning method transfers a lease that must outlive this caller's
    // argument and receiver cleanup, so its staging is published before the arguments.
    let reference_staging = begin_reference_return_call(ctx, sig.as_ref(), expr.span);
    // An ordinary owned result needs the same protection: retiring the argument roots, the
    // evaluation intermediates and an owning receiver all run PHP destructors that can throw,
    // and the result is only an SSA temporary until this staging holds it. Both the result type
    // and the alias summary are resolved from the receiver and the method alone, so they are
    // available here, before any argument expression runs.
    let return_alias = method_return_arg_alias(ctx, object.value, dispatch_method);
    let result_staging = prepublish_user_call_result(
        ctx, sig.as_ref(), &return_alias, &result_type, expr.span,
    );
    let receiver_class = match ctx.builder.value_php_type(object.value).codegen_repr() {
        PhpType::Object(class_name) => Some(class_name.trim_start_matches('\\').to_string()),
        _ => None,
    };
    // A concrete local load may later unbox physical Mixed storage after eval widens
    // its slot. Acquiring a root is safe for both borrowed and independently owned loads.
    let (object, receiver_owner) = root_method_receiver(ctx, object, expr.span);
    let mut operands = vec![object.value];
    begin_call_argument_evaluation(ctx);
    let call_description = receiver_class
        .map(|class_name| format!("{class_name}::{dispatch_method}"))
        .unwrap_or_else(|| dispatch_method.to_string());
    let previous_description = ctx.call_description.replace(call_description);
    let ref_place_rewrite = sig.as_ref().and_then(|signature| {
        super::ref_place_args::rewrite_typed_property_ref_args(ctx, signature, args)
    });
    let call_args = ref_place_rewrite
        .as_ref()
        .map_or(args, |rewrite| rewrite.args.as_slice());
    let arg_values = lower_args_with_signature(ctx, sig.as_ref(), call_args);
    ctx.call_description = previous_description;
    let mut arg_values = arg_values;
    if method_is_fiber_start(ctx, object.value, dispatch_method) {
        for argument in &mut arg_values {
            let omitted = ctx.builder.value_defining_instruction(*argument).is_some_and(|inst| {
                inst.op == Op::ConstNull && inst.span.is_some_and(|span| span.line == 0 && span.col == 0)
            });
            if omitted { continue; }
            // The native start buffer borrows Mixed cells. Publish boxing in EIR
            // so ordinary argument cleanup owns it on return, suspension and throw.
            let value = lowered_value_from_id(ctx, *argument);
            let boxed = coerce_value_for_temp(ctx, value, &PhpType::Mixed, expr.span);
            *argument = root_evaluated_call_argument(ctx, boxed, expr.span).value;
        }
    }
    let evaluation_intermediates = finish_call_argument_evaluation(ctx, &mut arg_values);
    let roots = root_user_call_operands(
        ctx,
        &mut arg_values,
        sig.as_ref(),
        &return_alias,
        &result_type,
        expr.span,
    );
    operands.extend(arg_values.iter().copied());
    let data = ctx.intern_string(dispatch_method);
    let call = ctx.emit_value(
        op,
        operands,
        Some(Immediate::Data(data)),
        result_type,
        op.default_effects(),
        Some(expr.span),
    );
    ctx.invalidate_callable_ref_argument_locals(sig.as_ref(), &arg_values);
    let call = finish_reference_return_call(
        ctx, call, sig.as_ref(), reference_staging.as_ref(), expr.span,
    );
    stage_call_result(ctx, result_staging.as_ref(), call, expr.span);
    release_owned_call_arg_temporaries_with_roots(
        ctx,
        &arg_values,
        Some(call.value),
        &return_alias,
        sig.as_ref(),
        &roots,
        expr.span,
    );
    retire_call_argument_intermediates(ctx, &evaluation_intermediates);
    if let Some(rewrite) = ref_place_rewrite {
        super::ref_place_args::finish_ref_place_rewrite(ctx, rewrite, expr.span);
    }
    if let Some(owner) = receiver_owner {
        retire_owned_call_operand(ctx, owner, expr.span);
    } else {
        release_owning_receiver_temporary(ctx, object, expr.span);
    }
    let call = take_prepublished_call_result(ctx, result_staging, call, expr.span);
    finish_reference_return_value(ctx, call, reference_staging, expr.span)
}

/// Builds synthetic `__call` arguments when a class lacks the requested method.
pub(super) fn magic_call_dispatch_args(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    method: &str,
    args: &[Expr],
    span: Span,
) -> Option<Vec<Expr>> {
    if method_signature(ctx, object, method).is_some() {
        return None;
    }
    let object_ty = ctx.builder.value_php_type(object);
    let Some((class_name, _)) = singular_object_class(&object_ty) else {
        return None;
    };
    let normalized = class_name.trim_start_matches('\\');
    class_method_signature(ctx, normalized, &php_symbol_key("__call"))?;
    Some(vec![
        Expr::new(ExprKind::StringLiteral(method.to_string()), span),
        Expr::new(ExprKind::ArrayLiteral(args.to_vec()), span),
    ])
}

/// Returns the signature to use for method-call argument normalization.
pub(super) fn method_call_argument_signature(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
    object: crate::ir::ValueId,
    method: &str,
) -> Option<FunctionSig> {
    if method_is_fiber_start(ctx, object, method) {
        return crate::ir_lower::fibers::start_sig_for_expr(ctx, object_expr);
    }
    method_signature(ctx, object, method)
}

/// Returns true when a method call targets PHP's built-in `Fiber::start()`.
pub(super) fn method_is_fiber_start(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    method: &str,
) -> bool {
    if php_symbol_key(method) != "start" {
        return false;
    }
    let object_ty = ctx.builder.value_php_type(object);
    let Some((class_name, _)) = singular_object_class(&object_ty) else {
        return false;
    };
    php_symbol_key(class_name.trim_start_matches('\\')) == "fiber"
}

/// Lowers `?Object->method()` calls so null receivers fatal before argument evaluation.
pub(super) fn lower_nullable_regular_method_call(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let result_type = method_call_result_type(ctx, object.value, method, Op::MethodCall, expr);
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
    let fatal_block = ctx
        .builder
        .create_named_block("method.null.fatal", Vec::new());
    let call_block = ctx
        .builder
        .create_named_block("method.non_null.call", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("method.nullable.merge", Vec::new());
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
        then_target: fatal_block,
        then_args: Vec::new(),
        else_target: call_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(fatal_block);
    terminate_method_call_on_null(ctx, method);

    ctx.builder.position_at_end(call_block);
    let call = lower_method_call_with_receiver(ctx, object, method, args, Op::MethodCall, expr);
    store_value_into_temp(ctx, &temp_name, result_type.clone(), call, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, expr.span)
}
