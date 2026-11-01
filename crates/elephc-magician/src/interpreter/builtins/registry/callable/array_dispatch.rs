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
        EvaluatedCallable::ForeignContext { callback, owner } => {
            with_foreign_owner_context(owner, context, values, |owner_context, values| {
                eval_evaluated_callable_with_call_array_args(
                    callback,
                    evaluated_args,
                    owner_context,
                    values,
                )
            })
        }
        EvaluatedCallable::Named { name, .. } => {
            if let Some(closure) = context.closure(name).cloned() {
                let parameter_is_by_ref = closure.function().parameter_is_by_ref();
                let by_ref_mode = EvalByRefBindingMode::WarnByValue {
                    callable_name: closure.function().name(),
                };
                // A non-static closure or arrow function declared inside a method implicitly
                // captured `$this` at its declaration site (`eval_closure_expr`); an unbound call
                // through a plain variable (`$f(...)`) must still see it, exactly as it would
                // through the direct call path in `execution.rs`. Deriving the called (late
                // static) class from the captured object itself -- rather than reusing whatever
                // lexical scope was captured -- is also what makes `static::` inside the closure
                // follow the RUNTIME class of that `$this`, not the class that lexically declared
                // the closure (a closure declared in a parent method but invoked on a child
                // instance must see `static::` as the child).
                return match closure.declaring_this() {
                    Some(this_object) => eval_closure_with_evaluated_args_and_bound_this_scope_ref_mode(
                        &closure,
                        this_object,
                        closure.declaring_class_scope().map(str::to_string),
                        parameter_is_by_ref,
                        evaluated_args,
                        by_ref_mode,
                        context,
                        values,
                    ),
                    None => eval_closure_with_evaluated_args_and_bound_scope_ref_mode(
                        &closure,
                        closure.declaring_class_scope().map(str::to_string),
                        parameter_is_by_ref,
                        evaluated_args,
                        by_ref_mode,
                        context,
                        values,
                    ),
                };
            }
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
        EvaluatedCallable::NativeDescriptor { descriptor } => {
            let Some(decoded) =
                (unsafe { decode_callable_descriptor(*descriptor as *mut std::ffi::c_void) })
            else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let bound =
                bind_evaluated_native_function_args(&decoded.function, evaluated_args, context, values)?;
            eval_native_function_with_values(decoded.function, bound, context, values)
        }
        EvaluatedCallable::ObjectMethod {
            object,
            method,
            called_class,
            native_class,
            bridge_scope,
        } => {
            eval_reject_fiber_object_switch_during_pcntl_dispatch(
                *object,
                method,
                context,
                values,
            )?;
            // A Reflection owner's methods are answered by the interpreter's own reflection
            // layer, which the ordinary method dispatch consults first; the native body sees a
            // reflector the interpreter materialized and answers nothing useful for it. Doctrine's
            // Instantiator returns `[$reflectionClass, 'newInstanceWithoutConstructor']` and calls
            // it, so every entity hydration built a null and then `ReflectionClass("")`.
            let native_class = native_class
                .as_deref()
                .filter(|class| !crate::interpreter::reflection::eval_is_reflection_owner_class(class));
            match native_class {
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
            None => super::execution::with_closure_method_scope(
                bridge_scope.as_deref(),
                context,
                |context| {
                    eval_object_method_with_call_user_func_args(
                        *object,
                        method,
                        EvalObjectCallbackKind::Method,
                        evaluated_args,
                        context,
                        values,
                    )
                },
            ),
            }
        }
        EvaluatedCallable::StaticMethod {
            class_name,
            method,
            called_class,
            native_class,
            bridge_scope,
        } => {
            eval_reject_fiber_switch_during_pcntl_dispatch(
                class_name,
                method,
                context,
                values,
            )?;
            match native_class {
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
            }
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
    if let Some(forbidden) = eval_forbidden_dynamic_scope_builtin(name) {
        return eval_throw_forbidden_dynamic_scope_builtin(forbidden, context, values);
    }
    if let Some(parameters) = eval_opcache_parameters(name, context) {
        return eval_opcache_call_with_evaluated_args(
            name, parameters, &positional_args(evaluated_args), None, context, values,
        );
    }
    if eval_builtin_uses_owned_arguments(name) {
        return eval_builtin_callback_with_arguments(name, positional_args(evaluated_args), false, context, values);
    }
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
    if let Some(function) = match context.function(name).cloned() {
        Some(function) => Some(function),
        None => generic_function_for_call(name, &positional_args(evaluated_args.clone()), context, values)?,
    } {
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
    if let Some(forbidden) = eval_forbidden_dynamic_scope_builtin(name) {
        return eval_throw_forbidden_dynamic_scope_builtin(forbidden, context, values);
    }
    if let Some(parameters) = eval_opcache_parameters(name, context) {
        return eval_opcache_call_with_evaluated_args(
            name, parameters, &evaluated_args, None, context, values,
        );
    }
    // Check a known positional count before generic callable dispatch.
    if evaluated_args.iter().all(|arg| arg.name.is_none()) {
        eval_check_builtin_arity(name, evaluated_args.len(), context, values)?;
    }
    if eval_builtin_uses_owned_arguments(name) {
        return eval_builtin_callback_with_arguments(name, evaluated_args, true, context, values);
    }
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
        return eval_bound_builtin_call(name, evaluated_args, context, values);
    }
    if let Some(closure) = context.closure(name).cloned() {
        return eval_closure_with_evaluated_args_and_bound_scope_ref_mode(
            &closure,
            closure.declaring_class_scope().map(str::to_string),
            closure.function().parameter_is_by_ref(),
            evaluated_args,
            EvalByRefBindingMode::WarnByValue {
                callable_name: closure.function().name(),
            },
            context,
            values,
        );
    }
    if let Some(function) = match context.function(name).cloned() {
        Some(function) => Some(function),
        None => generic_function_for_call(name, &evaluated_args, context, values)?,
    } {
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
    // Legacy runtime handlers without registry signatures accept positional values only.
    if evaluated_args.iter().all(|arg| arg.name.is_none()) {
        let positional: Vec<RuntimeCellHandle> =
            evaluated_args.iter().map(|arg| arg.value).collect();
        if let Some(result) = eval_builtin_with_values(name, &positional, context, values)? {
            return Ok(result);
        }
    }
    note_eval_runtime_failure(format!("callable {name}() is not defined"), context);
    Err(EvalStatus::UnsupportedConstruct)
}
