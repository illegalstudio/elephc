//! Purpose:
//! Executes normalized callbacks from evaluated call-array arguments or values.
//!
//! Called from:
//! - `call_user_func_array` and internal callable-with-values helpers.
//!
//! Key details:
//! - Named metadata and call-array ordering are preserved before target dispatch.

use super::*;

/// Invokes an already normalized callback with optional named-argument metadata.
pub(in crate::interpreter) fn eval_evaluated_callable_with_call_array_args(
    callback: &EvaluatedCallable,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match callback {
        EvaluatedCallable::Named { name, .. } => {
            eval_callable_with_call_array_args(name, evaluated_args, context, values)
        }
        EvaluatedCallable::BoundClosure {
            name,
            bound_this,
            bound_scope,
        } => {
            let Some(closure) = context.closure(name).cloned() else {
                return eval_callable_with_call_array_args(name, evaluated_args, context, values);
            };
            eval_bound_closure_with_call_args_ref_mode(
                &closure,
                *bound_this,
                bound_scope.clone(),
                closure.function().parameter_is_by_ref(),
                evaluated_args,
                EvalByRefBindingMode::WarnByValue {
                    callable_name: closure.function().name(),
                },
                context,
                values,
            )
        }
        EvaluatedCallable::InvokableObject { object } => {
            eval_invokable_object_with_call_user_func_args(*object, evaluated_args, context, values)
        }
        EvaluatedCallable::ObjectMethod {
            object,
            method,
            called_class,
            native_class,
            bridge_scope,
        } => match native_class {
            Some(native_class) => eval_native_method_with_evaluated_args_for_call_user_func_unchecked_bridge_scope(
                *object,
                native_class,
                method,
                evaluated_args,
                bridge_scope.as_deref(),
                called_class.as_deref(),
                context,
                values,
            ),
            None => eval_object_method_with_call_user_func_args(
                *object,
                method,
                EvalObjectCallbackKind::Method,
                evaluated_args,
                context,
                values,
            ),
        },
        EvaluatedCallable::StaticMethod {
            class_name,
            method,
            called_class,
            native_class,
            bridge_scope,
        } => match native_class {
            Some(native_class) => {
                eval_native_static_method_with_evaluated_args_for_call_user_func_unchecked_bridge_scope(
                    native_class,
                    method,
                    evaluated_args,
                    bridge_scope.as_deref(),
                    called_class.as_deref(),
                    context,
                    values,
                )
            }
            None => match called_class {
                Some(called_class) => eval_static_method_with_call_user_func_args(
                    class_name,
                    method,
                    Some(called_class),
                    evaluated_args,
                    context,
                    values,
                ),
                None if eval_callable_array_receiver_is_special_class_name(class_name) => {
                    eval_throw_class_not_found_error(class_name, context, values)
                }
                None => eval_static_method_with_call_user_func_args(
                    class_name,
                    method,
                    None,
                    evaluated_args,
                    context,
                    values,
                ),
            },
        },
    }
}

/// Invokes a PHP-visible callable name with source-order positional values.
pub(in crate::interpreter) fn eval_callable_with_values(
    name: &str,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(result) = eval_builtin_with_values(name, &evaluated_args, context, values)? {
        return Ok(result);
    }
    if let Some(closure) = context.closure(name).cloned() {
        return eval_closure_with_evaluated_args(
            &closure,
            positional_args(evaluated_args),
            context,
            values,
        );
    }
    if let Some(function) = context.function(name).cloned() {
        return eval_dynamic_function_with_values(&function, evaluated_args, context, values);
    }
    if let Some(function) = context.native_function(name) {
        let evaluated_args = positional_args(evaluated_args);
        let evaluated_args =
            bind_evaluated_native_function_args(&function, evaluated_args, context, values)?;
        return eval_native_function_with_values(function, evaluated_args, context, values);
    }
    Err(EvalStatus::UnsupportedConstruct)
}

/// Invokes a callable with arguments that may carry `call_user_func_array` names.
pub(in crate::interpreter) fn eval_callable_with_call_array_args(
    name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(result) =
        eval_date_procedural_alias_with_evaluated_args(name, evaluated_args.clone(), context, values)?
    {
        return Ok(result);
    }
    if let Some(result) =
        eval_mutating_builtin_with_call_array_args(name, &evaluated_args, context, values)?
    {
        return Ok(result);
    }
    if eval_php_visible_builtin_exists(name) {
        let evaluated_args = bind_evaluated_builtin_args(name, evaluated_args, values)?;
        let Some(result) = eval_builtin_with_values(name, &evaluated_args, context, values)? else {
            return Err(EvalStatus::UnsupportedConstruct);
        };
        return Ok(result);
    }
    if let Some(closure) = context.closure(name).cloned() {
        return eval_closure_with_evaluated_args_and_bound_scope_ref_mode(
            &closure,
            None,
            closure.function().parameter_is_by_ref(),
            evaluated_args,
            EvalByRefBindingMode::WarnByValue {
                callable_name: closure.function().name(),
            },
            context,
            values,
        );
    }
    if let Some(function) = context.function(name).cloned() {
        return eval_dynamic_function_with_evaluated_args_and_ref_mode(
            &function,
            function.parameter_is_by_ref(),
            evaluated_args,
            EvalByRefBindingMode::WarnByValue {
                callable_name: function.name(),
            },
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
