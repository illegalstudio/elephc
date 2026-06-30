//! Purpose:
//! Validates normalized call_user_func callback targets before dispatch.
//! Keeps PHP's invalid-callback TypeError messages out of generic callable normalization.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry::callable`.
//!
//! Key details:
//! - Direct callable invocation still uses normal method-call errors; these helpers
//!   are scoped to `call_user_func` and `call_user_func_array`.

use super::super::super::*;

#[derive(Clone, Copy)]
enum EvalCallableValidationError<'a> {
    CallUserFunc(&'a str),
    ClosureFromCallable,
}

/// Validates callback targets whose PHP errors depend on method metadata.
pub(in crate::interpreter) fn eval_validate_call_user_func_callback(
    callback: &EvaluatedCallable,
    function_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    eval_validate_callback(
        callback,
        EvalCallableValidationError::CallUserFunc(function_name),
        context,
        values,
    )
}

/// Validates `Closure::fromCallable()` callback targets before materializing a closure.
pub(in crate::interpreter) fn eval_validate_closure_from_callable_callback(
    callback: &EvaluatedCallable,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    eval_validate_callback(
        callback,
        EvalCallableValidationError::ClosureFromCallable,
        context,
        values,
    )
}

/// Throws the PHP TypeError used when `Closure::fromCallable()` cannot normalize a value.
pub(in crate::interpreter) fn eval_closure_from_callable_type_error<T>(
    reason: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_invalid_callback_type_error(
        EvalCallableValidationError::ClosureFromCallable,
        reason,
        context,
        values,
    )
}

/// Validates one normalized callback for a PHP API with invalid-callback TypeErrors.
fn eval_validate_callback(
    callback: &EvaluatedCallable,
    error: EvalCallableValidationError<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    match callback {
        EvaluatedCallable::Named { name, display_name } => {
            eval_validate_named_callable(name, display_name, error, context, values)
        }
        EvaluatedCallable::ObjectMethod {
            object,
            method,
            native_class,
            ..
        } => match native_class {
            Some(_) => Ok(()),
            None => eval_validate_call_user_func_object_method(
                *object,
                method,
                error,
                context,
                values,
            ),
        },
        EvaluatedCallable::StaticMethod {
            class_name,
            method,
            native_class,
            ..
        } => match native_class {
            Some(_) => Ok(()),
            None => eval_validate_call_user_func_static_method(
                class_name,
                method,
                error,
                context,
                values,
            ),
        },
        EvaluatedCallable::BoundClosure { .. } | EvaluatedCallable::InvokableObject { .. } => {
            Ok(())
        }
    }
}

/// Validates string function callables before APIs materialize or dispatch them.
fn eval_validate_named_callable(
    name: &str,
    display_name: &str,
    error: EvalCallableValidationError<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if context.has_closure(name)
        || context.has_function(name)
        || eval_function_probe_exists(context, name)
    {
        return Ok(());
    }
    eval_invalid_callback_type_error(
        error,
        &format!("function \"{display_name}\" not found or invalid function name"),
        context,
        values,
    )
}

/// Validates `[$object, "method"]` callbacks before call_user_func dispatch.
fn eval_validate_call_user_func_object_method(
    object: RuntimeCellHandle,
    method_name: &str,
    error: EvalCallableValidationError<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        return Ok(());
    };
    let Some(class) = context.dynamic_object_class(identity) else {
        return eval_validate_call_user_func_native_object_method(
            object,
            method_name,
            error,
            context,
            values,
        );
    };
    let called_class_name = class.name().to_string();
    let Some((declaring_class, method)) =
        eval_dynamic_method_for_call(&called_class_name, method_name, context)
    else {
        if let Some(parent) = context.class_native_parent_name(&called_class_name) {
            let has_native_metadata = eval_dynamic_class_native_method_metadata(
                &called_class_name,
                method_name,
                context,
                values,
            )?
            .is_some();
            let has_native_magic =
                eval_call_user_func_native_instance_magic_callable(&parent, context, values)?;
            let has_native_signature = context
                .native_method_signature(&parent, method_name)
                .is_some();
            let missing_native_class = !values.class_exists(&parent)?;
            let has_native_target = has_native_metadata
                || has_native_magic
                || has_native_signature
                || missing_native_class;
            if has_native_target {
                return eval_validate_call_user_func_native_object_method_for_class(
                    &parent,
                    &called_class_name,
                    method_name,
                    error,
                    context,
                    values,
                );
            }
        }
        if eval_call_user_func_instance_magic_callable(&called_class_name, context) {
            return Ok(());
        }
        return eval_call_user_func_missing_method_type_error(
            error,
            &called_class_name,
            method_name,
            context,
            values,
        );
    };
    if method.is_abstract() {
        return Err(EvalStatus::RuntimeFatal);
    }
    if validate_eval_member_access(&declaring_class, method.visibility(), context).is_ok()
        || eval_call_user_func_instance_magic_callable(&called_class_name, context)
    {
        return Ok(());
    }
    eval_call_user_func_method_access_type_error(
        error,
        &declaring_class,
        method.name(),
        method.visibility(),
        context,
        values,
    )
}

