//! Purpose:
//! Dispatches named, invokable-object, and object-method callbacks.
//!
//! Called from:
//! - Callable execution after callback normalization.
//!
//! Key details:
//! - Native and eval object methods share call_user_func by-value handling.

use super::*;

/// Invokes a named callable through `call_user_func()` and warns for by-ref parameters.
pub(super) fn eval_named_callable_with_call_user_func_values(
    name: &str,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(result) = eval_builtin_with_values(name, &evaluated_args, context, values)? {
        return Ok(result);
    }
    if let Some(closure) = context.closure(name).cloned() {
        let evaluated_args = positional_args(evaluated_args);
        let parameter_is_by_ref = eval_call_user_func_by_value_ref_flags(
            closure.function().name(),
            closure.function().params(),
            closure.function().parameter_is_by_ref(),
            closure.function().parameter_is_variadic(),
            evaluated_args.len(),
            values,
        )?;
        return eval_closure_with_evaluated_args_and_bound_scope_ref_flags(
            &closure,
            None,
            &parameter_is_by_ref,
            evaluated_args,
            context,
            values,
        );
    }
    if let Some(function) = context.function(name).cloned() {
        let evaluated_args = positional_args(evaluated_args);
        let parameter_is_by_ref = eval_call_user_func_by_value_ref_flags(
            function.name(),
            function.params(),
            function.parameter_is_by_ref(),
            function.parameter_is_variadic(),
            evaluated_args.len(),
            values,
        )?;
        return eval_dynamic_function_with_evaluated_args_and_ref_flags(
            &function,
            &parameter_is_by_ref,
            evaluated_args,
            context,
            values,
        );
    }
    if let Some(function) = context.native_function(name) {
        let evaluated_args = positional_args(evaluated_args);
        let evaluated_args = bind_evaluated_native_function_args_for_call_user_func(
            name,
            &function,
            evaluated_args,
            context,
            values,
        )?;
        return eval_native_function_with_values(function, evaluated_args, context, values);
    }
    Err(EvalStatus::UnsupportedConstruct)
}

/// Invokes a named callable through by-value callable semantics with named args.
pub(super) fn eval_named_callable_with_call_user_func_args(
    name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args
        .iter()
        .all(|arg| arg.name.is_none() && arg.ref_target.is_none())
    {
        let evaluated_values = evaluated_args.into_iter().map(|arg| arg.value).collect();
        return eval_named_callable_with_call_user_func_values(
            name,
            evaluated_values,
            context,
            values,
        );
    }
    if let Some(closure) = context.closure(name).cloned() {
        let parameter_is_by_ref = eval_call_user_func_by_value_ref_flags(
            closure.function().name(),
            closure.function().params(),
            closure.function().parameter_is_by_ref(),
            closure.function().parameter_is_variadic(),
            evaluated_args.len(),
            values,
        )?;
        return eval_closure_with_evaluated_args_and_bound_scope_ref_flags(
            &closure,
            None,
            &parameter_is_by_ref,
            evaluated_args,
            context,
            values,
        );
    }
    if let Some(function) = context.function(name).cloned() {
        let parameter_is_by_ref = eval_call_user_func_by_value_ref_flags(
            function.name(),
            function.params(),
            function.parameter_is_by_ref(),
            function.parameter_is_variadic(),
            evaluated_args.len(),
            values,
        )?;
        return eval_dynamic_function_with_evaluated_args_and_ref_flags(
            &function,
            &parameter_is_by_ref,
            evaluated_args,
            context,
            values,
        );
    }
    if let Some(function) = context.native_function(name) {
        let evaluated_args = bind_evaluated_native_function_args_for_call_user_func(
            name,
            &function,
            evaluated_args,
            context,
            values,
        )?;
        return eval_native_function_with_values(function, evaluated_args, context, values);
    }
    Err(EvalStatus::UnsupportedConstruct)
}

/// Invokes an invokable object through `call_user_func()` by-value argument semantics.
pub(super) fn eval_invokable_object_with_call_user_func_values(
    object: RuntimeCellHandle,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_object_method_with_call_user_func_values(
        object,
        "__invoke",
        EvalObjectCallbackKind::InvokableObject,
        evaluated_args,
        context,
        values,
    )
}

/// Invokes an invokable object through by-value callable semantics with named args.
pub(super) fn eval_invokable_object_with_call_user_func_args(
    object: RuntimeCellHandle,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_object_method_with_call_user_func_args(
        object,
        "__invoke",
        EvalObjectCallbackKind::InvokableObject,
        evaluated_args,
        context,
        values,
    )
}

