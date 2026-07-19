//! Purpose:
//! Dispatches invokable objects and magic instance/static calls.
//!
//! Called from:
//! - Callable and method dispatch after normal method lookup.
//!
//! Key details:
//! - `__invoke`, `__call`, and `__callStatic` receive normalized argument arrays.

use super::*;

/// Dispatches an invokable object through `__invoke()` without enforcing hook visibility.
pub(in crate::interpreter) fn eval_invokable_object_call_result(
    object: RuntimeCellHandle,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        let evaluated_args = positional_evaluated_arg_values(evaluated_args)?;
        return values.method_call(object, "__invoke", evaluated_args);
    };
    let Some(class) = context.dynamic_object_class(identity) else {
        let class_name = runtime_object_class_name(object, values)?;
        let Some((_, _, is_static, is_abstract)) =
            eval_aot_method_dispatch_metadata_in_hierarchy(
                &class_name,
                "__invoke",
                context,
                values,
            )?
        else {
            return eval_throw_object_not_callable_error(&class_name, context, values);
        };
        if is_static || is_abstract {
            return Err(EvalStatus::RuntimeFatal);
        }
        return eval_native_method_with_evaluated_args_unchecked(
            object,
            &class_name,
            "__invoke",
            evaluated_args,
            context,
            values,
        );
    };
    let called_class_name = class.name().to_string();
    let Some((declaring_class, method)) = context.class_method(&called_class_name, "__invoke")
    else {
        if let Some(native_class_name) =
            eval_dynamic_class_native_invokable_method_class(&called_class_name, context, values)?
        {
            return eval_native_method_with_evaluated_args_unchecked_bridge_scope(
                object,
                &native_class_name,
                "__invoke",
                evaluated_args,
                Some(&native_class_name),
                Some(&called_class_name),
                context,
                values,
            );
        }
        return eval_throw_object_not_callable_error(&called_class_name, context, values);
    };
    if method.is_static() || method.is_abstract() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_dynamic_method_with_values(
        &declaring_class,
        &called_class_name,
        &method,
        object,
        evaluated_args,
        context,
        values,
    )
}

/// Rejects non-invokable eval-declared objects before dynamic-call arguments are evaluated.
pub(in crate::interpreter) fn eval_invokable_object_precheck(
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        return Ok(());
    };
    let Some(class) = context.dynamic_object_class(identity) else {
        let class_name = runtime_object_class_name(object, values)?;
        let Some((_, _, is_static, is_abstract)) =
            eval_aot_method_dispatch_metadata_in_hierarchy(
                &class_name,
                "__invoke",
                context,
                values,
            )?
        else {
            return eval_throw_object_not_callable_error(&class_name, context, values);
        };
        if is_static || is_abstract {
            return Err(EvalStatus::RuntimeFatal);
        }
        return Ok(());
    };
    let called_class_name = class.name().to_string();
    let Some((_, method)) = context.class_method(&called_class_name, "__invoke") else {
        if eval_dynamic_class_native_invokable_method_class(&called_class_name, context, values)?
            .is_some()
        {
            return Ok(());
        }
        return eval_throw_object_not_callable_error(&called_class_name, context, values);
    };
    if method.is_static() || method.is_abstract() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(())
}

/// Returns the generated/AOT class that can dispatch an inherited `__invoke()` hook.
pub(in crate::interpreter) fn eval_dynamic_class_native_invokable_method_class(
    called_class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    let Some((declaring_class, _, is_static, is_abstract)) =
        eval_dynamic_class_native_method_metadata(called_class_name, "__invoke", context, values)?
    else {
        return Ok(None);
    };
    if is_static || is_abstract {
        return Ok(None);
    }
    Ok(Some(declaring_class))
}

/// Returns generated/AOT method metadata inherited by an eval-declared class.
pub(in crate::interpreter) fn eval_dynamic_class_native_method_metadata(
    called_class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(String, EvalVisibility, bool, bool)>, EvalStatus> {
    let Some(parent) = context.class_native_parent_name(called_class_name) else {
        return Ok(None);
    };
    eval_aot_method_dispatch_metadata_in_hierarchy(&parent, method_name, context, values)
}

/// Dispatches a missing or inaccessible eval instance method through `__call()`.
pub(in crate::interpreter) fn eval_magic_instance_method_call(
    object: RuntimeCellHandle,
    called_class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some((declaring_class, method)) = context.class_method(called_class_name, "__call") else {
        return Ok(None);
    };
    if method.is_static() || method.is_abstract() {
        return Ok(None);
    }
    let magic_args = eval_magic_call_args(method_name, evaluated_args, values)?;
    eval_dynamic_method_with_values(
        &declaring_class,
        called_class_name,
        &method,
        object,
        magic_args,
        context,
        values,
    )
    .map(Some)
}

/// Dispatches a missing or inaccessible eval static method through `__callStatic()`.
pub(in crate::interpreter) fn eval_magic_static_method_call(
    class_name: &str,
    called_class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some((declaring_class, method)) = context.class_method(class_name, "__callStatic") else {
        return Ok(None);
    };
    if !method.is_static() || method.is_abstract() {
        return Ok(None);
    }
    let magic_args = eval_magic_call_args(method_name, evaluated_args, values)?;
    eval_dynamic_static_method_with_values(
        &declaring_class,
        called_class_name,
        &method,
        magic_args,
        context,
        values,
    )
    .map(Some)
}

/// Builds the two synthetic arguments passed to `__call()` and `__callStatic()`.
pub(super) fn eval_magic_call_args(
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    let method = values.string(method_name)?;
    let args = eval_magic_call_arg_array(evaluated_args, values)?;
    Ok(positional_args(vec![method, args]))
}

/// Materializes PHP's `$args` array for a magic method fallback.
pub(super) fn eval_magic_call_arg_array(
    evaluated_args: Vec<EvaluatedCallArg>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let contains_named = evaluated_args.iter().any(|arg| arg.name.is_some());
    let mut args = if contains_named {
        values.assoc_new(evaluated_args.len())?
    } else {
        values.array_new(evaluated_args.len())?
    };
    let mut next_positional = 0_i64;
    for arg in evaluated_args {
        let key = if let Some(name) = arg.name {
            values.string(&name)?
        } else {
            let key = values.int(next_positional)?;
            next_positional = next_positional
                .checked_add(1)
                .ok_or(EvalStatus::RuntimeFatal)?;
            key
        };
        args = values.array_set(args, key, arg.value)?;
    }
    Ok(args)
}
