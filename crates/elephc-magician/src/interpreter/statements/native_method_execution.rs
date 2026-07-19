//! Purpose:
//! Executes generated/AOT instance and static methods through native bridge scopes.
//!
//! Called from:
//! - Method, static-method, Reflection, and call_user_func dispatch.
//!
//! Key details:
//! - Checked and unchecked entry points share binding, bridge scope, and reference-mode handling.

use super::*;

/// Calls one generated/AOT instance method after native signature binding.
pub(in crate::interpreter) fn eval_native_method_with_evaluated_args(
    object: RuntimeCellHandle,
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_native_method_with_evaluated_args_bridge_scope(
        object,
        class_name,
        method_name,
        evaluated_args,
        None,
        None,
        context,
        values,
    )
}

/// Calls one generated/AOT instance method after validation with an optional bridge scope.
pub(super) fn eval_native_method_with_evaluated_args_bridge_scope(
    object: RuntimeCellHandle,
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    bridge_scope: Option<&str>,
    called_class_scope: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut resolved_bridge_scope = bridge_scope.map(str::to_string);
    if resolved_bridge_scope.is_none() {
        if let Some(shadow_scope) =
            eval_private_scope_shadow_bridge_scope(class_name, method_name, context, values)?
        {
            // The calling scope's own private method shadows any override on
            // the receiver's class; access is inherently allowed, so skip the
            // hierarchy resolution (it would find the override instead).
            return eval_native_method_with_evaluated_args_unchecked_bridge_scope(
                object,
                class_name,
                method_name,
                evaluated_args,
                Some(&shadow_scope),
                called_class_scope,
                context,
                values,
            );
        }
    }
    let metadata =
        eval_aot_method_dispatch_metadata_in_hierarchy(class_name, method_name, context, values)?;
    if let Some((declaring_class, visibility, _, is_abstract)) = metadata {
        if resolved_bridge_scope.is_none() {
            resolved_bridge_scope = Some(declaring_class.clone());
        }
        if !is_abstract
            && validate_eval_member_access(&declaring_class, visibility, context).is_err()
        {
            if eval_native_instance_magic_method_available(class_name, context, values)? {
                return eval_native_magic_instance_method_call(
                    object,
                    class_name,
                    method_name,
                    evaluated_args,
                    context,
                    values,
                );
            }
            return eval_throw_method_access_error(
                &declaring_class,
                method_name,
                visibility,
                context,
                values,
            );
        }
    } else if eval_native_instance_magic_method_available(class_name, context, values)? {
        return eval_native_magic_instance_method_call(
            object,
            class_name,
            method_name,
            evaluated_args,
            context,
            values,
        );
    }
    eval_native_method_with_evaluated_args_unchecked_bridge_scope(
        object,
        class_name,
        method_name,
        evaluated_args,
        resolved_bridge_scope.as_deref(),
        called_class_scope,
        context,
        values,
    )
}

/// Calls one generated/AOT instance method without enforcing member visibility.
pub(super) fn eval_native_method_with_evaluated_args_unchecked(
    object: RuntimeCellHandle,
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_native_method_with_evaluated_args_unchecked_bridge_scope(
        object,
        class_name,
        method_name,
        evaluated_args,
        None,
        None,
        context,
        values,
    )
}

/// Calls one generated/AOT instance method without visibility checks using an optional bridge scope.
pub(in crate::interpreter) fn eval_native_method_with_evaluated_args_unchecked_bridge_scope(
    object: RuntimeCellHandle,
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    bridge_scope: Option<&str>,
    called_class_scope: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_native_method_with_evaluated_args_unchecked_bridge_scope_with_ref_mode(
        object,
        class_name,
        method_name,
        evaluated_args,
        bridge_scope,
        called_class_scope,
        EvalByRefBindingMode::RequireTarget,
        context,
        values,
    )
}