/// Invokes an object-method callable through `call_user_func()` by-value semantics.
pub(super) fn eval_object_method_with_call_user_func_values(
    object: RuntimeCellHandle,
    method: &str,
    callback_kind: EvalObjectCallbackKind,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = positional_args(evaluated_args);
    if let Some(result) = eval_object_method_call_user_func_result(
        object,
        method,
        callback_kind,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    eval_method_call_result_with_evaluated_args(object, method, evaluated_args, context, values)
}

/// Invokes an object-method callable through by-value callable semantics with named args.
pub(super) fn eval_object_method_with_call_user_func_args(
    object: RuntimeCellHandle,
    method: &str,
    callback_kind: EvalObjectCallbackKind,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(result) = eval_object_method_call_user_func_result(
        object,
        method,
        callback_kind,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    eval_method_call_result_with_evaluated_args(object, method, evaluated_args, context, values)
}

/// Attempts call-user-func by-value dispatch for eval-declared or generated object methods.
pub(super) fn eval_object_method_call_user_func_result(
    object: RuntimeCellHandle,
    method_name: &str,
    callback_kind: EvalObjectCallbackKind,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        return eval_native_object_method_call_user_func_result(
            object,
            method_name,
            evaluated_args,
            context,
            values,
        );
    };
    let Some(class) = context.dynamic_object_class(identity) else {
        return eval_native_object_method_call_user_func_result(
            object,
            method_name,
            evaluated_args,
            context,
            values,
        );
    };
    let called_class_name = class.name().to_string();
    if let Some((declaring_class, method)) =
        eval_dynamic_method_for_call(&called_class_name, method_name, context)
    {
        if method.is_static() || method.is_abstract() {
            return Ok(None);
        }
        if callback_kind == EvalObjectCallbackKind::Method
            && validate_eval_member_access(&declaring_class, method.visibility(), context).is_err()
        {
            return eval_magic_instance_method_call(
                object,
                &called_class_name,
                method_name,
                evaluated_args,
                context,
                values,
            );
        }
        let callable_name = format!("{}::{}", declaring_class.trim_start_matches('\\'), method.name());
        return eval_dynamic_method_with_values_and_ref_mode(
            &declaring_class,
            &called_class_name,
            &method,
            object,
            method.parameter_is_by_ref(),
            evaluated_args,
            EvalByRefBindingMode::WarnByValue {
                callable_name: &callable_name,
            },
            context,
            values,
        )
        .map(Some);
    }
    let Some(parent) = context.class_native_parent_name(&called_class_name) else {
        return Ok(None);
    };
    eval_native_object_method_call_user_func_result_for_class(
        object,
        &parent,
        method_name,
        Some(&called_class_name),
        evaluated_args,
        context,
        values,
    )
}

/// Attempts call-user-func by-value dispatch for a generated/AOT object method.
pub(super) fn eval_native_object_method_call_user_func_result(
    object: RuntimeCellHandle,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let class_name = runtime_object_class_name(object, values)?;
    eval_native_object_method_call_user_func_result_for_class(
        object,
        &class_name,
        method_name,
        Some(&class_name),
        evaluated_args,
        context,
        values,
    )
}

/// Attempts generated/AOT object-method dispatch for one resolved receiver class.
pub(super) fn eval_native_object_method_call_user_func_result_for_class(
    object: RuntimeCellHandle,
    class_name: &str,
    method_name: &str,
    called_class_scope: Option<&str>,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if let Some(shadow_scope) =
        eval_private_scope_shadow_bridge_scope(class_name, method_name, context, values)?
    {
        // The calling scope's own private method shadows any override on the
        // receiver's class (PHP private-method shadowing); dispatch with the
        // scope string so the native shadow slot selects it directly.
        return eval_native_method_with_evaluated_args_for_call_user_func_unchecked_bridge_scope(
            object,
            class_name,
            method_name,
            evaluated_args,
            Some(&shadow_scope),
            called_class_scope,
            context,
            values,
        )
        .map(Some);
    }
    let Some((declaring_class, visibility, is_static, is_abstract)) =
        eval_aot_method_dispatch_metadata_in_hierarchy(class_name, method_name, context, values)?
    else {
        return Ok(None);
    };
    if is_static || is_abstract {
        return Ok(None);
    }
    if validate_eval_member_access(&declaring_class, visibility, context).is_err()
        && eval_aot_method_dispatch_metadata_in_hierarchy(class_name, "__call", context, values)?
            .is_some_and(|(_, _, is_static, is_abstract)| !is_static && !is_abstract)
    {
        return eval_native_magic_instance_method_call(
            object,
            class_name,
            method_name,
            evaluated_args,
            context,
            values,
        )
        .map(Some);
    }
    eval_native_method_with_evaluated_args_for_call_user_func_unchecked_bridge_scope(
        object,
        class_name,
        method_name,
        evaluated_args,
        Some(&declaring_class),
        called_class_scope,
        context,
        values,
    )
    .map(Some)
}
