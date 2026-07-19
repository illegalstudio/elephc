//! Purpose:
//! Dispatches normalized static-method callbacks through call_user_func semantics.
//!
//! Called from:
//! - Callable execution for class-string and special-scope static targets.
//!
//! Key details:
//! - Called-class propagation and by-reference warning flags stay paired.

use super::*;

/// Invokes a static-method callable through `call_user_func()` by-value semantics.
pub(super) fn eval_static_method_with_call_user_func_values(
    class_name: &str,
    method_name: &str,
    called_class: Option<&str>,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = positional_args(evaluated_args);
    if let Some(result) = eval_static_method_call_user_func_result(
        class_name,
        method_name,
        called_class,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    match called_class {
        Some(called_class) => eval_static_method_call_result_with_called_class(
            class_name,
            called_class,
            method_name,
            evaluated_args,
            context,
            values,
        ),
        None if eval_callable_array_receiver_is_special_class_name(class_name) => {
            eval_throw_class_not_found_error(class_name, context, values)
        }
        None => eval_static_method_call_result(
            class_name,
            method_name,
            evaluated_args,
            context,
            values,
        ),
    }
}

/// Invokes a static-method callable through by-value callable semantics with named args.
pub(super) fn eval_static_method_with_call_user_func_args(
    class_name: &str,
    method_name: &str,
    called_class: Option<&str>,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(result) = eval_static_method_call_user_func_result(
        class_name,
        method_name,
        called_class,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    match called_class {
        Some(called_class) => eval_static_method_call_result_with_called_class(
            class_name,
            called_class,
            method_name,
            evaluated_args,
            context,
            values,
        ),
        None if eval_callable_array_receiver_is_special_class_name(class_name) => {
            eval_throw_class_not_found_error(class_name, context, values)
        }
        None => eval_static_method_call_result(
            class_name,
            method_name,
            evaluated_args,
            context,
            values,
        ),
    }
}

/// Attempts call-user-func by-value dispatch for eval-declared or generated static methods.
pub(super) fn eval_static_method_call_user_func_result(
    class_name: &str,
    method_name: &str,
    called_class: Option<&str>,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let dispatch_class = resolve_eval_static_member_class_name(class_name, context)?;
    let called_class = called_class.unwrap_or(&dispatch_class).to_string();
    if let Some((declaring_class, method)) =
        eval_dynamic_static_method_for_call(&dispatch_class, method_name, context)
    {
        if !method.is_static() || method.is_abstract() {
            return Ok(None);
        }
        if validate_eval_member_access(&declaring_class, method.visibility(), context).is_err() {
            return eval_magic_static_method_call(
                &dispatch_class,
                &called_class,
                method_name,
                evaluated_args,
                context,
                values,
            );
        }
        let callable_name = format!("{}::{}", declaring_class.trim_start_matches('\\'), method.name());
        return eval_dynamic_static_method_with_values_and_ref_mode(
            &declaring_class,
            &called_class,
            &method,
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
    let native_class = if context.has_class(&dispatch_class) {
        let Some(parent) = context.class_native_parent_name(&dispatch_class) else {
            return Ok(None);
        };
        parent
    } else if context.has_interface(&dispatch_class)
        || context.has_trait(&dispatch_class)
        || context.has_enum(&dispatch_class)
    {
        return Ok(None);
    } else {
        dispatch_class.clone()
    };
    let Some((declaring_class, visibility, is_static, is_abstract)) =
        eval_aot_method_dispatch_metadata_in_hierarchy(
            &native_class,
            method_name,
            context,
            values,
        )?
    else {
        if context
            .native_static_method_signature(&native_class, method_name)
            .is_some()
        {
            return eval_native_static_method_with_evaluated_args_for_call_user_func_unchecked_bridge_scope(
                &native_class,
                method_name,
                evaluated_args,
                None,
                Some(&called_class),
                context,
                values,
            )
            .map(Some);
        }
        return Ok(None);
    };
    if !is_static || is_abstract {
        return Ok(None);
    }
    if validate_eval_member_access(&declaring_class, visibility, context).is_err()
        && eval_aot_method_dispatch_metadata_in_hierarchy(
            &native_class,
            "__callStatic",
            context,
            values,
        )?
        .is_some_and(|(_, _, is_static, is_abstract)| is_static && !is_abstract)
    {
        return eval_native_magic_static_method_call(
            &native_class,
            method_name,
            evaluated_args,
            context,
            values,
        )
        .map(Some);
    }
    eval_native_static_method_with_evaluated_args_for_call_user_func_unchecked_bridge_scope(
        &native_class,
        method_name,
        evaluated_args,
        Some(&declaring_class),
        Some(&called_class),
        context,
        values,
    )
    .map(Some)
}

/// Builds by-value binding flags for `call_user_func()` and emits PHP by-ref warnings.
pub(super) fn eval_call_user_func_by_value_ref_flags(
    callable_name: &str,
    params: &[String],
    parameter_is_by_ref: &[bool],
    parameter_is_variadic: &[bool],
    supplied_count: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<bool>, EvalStatus> {
    let variadic_index = parameter_is_variadic
        .iter()
        .position(|is_variadic| *is_variadic);
    for arg_index in 0..supplied_count {
        let param_index = if variadic_index.is_some_and(|index| arg_index >= index) {
            variadic_index.ok_or(EvalStatus::RuntimeFatal)?
        } else {
            arg_index
        };
        if !parameter_is_by_ref
            .get(param_index)
            .copied()
            .unwrap_or(false)
        {
            continue;
        }
        let param_name = params
            .get(param_index)
            .map(String::as_str)
            .unwrap_or("arg");
        values.warning(&format!(
            "{callable_name}(): Argument #{} (${param_name}) must be passed by reference, value given",
            arg_index + 1
        ))?;
    }
    Ok(vec![false; params.len()])
}