/// Calls one generated/AOT instance method for `call_user_func()` by-value by-ref degradation.
pub(in crate::interpreter) fn eval_native_method_with_evaluated_args_for_call_user_func_unchecked_bridge_scope(
    object: RuntimeCellHandle,
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    bridge_scope: Option<&str>,
    called_class_scope: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let signature_owner = bridge_scope.unwrap_or(class_name);
    let callable_name = format!("{}::{}", signature_owner.trim_start_matches('\\'), method_name);
    eval_native_method_with_evaluated_args_unchecked_bridge_scope_with_ref_mode(
        object,
        class_name,
        method_name,
        evaluated_args,
        bridge_scope,
        called_class_scope,
        EvalByRefBindingMode::WarnByValue {
            callable_name: &callable_name,
        },
        context,
        values,
    )
}

/// Calls one generated/AOT instance method with a selected by-reference binding mode.
pub(super) fn eval_native_method_with_evaluated_args_unchecked_bridge_scope_with_ref_mode(
    object: RuntimeCellHandle,
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    bridge_scope: Option<&str>,
    called_class_scope: Option<&str>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let signature_owner = bridge_scope.unwrap_or(class_name);
    let signature = context.native_method_signature(signature_owner, method_name);
    let return_type = signature.as_ref().and_then(|signature| signature.return_type().cloned());
    let bound_args =
        bind_native_callable_bound_args_with_mode(signature, evaluated_args, by_ref_mode, context, values)?;
    let result = if let Some(scope) = bridge_scope {
        eval_native_method_call_with_scope(
            scope,
            called_class_scope,
            object,
            method_name,
            native_bound_arg_values(&bound_args),
            context,
            values,
        )
    } else {
        values.method_call(object, method_name, native_bound_arg_values(&bound_args))
    };
    let writeback = write_back_native_callable_ref_args(&bound_args, context, values);
    match (result, writeback) {
        (Err(status), _) | (_, Err(status)) => Err(status),
        (Ok(result), Ok(())) => eval_declared_native_return_value(
            return_type.as_ref(),
            Some(signature_owner),
            called_class_scope.or(Some(class_name)),
            result,
            context,
            values,
        ),
    }
}

/// Calls one generated/AOT static method after native signature binding.
pub(in crate::interpreter) fn eval_native_static_method_with_evaluated_args(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_native_static_method_with_evaluated_args_bridge_scope(
        class_name,
        method_name,
        evaluated_args,
        None,
        None,
        context,
        values,
    )
}

/// Calls one generated/AOT static method after validation with an optional bridge scope.
pub(super) fn eval_native_static_method_with_evaluated_args_bridge_scope(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    bridge_scope: Option<&str>,
    called_class_scope: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut resolved_bridge_scope = bridge_scope.map(str::to_string);
    let metadata =
        eval_aot_method_dispatch_metadata_in_hierarchy(class_name, method_name, context, values)?;
    if let Some((declaring_class, visibility, is_static, is_abstract)) = metadata {
        if resolved_bridge_scope.is_none() {
            resolved_bridge_scope = Some(declaring_class.clone());
        }
        if is_static
            && !is_abstract
            && validate_eval_member_access(&declaring_class, visibility, context).is_err()
        {
            if eval_native_static_magic_method_available(class_name, context, values)? {
                return eval_native_magic_static_method_call(
                    class_name,
                    method_name,
                    evaluated_args,
                    context,
                    values,
                );
            }
            return eval_throw_method_access_error(
                &declaring_class,
                method_name,
                visibility,
                context,
                values,
            );
        }
    } else if eval_native_static_magic_method_available(class_name, context, values)? {
        return eval_native_magic_static_method_call(
            class_name,
            method_name,
            evaluated_args,
            context,
            values,
        );
    }
    eval_native_static_method_with_evaluated_args_unchecked_bridge_scope(
        class_name,
        method_name,
        evaluated_args,
        resolved_bridge_scope.as_deref(),
        called_class_scope,
        context,
        values,
    )
}

/// Calls one generated/AOT static method without enforcing member visibility.
pub(super) fn eval_native_static_method_with_evaluated_args_unchecked(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_native_static_method_with_evaluated_args_unchecked_bridge_scope(
        class_name,
        method_name,
        evaluated_args,
        None,
        None,
        context,
        values,
    )
}

