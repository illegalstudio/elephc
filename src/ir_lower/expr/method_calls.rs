//! Purpose:
//! Core instance-method dispatch and Closure rebinding methods.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Returns whether an inline `ReflectionClass` receiver has no observable construction effects.
///
/// Known literal class reflectors may be skipped when a member-list call is rebuilt directly
/// from compile-time metadata. `ReflectionObject` must still be materialized because its
/// DateInterval property surface depends on live instance state.
fn reflection_class_inline_owner_can_be_elided(object_expr: &Expr) -> bool {
    matches!(
        &object_expr.kind,
        ExprKind::NewObject { class_name, .. }
            if php_symbol_key(class_name.as_str().trim_start_matches('\\')) == "reflectionclass"
    )
}

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
    if op == Op::MethodCall && reflection_class_inline_owner_can_be_elided(object_expr) {
        if let Some(value) =
            lower_reflection_class_member_list_call(ctx, Some(object_expr), method, args, expr)
        {
            return value;
        }
    }
    let object = lower_expr(ctx, object_expr);
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
        return lower_reflection_class_new_instance_without_constructor(
            ctx,
            Some(object_expr),
            object,
            args,
            expr,
        );
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
    if op == Op::MethodCall {
        if let Some(value) =
            lower_reflection_property_value_call(ctx, Some(object_expr), method, args, expr)
        {
            return value;
        }
    }
    if matches!(
        ctx.builder.value_php_type(object.value).codegen_repr(),
        PhpType::Callable
    ) {
        if let Some(result) = lower_closure_bind_method(ctx, &object, method, args, expr) {
            return result;
        }
    }
    let receiver_type = ctx.builder.value_php_type(object.value);
    if op == Op::MethodCall
        && php_symbol_key(method) == "format"
        && ctx.owner_name().ends_with("::__construct")
        && matches!(object_expr.kind, ExprKind::This)
        && matches!(
            &receiver_type,
            PhpType::Object(class_name)
                if !class_name.trim_start_matches('\\').eq_ignore_ascii_case("DateTimeInterface")
        )
        && is_datetime_family_value(ctx, object.value)
    {
        guard_constructor_datetime_format(ctx, object.value, expr.span);
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
    let mut operands = vec![object.value];
    let sig = method_call_argument_signature(ctx, object_expr, object.value, dispatch_method);
    promote_pdo_binding_ref_argument(ctx, object.value, dispatch_method, args);
    let mut arg_values = lower_args_with_signature(ctx, sig.as_ref(), args);
    coerce_datetime_method_arguments(ctx, object.value, dispatch_method, &mut arg_values, expr.span);
    validate_datetime_method_arguments(
        ctx,
        object.value,
        dispatch_method,
        &arg_values,
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
    let return_alias = method_return_arg_alias(ctx, object.value, dispatch_method);
    release_owned_call_arg_temporaries_with_signature(
        ctx,
        &arg_values,
        Some(call.value),
        &return_alias,
        sig.as_ref(),
        expr.span,
    );
    release_owning_receiver_temporary(ctx, object, expr.span);
    call
}

/// Throws php-src's uninitialized DateObjectError at a constructor's `$this->format()` callsite.
fn guard_constructor_datetime_format(
    ctx: &mut LoweringContext<'_, '_>,
    object: ValueId,
    span: Span,
) {
    let property = ctx.intern_string("__elephc_initialized");
    let initialized = ctx.emit_value(
        Op::PropGet,
        vec![object],
        Some(Immediate::Data(property)),
        PhpType::Bool,
        Op::PropGet.default_effects(),
        Some(span),
    );
    let valid = ctx
        .builder
        .create_named_block("date.constructor.format.valid", Vec::new());
    let invalid = ctx
        .builder
        .create_named_block("date.constructor.format.invalid", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: initialized.value,
        then_target: valid,
        then_args: Vec::new(),
        else_target: invalid,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(invalid);
    let class_name = ctx.current_class.as_deref().unwrap_or("DateTime");
    let builtin_name = if class_extends_class(ctx, class_name, "DateTimeImmutable") {
        "DateTimeImmutable"
    } else {
        "DateTime"
    };
    let inheritance = if class_name
        .trim_start_matches('\\')
        .eq_ignore_ascii_case(builtin_name)
    {
        String::new()
    } else {
        format!(" (inheriting {builtin_name})")
    };
    let message = format!(
        "Object of type {class_name}{inheritance} has not been correctly initialized by calling parent::__construct() in its constructor"
    );
    emit_exception_and_terminate(ctx, "DateObjectError", &message, span);
    ctx.builder.position_at_end(valid);
}

/// Applies weak scalar coercion required by selected ext/date method signatures.
pub(super) fn coerce_datetime_method_arguments(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: ValueId,
    method: &str,
    arguments: &mut [ValueId],
    span: Span,
) {
    match php_symbol_key(method).as_str() {
        "settimestamp" if is_datetime_family_value(ctx, receiver) => {
            let Some(value) = arguments.first_mut() else {
                return;
            };
            let lowered = LoweredValue {
                value: *value,
                ir_type: ctx.builder.value_type(*value),
            };
            *value = coerce_to_int_at_span(ctx, lowered, Some(span)).value;
        }
        "gettransitions" if is_datetime_zone_family_value(ctx, receiver) => {
            for value in arguments.iter_mut().take(2) {
                let lowered = LoweredValue {
                    value: *value,
                    ir_type: ctx.builder.value_type(*value),
                };
                *value = coerce_to_int_at_span(ctx, lowered, Some(span)).value;
            }
        }
        _ => {}
    }
}

/// Guards ext/date object arguments before direct method ABI materialization can read bad bits.
fn validate_datetime_method_arguments(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: ValueId,
    method: &str,
    arguments: &[ValueId],
    span: Span,
) {
    if php_symbol_key(method) != "getoffset" || !is_datetime_zone_family_value(ctx, receiver) {
        return;
    }
    let Some(datetime) = arguments.first().copied() else {
        return;
    };
    emit_runtime_named_object_argument_guard(
        ctx,
        datetime,
        "DateTimeInterface",
        "DateTimeZone::getOffset(): Argument #1 ($datetime) must be of type DateTimeInterface, ",
        span,
    );
}

/// Accepts one runtime object family or throws a php-src-style TypeError with the actual type.
fn emit_runtime_named_object_argument_guard(
    ctx: &mut LoweringContext<'_, '_>,
    value: ValueId,
    expected_class: &str,
    message_prefix: &str,
    span: Span,
) {
    let class_data = ctx.intern_class_name(expected_class);
    let matches = ctx.emit_value(
        Op::InstanceOf,
        vec![value],
        Some(Immediate::Data(class_data)),
        PhpType::Bool,
        Op::InstanceOf.default_effects(),
        Some(span),
    );
    let valid = ctx
        .builder
        .create_named_block("date.method.arg.valid", Vec::new());
    let invalid = ctx
        .builder
        .create_named_block("date.method.arg.invalid", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: matches.value,
        then_target: valid,
        then_args: Vec::new(),
        else_target: invalid,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(invalid);
    emit_runtime_argument_type_error_and_terminate(ctx, value, message_prefix, span);
    ctx.builder.position_at_end(valid);
}

/// Lowers the `Closure` rebinding methods on a closure (`Callable`) receiver:
/// `$closure->bindTo($newThis [, $scope])` and `$closure->call($newThis, ...$args)`.
/// Returns `None` for any other method so normal dispatch (and its diagnostics)
/// still apply. The `$scope` argument is accepted and ignored — visibility is
/// resolved at compile time in elephc's closed-world model.
pub(super) fn lower_closure_bind_method(
    ctx: &mut LoweringContext<'_, '_>,
    closure: &LoweredValue,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    match php_symbol_key(method).as_str() {
        "bindto" => {
            let new_this = match args.first() {
                Some(arg) => lower_expr(ctx, arg),
                None => lower_null(ctx, expr),
            };
            Some(emit_closure_bind(ctx, closure.value, new_this.value, expr))
        }
        "call" => {
            // `$closure->call($newThis, ...$args)`: bind `$this` then invoke the
            // bound closure with the remaining arguments in one step.
            let new_this = match args.first() {
                Some(arg) => lower_expr(ctx, arg),
                None => lower_null(ctx, expr),
            };
            let bound = emit_closure_bind(ctx, closure.value, new_this.value, expr);
            let call_args = &args[args.len().min(1)..];
            let arg_container =
                lower_untyped_descriptor_invoker_arg_container(ctx, call_args, expr.span)?;
            Some(ctx.emit_value(
                Op::CallableDescriptorInvoke,
                vec![bound.value, arg_container.value],
                callable_profile_immediate(),
                PhpType::Mixed,
                Op::CallableDescriptorInvoke.default_effects(),
                Some(expr.span),
            ))
        }
        _ => None,
    }
}

/// Emits the `closure_bind` runtime call that rebinds a closure's captured
/// `$this`, yielding a new closure (`Callable`) descriptor.
pub(super) fn emit_closure_bind(
    ctx: &mut LoweringContext<'_, '_>,
    closure: crate::ir::ValueId,
    new_this: crate::ir::ValueId,
    expr: &Expr,
) -> LoweredValue {
    ctx.emit_value(
        Op::ClosureBind,
        vec![closure, new_this],
        None,
        PhpType::Callable,
        Op::ClosureBind.default_effects(),
        Some(expr.span),
    )
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
