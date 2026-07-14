//! Purpose:
//! Executes normalized callables from already evaluated positional values.
//!
//! Called from:
//! - `call_user_func`, direct callable invocation, and callback builtins.
//!
//! Key details:
//! - Closure binding and by-value reference warnings preserve PHP call semantics.

use super::*;

/// Invokes an already normalized callback with source-order positional values.
pub(in crate::interpreter) fn eval_evaluated_callable_with_values(
    callback: &EvaluatedCallable,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match callback {
        EvaluatedCallable::Named { name, .. } => {
            if let Some(closure) = context.closure(name).cloned() {
                return eval_closure_with_evaluated_args(
                    &closure,
                    positional_args(evaluated_args),
                    context,
                    values,
                );
            }
            eval_callable_with_values(name, evaluated_args, context, values)
        }
        EvaluatedCallable::BoundClosure {
            name,
            bound_this,
            bound_scope,
        } => {
            let Some(closure) = context.closure(name).cloned() else {
                return eval_callable_with_values(name, evaluated_args, context, values);
            };
            eval_bound_closure_with_call_args(
                &closure,
                *bound_this,
                bound_scope.clone(),
                positional_args(evaluated_args),
                context,
                values,
            )
        }
        EvaluatedCallable::InvokableObject { object } => {
            eval_invokable_object_call_result(
                *object,
                positional_args(evaluated_args),
                context,
                values,
            )
        }
        EvaluatedCallable::ObjectMethod {
            object,
            method,
            called_class,
            native_class,
            bridge_scope,
        } => match native_class {
            Some(native_class) => eval_native_method_with_evaluated_args_unchecked_bridge_scope(
                *object,
                native_class,
                method,
                positional_args(evaluated_args),
                bridge_scope.as_deref(),
                called_class.as_deref(),
                context,
                values,
            ),
            None => eval_method_call_result(*object, method, evaluated_args, context, values),
        },
        EvaluatedCallable::StaticMethod {
            class_name,
            method,
            called_class,
            native_class,
            bridge_scope,
        } => match native_class {
            Some(native_class) => {
                eval_native_static_method_with_evaluated_args_unchecked_bridge_scope(
                    native_class,
                    method,
                    positional_args(evaluated_args),
                    bridge_scope.as_deref(),
                    called_class.as_deref(),
                    context,
                    values,
                )
            }
            None => match called_class {
                Some(called_class) => eval_static_method_call_result_with_called_class(
                    class_name,
                    called_class,
                    method,
                    positional_args(evaluated_args),
                    context,
                    values,
                ),
                None if eval_callable_array_receiver_is_special_class_name(class_name) => {
                    eval_throw_class_not_found_error(class_name, context, values)
                }
                None => eval_static_method_call_result(
                    class_name,
                    method,
                    positional_args(evaluated_args),
                    context,
                    values,
                ),
            },
        },
    }
}

/// Invokes a bound eval closure with either `$this` or only an explicit class scope.
pub(super) fn eval_bound_closure_with_call_args(
    closure: &EvalClosure,
    bound_this: Option<RuntimeCellHandle>,
    bound_scope: Option<String>,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match bound_this {
        Some(this_object) => eval_closure_with_evaluated_args_and_bound_this_scope(
            closure,
            this_object,
            bound_scope,
            evaluated_args,
            context,
            values,
        ),
        None => eval_closure_with_evaluated_args_and_bound_scope(
            closure,
            bound_scope,
            evaluated_args,
            context,
            values,
        ),
    }
}

/// Invokes a bound eval closure with caller-selected by-reference binding flags.
pub(super) fn eval_bound_closure_with_call_args_ref_flags(
    closure: &EvalClosure,
    bound_this: Option<RuntimeCellHandle>,
    bound_scope: Option<String>,
    parameter_is_by_ref: &[bool],
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match bound_this {
        Some(this_object) => eval_closure_with_evaluated_args_and_bound_this_scope_ref_flags(
            closure,
            this_object,
            bound_scope,
            parameter_is_by_ref,
            evaluated_args,
            context,
            values,
        ),
        None => eval_closure_with_evaluated_args_and_bound_scope_ref_flags(
            closure,
            bound_scope,
            parameter_is_by_ref,
            evaluated_args,
            context,
            values,
        ),
    }
}

/// Invokes a bound eval closure with caller-selected by-reference binding mode.
pub(super) fn eval_bound_closure_with_call_args_ref_mode(
    closure: &EvalClosure,
    bound_this: Option<RuntimeCellHandle>,
    bound_scope: Option<String>,
    parameter_is_by_ref: &[bool],
    evaluated_args: Vec<EvaluatedCallArg>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match bound_this {
        Some(this_object) => eval_closure_with_evaluated_args_and_bound_this_scope_ref_mode(
            closure,
            this_object,
            bound_scope,
            parameter_is_by_ref,
            evaluated_args,
            by_ref_mode,
            context,
            values,
        ),
        None => eval_closure_with_evaluated_args_and_bound_scope_ref_mode(
            closure,
            bound_scope,
            parameter_is_by_ref,
            evaluated_args,
            by_ref_mode,
            context,
            values,
        ),
    }
}