/// Validates generated/AOT object-method callbacks when method metadata is available.
fn eval_validate_call_user_func_native_object_method(
    object: RuntimeCellHandle,
    method_name: &str,
    error: EvalCallableValidationError<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let class_name = runtime_object_class_name(object, values)?;
    eval_validate_call_user_func_native_object_method_for_class(
        &class_name,
        &class_name,
        method_name,
        error,
        context,
        values,
    )
}

/// Validates generated/AOT object-method callbacks by class metadata.
fn eval_validate_call_user_func_native_object_method_for_class(
    class_name: &str,
    error_class_name: &str,
    method_name: &str,
    error: EvalCallableValidationError<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some((declaring_class, visibility, _, is_abstract)) =
        eval_aot_method_dispatch_metadata_in_hierarchy(&class_name, method_name, context, values)?
    else {
        if eval_call_user_func_native_instance_magic_callable(&class_name, context, values)?
            || context
                .native_method_signature(&class_name, method_name)
                .is_some()
            || !values.class_exists(class_name)?
        {
            return Ok(());
        }
        return eval_call_user_func_missing_method_type_error(
            error,
            error_class_name,
            method_name,
            context,
            values,
        );
    };
    if is_abstract {
        return Err(EvalStatus::RuntimeFatal);
    }
    if validate_eval_member_access(&declaring_class, visibility, context).is_ok()
        || eval_call_user_func_native_instance_magic_callable(&class_name, context, values)?
    {
        return Ok(());
    }
    eval_call_user_func_method_access_type_error(
        error,
        &declaring_class,
        method_name,
        visibility,
        context,
        values,
    )
}

/// Validates `["Class", "method"]` callbacks before call_user_func dispatch.
fn eval_validate_call_user_func_static_method(
    class_name: &str,
    method_name: &str,
    error: EvalCallableValidationError<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let class_name = resolve_eval_static_member_class_name(class_name, context)?;
    if eval_enum_static_builtin_applies(&class_name, method_name, context).is_some() {
        return Ok(());
    }
    if let Some((declaring_class, method)) = context.class_method(&class_name, method_name) {
        if !method.is_static() {
            return eval_call_user_func_non_static_method_type_error(
                error,
                &declaring_class,
                method.name(),
                context,
                values,
            );
        }
        if method.is_abstract() {
            return Err(EvalStatus::RuntimeFatal);
        }
        if validate_eval_member_access(&declaring_class, method.visibility(), context).is_ok()
            || eval_call_user_func_static_magic_callable(&class_name, context)
        {
            return Ok(());
        }
        return eval_call_user_func_method_access_type_error(
            error,
            &declaring_class,
            method.name(),
            method.visibility(),
            context,
            values,
        );
    }
    if context.has_class(&class_name)
        || context.has_interface(&class_name)
        || context.has_trait(&class_name)
        || context.has_enum(&class_name)
    {
        if eval_call_user_func_static_magic_callable(&class_name, context) {
            return Ok(());
        }
        if let Some(parent) = context.class_native_parent_name(&class_name) {
            return eval_validate_call_user_func_native_static_method_for_class(
                &parent,
                &class_name,
                method_name,
                error,
                context,
                values,
            );
        }
        return eval_call_user_func_missing_method_type_error(
            error,
            &class_name,
            method_name,
            context,
            values,
        );
    }
    eval_validate_call_user_func_native_static_method(
        &class_name,
        method_name,
        error,
        context,
        values,
    )
}

/// Validates generated/AOT static-method callbacks when method metadata is available.
fn eval_validate_call_user_func_native_static_method(
    class_name: &str,
    method_name: &str,
    error: EvalCallableValidationError<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some((declaring_class, visibility, is_static, is_abstract)) =
        eval_aot_method_dispatch_metadata_in_hierarchy(class_name, method_name, context, values)?
    else {
        if eval_call_user_func_native_static_magic_callable(class_name, context, values)?
            || context
                .native_static_method_signature(class_name, method_name)
                .is_some()
            || !values.class_exists(class_name)?
        {
            return Ok(());
        }
        return eval_call_user_func_missing_method_type_error(
            error,
            class_name,
            method_name,
            context,
            values,
        );
    };
    if !is_static {
        return eval_call_user_func_non_static_method_type_error(
            error,
            &declaring_class,
            method_name,
            context,
            values,
        );
    }
    if is_abstract {
        return Err(EvalStatus::RuntimeFatal);
    }
    if validate_eval_member_access(&declaring_class, visibility, context).is_ok()
        || eval_call_user_func_native_static_magic_callable(class_name, context, values)?
    {
        return Ok(());
    }
    eval_call_user_func_method_access_type_error(
        error,
        &declaring_class,
        method_name,
        visibility,
        context,
        values,
    )
}

