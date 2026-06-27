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

/// Validates callback targets whose PHP errors depend on method metadata.
pub(in crate::interpreter) fn eval_validate_call_user_func_callback(
    callback: &EvaluatedCallable,
    function_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    match callback {
        EvaluatedCallable::ObjectMethod { object, method } => {
            eval_validate_call_user_func_object_method(
                *object,
                method,
                function_name,
                context,
                values,
            )
        }
        EvaluatedCallable::StaticMethod {
            class_name, method, ..
        } => {
            eval_validate_call_user_func_static_method(
                class_name,
                method,
                function_name,
                context,
                values,
            )
        }
        EvaluatedCallable::Named(_) | EvaluatedCallable::InvokableObject { .. } => Ok(()),
    }
}

/// Validates `[$object, "method"]` callbacks before call_user_func dispatch.
fn eval_validate_call_user_func_object_method(
    object: RuntimeCellHandle,
    method_name: &str,
    function_name: &str,
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
            function_name,
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
                    function_name,
                    context,
                    values,
                );
            }
        }
        if eval_call_user_func_instance_magic_callable(&called_class_name, context) {
            return Ok(());
        }
        return eval_call_user_func_missing_method_type_error(
            function_name,
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
        function_name,
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
    function_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let class_name = runtime_object_class_name(object, values)?;
    eval_validate_call_user_func_native_object_method_for_class(
        &class_name,
        &class_name,
        method_name,
        function_name,
        context,
        values,
    )
}

/// Validates generated/AOT object-method callbacks by class metadata.
fn eval_validate_call_user_func_native_object_method_for_class(
    class_name: &str,
    error_class_name: &str,
    method_name: &str,
    function_name: &str,
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
            function_name,
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
        function_name,
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
    function_name: &str,
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
                function_name,
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
            function_name,
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
        return eval_call_user_func_missing_method_type_error(
            function_name,
            &class_name,
            method_name,
            context,
            values,
        );
    }
    eval_validate_call_user_func_native_static_method(
        &class_name,
        method_name,
        function_name,
        context,
        values,
    )
}

/// Validates generated/AOT static-method callbacks when method metadata is available.
fn eval_validate_call_user_func_native_static_method(
    class_name: &str,
    method_name: &str,
    function_name: &str,
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
            function_name,
            class_name,
            method_name,
            context,
            values,
        );
    };
    if !is_static {
        return eval_call_user_func_non_static_method_type_error(
            function_name,
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
        function_name,
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

/// Throws call_user_func's TypeError for a missing object or static method callback.
fn eval_call_user_func_missing_method_type_error<T>(
    function_name: &str,
    class_name: &str,
    method_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_call_user_func_type_error(
        function_name,
        &format!(
            "class {} does not have a method \"{}\"",
            class_name.trim_start_matches('\\'),
            method_name
        ),
        context,
        values,
    )
}

/// Throws call_user_func's TypeError for an inaccessible method callback.
fn eval_call_user_func_method_access_type_error<T>(
    function_name: &str,
    class_name: &str,
    method_name: &str,
    visibility: EvalVisibility,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_call_user_func_type_error(
        function_name,
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

/// Throws call_user_func's TypeError for non-static static-method callbacks.
fn eval_call_user_func_non_static_method_type_error<T>(
    function_name: &str,
    class_name: &str,
    method_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_call_user_func_type_error(
        function_name,
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
    eval_throw_type_error(
        &format!(
            "{}(): Argument #1 ($callback) must be a valid callback, {}",
            function_name, reason
        ),
        context,
        values,
    )
}

/// Returns PHP's lowercase visibility label used in callback TypeError messages.
fn eval_call_user_func_visibility_label(visibility: EvalVisibility) -> &'static str {
    match visibility {
        EvalVisibility::Public => "public",
        EvalVisibility::Protected => "protected",
        EvalVisibility::Private => "private",
    }
}