/// Invokes a normalized callback through `call_user_func()` by-value argument semantics.
pub(super) fn eval_evaluated_callable_with_call_user_func_values(
    callback: &EvaluatedCallable,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match callback {
        EvaluatedCallable::Named { name, .. } => {
            eval_named_callable_with_call_user_func_values(name, evaluated_args, context, values)
        }
        EvaluatedCallable::BoundClosure {
            name,
            bound_this,
            bound_scope,
        } => {
            let Some(closure) = context.closure(name).cloned() else {
                return eval_named_callable_with_call_user_func_values(
                    name,
                    evaluated_args,
                    context,
                    values,
                );
            };
            let evaluated_args = positional_args(evaluated_args);
            let parameter_is_by_ref = eval_call_user_func_by_value_ref_flags(
                closure.function().name(),
                closure.function().params(),
                closure.function().parameter_is_by_ref(),
                closure.function().parameter_is_variadic(),
                evaluated_args.len(),
                values,
            )?;
            eval_bound_closure_with_call_args_ref_flags(
                &closure,
                *bound_this,
                bound_scope.clone(),
                &parameter_is_by_ref,
                evaluated_args,
                context,
                values,
            )
        }
        EvaluatedCallable::InvokableObject { object } => {
            eval_invokable_object_with_call_user_func_values(
                *object,
                evaluated_args,
                context,
                values,
            )
        }
        EvaluatedCallable::ObjectMethod {
            object,
            method,
            called_class,
            native_class,
            bridge_scope,
        } => match native_class {
            Some(native_class) => {
                eval_native_method_with_evaluated_args_for_call_user_func_unchecked_bridge_scope(
                    *object,
                    native_class,
                    method,
                    positional_args(evaluated_args),
                    bridge_scope.as_deref(),
                    called_class.as_deref(),
                    context,
                    values,
                )
            }
            None => eval_object_method_with_call_user_func_values(
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
                    positional_args(evaluated_args),
                    bridge_scope.as_deref(),
                    called_class.as_deref(),
                    context,
                    values,
                )
            }
            None => eval_static_method_with_call_user_func_values(
                class_name,
                method,
                called_class.as_deref(),
                evaluated_args,
                context,
                values,
            ),
        },
    }
}

/// Invokes a normalized callback with by-value semantics while preserving named args.
pub(in crate::interpreter) fn eval_evaluated_callable_with_by_value_call_args(
    callback: &EvaluatedCallable,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = eval_clear_evaluated_arg_ref_targets(evaluated_args);
    match callback {
        EvaluatedCallable::Named { name, .. } => {
            eval_named_callable_with_call_user_func_args(name, evaluated_args, context, values)
        }
        EvaluatedCallable::BoundClosure {
            name,
            bound_this,
            bound_scope,
        } => {
            let Some(closure) = context.closure(name).cloned() else {
                return eval_named_callable_with_call_user_func_args(
                    name,
                    evaluated_args,
                    context,
                    values,
                );
            };
            let parameter_is_by_ref = eval_call_user_func_by_value_ref_flags(
                closure.function().name(),
                closure.function().params(),
                closure.function().parameter_is_by_ref(),
                closure.function().parameter_is_variadic(),
                evaluated_args.len(),
                values,
            )?;
            eval_bound_closure_with_call_args_ref_flags(
                &closure,
                *bound_this,
                bound_scope.clone(),
                &parameter_is_by_ref,
                evaluated_args,
                context,
                values,
            )
        }
        EvaluatedCallable::InvokableObject { object } => {
            eval_invokable_object_with_call_user_func_args(
                *object,
                evaluated_args,
                context,
                values,
            )
        }
        EvaluatedCallable::ObjectMethod {
            object,
            method,
            called_class,
            native_class,
            bridge_scope,
        } => match native_class {
            Some(native_class) => {
                eval_native_method_with_evaluated_args_for_call_user_func_unchecked_bridge_scope(
                    *object,
                    native_class,
                    method,
                    evaluated_args,
                    bridge_scope.as_deref(),
                    called_class.as_deref(),
                    context,
                    values,
                )
            }
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
            None => eval_static_method_with_call_user_func_args(
                class_name,
                method,
                called_class.as_deref(),
                evaluated_args,
                context,
                values,
            ),
        },
    }
}

/// Removes caller writeback targets before a by-value callable API dispatch.
pub(super) fn eval_clear_evaluated_arg_ref_targets(
    evaluated_args: Vec<EvaluatedCallArg>,
) -> Vec<EvaluatedCallArg> {
    evaluated_args
        .into_iter()
        .map(|arg| EvaluatedCallArg {
            name: arg.name,
            value: arg.value,
            ref_target: None,
        })
        .collect()
}