/// Validates generated/AOT static-method callbacks while preserving eval-class error names.
fn eval_validate_call_user_func_native_static_method_for_class(
    class_name: &str,
    error_class_name: &str,
    method_name: &str,
    error: EvalCallableValidationError<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some((declaring_class, visibility, is_static, is_abstract)) =
        eval_aot_method_dispatch_metadata_in_hierarchy(class_name, method_name, context, values)?
    else {
        if eval_call_user_func_native_static_magic_callable(class_name, context, values)?
            || context
                .native_static_method_signature(class_name, method_name)
                .is_some()
            || !values.class_exists(class_name)?
        {
            return Ok(());
        }
        return eval_call_user_func_missing_method_type_error(
            error,
            error_class_name,
            method_name,
            context,
            values,
        );
    };
    if !is_static {
        return eval_call_user_func_non_static_method_type_error(
            error,
            &declaring_class,
            method_name,
            context,
            values,
        );
    }
    if is_abstract {
        return Err(EvalStatus::RuntimeFatal);
    }
    if validate_eval_member_access(&declaring_class, visibility, context).is_ok()
        || eval_call_user_func_native_static_magic_callable(class_name, context, values)?
    {
        return Ok(());
    }
    eval_call_user_func_method_access_type_error(
        error,
        &declaring_class,
        method_name,
        visibility,
        context,
        values,
    )
}

/// Returns whether an eval class has an instance magic-call fallback.
fn eval_call_user_func_instance_magic_callable(
    class_name: &str,
    context: &ElephcEvalContext,
) -> bool {
    context
        .class_method(class_name, "__call")
        .is_some_and(|(_, method)| !method.is_static() && !method.is_abstract())
}

/// Returns whether an eval class has a static magic-call fallback.
fn eval_call_user_func_static_magic_callable(
    class_name: &str,
    context: &ElephcEvalContext,
) -> bool {
    context
        .class_method(class_name, "__callStatic")
        .is_some_and(|(_, method)| method.is_static() && !method.is_abstract())
}

/// Returns whether an AOT class has an instance magic-call fallback.
fn eval_call_user_func_native_instance_magic_callable(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(eval_aot_method_dispatch_metadata_in_hierarchy(class_name, "__call", context, values)?
        .is_some_and(|(_, _, is_static, is_abstract)| !is_static && !is_abstract))
}

/// Returns whether an AOT class has a static magic-call fallback.
fn eval_call_user_func_native_static_magic_callable(
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

/// Throws the API-specific TypeError for a missing object or static method callback.
fn eval_call_user_func_missing_method_type_error<T>(
    error: EvalCallableValidationError<'_>,
    class_name: &str,
    method_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_invalid_callback_type_error(
        error,
        &format!(
            "class {} does not have a method \"{}\"",
            class_name.trim_start_matches('\\'),
            method_name
        ),
        context,
        values,
    )
}

/// Throws the API-specific TypeError for an inaccessible method callback.
fn eval_call_user_func_method_access_type_error<T>(
    error: EvalCallableValidationError<'_>,
    class_name: &str,
    method_name: &str,
    visibility: EvalVisibility,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_invalid_callback_type_error(
        error,
        &format!(
            "cannot access {} method {}::{}()",
            eval_call_user_func_visibility_label(visibility),
            class_name.trim_start_matches('\\'),
            method_name
        ),
        context,
        values,
    )
}

/// Throws the API-specific TypeError for non-static static-method callbacks.
fn eval_call_user_func_non_static_method_type_error<T>(
    error: EvalCallableValidationError<'_>,
    class_name: &str,
    method_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_invalid_callback_type_error(
        error,
        &format!(
            "non-static method {}::{}() cannot be called statically",
            class_name.trim_start_matches('\\'),
            method_name
        ),
        context,
        values,
    )
}

/// Throws a call_user_func or call_user_func_array invalid-callback TypeError.
pub(in crate::interpreter) fn eval_call_user_func_type_error<T>(
    function_name: &str,
    reason: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_invalid_callback_type_error(
        EvalCallableValidationError::CallUserFunc(function_name),
        reason,
        context,
        values,
    )
}

/// Throws the invalid-callback TypeError for the PHP API currently validating a callback.
fn eval_invalid_callback_type_error<T>(
    error: EvalCallableValidationError<'_>,
    reason: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    let message = match error {
        EvalCallableValidationError::CallUserFunc(function_name) => {
            format!(
                "{}(): Argument #1 ($callback) must be a valid callback, {}",
                function_name, reason
            )
        }
        EvalCallableValidationError::ClosureFromCallable => {
            format!("Failed to create closure from callable: {reason}")
        }
    };
    eval_throw_type_error(&message, context, values)
}

/// Returns PHP's lowercase visibility label used in callback TypeError messages.
fn eval_call_user_func_visibility_label(visibility: EvalVisibility) -> &'static str {
    match visibility {
        EvalVisibility::Public => "public",
        EvalVisibility::Protected => "protected",
        EvalVisibility::Private => "private",
    }
}