/// Calls one generated/AOT static method without visibility checks using an optional bridge scope.
pub(in crate::interpreter) fn eval_native_static_method_with_evaluated_args_unchecked_bridge_scope(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    bridge_scope: Option<&str>,
    called_class_scope: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_native_static_method_with_evaluated_args_unchecked_bridge_scope_with_ref_mode(
        class_name,
        method_name,
        evaluated_args,
        bridge_scope,
        called_class_scope,
        EvalByRefBindingMode::RequireTarget,
        context,
        values,
    )
}

/// Calls one generated/AOT static method for `call_user_func()` by-value by-ref degradation.
pub(in crate::interpreter) fn eval_native_static_method_with_evaluated_args_for_call_user_func_unchecked_bridge_scope(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    bridge_scope: Option<&str>,
    called_class_scope: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let signature_owner = bridge_scope.unwrap_or(class_name);
    let callable_name = format!("{}::{}", signature_owner.trim_start_matches('\\'), method_name);
    eval_native_static_method_with_evaluated_args_unchecked_bridge_scope_with_ref_mode(
        class_name,
        method_name,
        evaluated_args,
        bridge_scope,
        called_class_scope,
        EvalByRefBindingMode::WarnByValue {
            callable_name: &callable_name,
        },
        context,
        values,
    )
}

/// Calls one generated/AOT static method with a selected by-reference binding mode.
pub(super) fn eval_native_static_method_with_evaluated_args_unchecked_bridge_scope_with_ref_mode(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    bridge_scope: Option<&str>,
    called_class_scope: Option<&str>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let signature_owner = bridge_scope.unwrap_or(class_name);
    let signature = context.native_static_method_signature(signature_owner, method_name);
    let return_type = signature.as_ref().and_then(|signature| signature.return_type().cloned());
    let bound_args =
        bind_native_callable_bound_args_with_mode(signature, evaluated_args, by_ref_mode, context, values)?;
    let result = if let Some(scope) = bridge_scope {
        eval_native_static_method_call_with_scope(
            scope,
            called_class_scope,
            class_name,
            method_name,
            native_bound_arg_values(&bound_args),
            context,
            values,
        )
    } else {
        values.static_method_call(class_name, method_name, native_bound_arg_values(&bound_args))
    };
    let writeback = write_back_native_callable_ref_args(&bound_args, context, values);
    match (result, writeback) {
        (Err(status), _) | (_, Err(status)) => Err(status),
        (Ok(result), Ok(())) => eval_declared_native_return_value(
            return_type.as_ref(),
            Some(signature_owner),
            called_class_scope.or(Some(class_name)),
            result,
            context,
            values,
        ),
    }
}

/// Returns whether a generated/AOT class has an instance `__call()` fallback.
pub(super) fn eval_native_instance_magic_method_available(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(eval_aot_method_dispatch_metadata_in_hierarchy(class_name, "__call", context, values)?
        .is_some_and(|(_, _, is_static, is_abstract)| !is_static && !is_abstract))
}

/// Returns whether a generated/AOT class has a static `__callStatic()` fallback.
pub(super) fn eval_native_static_magic_method_available(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(
        eval_aot_method_dispatch_metadata_in_hierarchy(
            class_name,
            "__callStatic",
            context,
            values,
        )?
        .is_some_and(|(_, _, is_static, is_abstract)| is_static && !is_abstract),
    )
}

/// Dispatches a missing or inaccessible generated/AOT instance method through `__call()`.
pub(in crate::interpreter) fn eval_native_magic_instance_method_call(
    object: RuntimeCellHandle,
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let magic_args = eval_magic_call_args(method_name, evaluated_args, values)?;
    eval_native_method_with_evaluated_args_unchecked(
        object,
        class_name,
        "__call",
        magic_args,
        context,
        values,
    )
}

/// Dispatches a missing or inaccessible generated/AOT static method through `__callStatic()`.
pub(in crate::interpreter) fn eval_native_magic_static_method_call(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let magic_args = eval_magic_call_args(method_name, evaluated_args, values)?;
    eval_native_static_method_with_evaluated_args_unchecked(
        class_name,
        "__callStatic",
        magic_args,
        context,
        values,
    )
}
