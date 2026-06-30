//! Purpose:
//! call_user_func and callable normalization helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry` re-exports.
//!
//! Key details:
//! - Helpers are scoped to the eval interpreter and operate on already parsed
//!   EvalIR call metadata or evaluated runtime-cell handles.

use super::super::super::*;
use super::*;

/// Evaluates `call_user_func($name, ...$args)` inside a runtime eval fragment.
pub(in crate::interpreter) fn eval_builtin_call_user_func(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_call_user_func_with_values(evaluated_args, context, values)
}

/// Evaluates `call_user_func_array($name, $args)` inside a runtime eval fragment.
pub(in crate::interpreter) fn eval_builtin_call_user_func_array(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [callback, arg_array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let callback = eval_expr(callback, context, scope, values)?;
    let arg_array = eval_expr(arg_array, context, scope, values)?;
    eval_call_user_func_array_with_values(callback, arg_array, context, values)
}

/// Dispatches `call_user_func_array` after callback and array arguments are evaluated.
pub(in crate::interpreter) fn eval_call_user_func_array_with_values(
    callback: RuntimeCellHandle,
    arg_array: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let callback = eval_call_user_func_callback(callback, "call_user_func_array", context, values)?;
    if !values.is_array_like(arg_array)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let evaluated_args = eval_array_call_arg_values(arg_array, context, values)?;
    eval_evaluated_callable_with_call_array_args(&callback, evaluated_args, context, values)
}

/// Dispatches `call_user_func` after its callback and arguments are already evaluated.
pub(in crate::interpreter) fn eval_call_user_func_with_values(
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((callback, callback_args)) = evaluated_args.split_first() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let callback = eval_call_user_func_callback(*callback, "call_user_func", context, values)?;
    eval_evaluated_callable_with_call_user_func_values(
        &callback,
        callback_args.to_vec(),
        context,
        values,
    )
}

/// Normalizes a `call_user_func*` callback and maps non-invokable objects to PHP's TypeError.
fn eval_call_user_func_callback(
    callback: RuntimeCellHandle,
    function_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvaluatedCallable, EvalStatus> {
    match eval_callable(callback, context, values) {
        Ok(callback) => {
            eval_validate_call_user_func_callback(&callback, function_name, context, values)?;
            Ok(callback)
        }
        Err(EvalStatus::UnsupportedConstruct) if values.type_tag(callback)? == EVAL_TAG_OBJECT => {
            eval_call_user_func_type_error(
                function_name,
                "no array or string given",
                context,
                values,
            )
        }
        Err(status) => Err(status),
    }
}

/// Normalizes one PHP callback value for eval dynamic callable dispatch.
pub(in crate::interpreter) fn eval_callable(
    callback: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvaluatedCallable, EvalStatus> {
    if values.type_tag(callback)? == EVAL_TAG_OBJECT {
        return eval_object_callable(callback, context, values);
    }
    if values.is_array_like(callback)? {
        return eval_array_callable(callback, context, values);
    }
    eval_string_callable(callback, values)
}

/// Normalizes one invokable eval object for dynamic callable dispatch.
pub(in crate::interpreter) fn eval_object_callable(
    callback: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvaluatedCallable, EvalStatus> {
    let identity = values.object_identity(callback)?;
    let Some(class) = context.dynamic_object_class(identity) else {
        let class_name = runtime_object_class_name(callback, values)?;
        let Some((_, _, is_static, is_abstract)) =
            eval_aot_method_dispatch_metadata_in_hierarchy(
                &class_name,
                "__invoke",
                context,
                values,
            )?
        else {
            return Err(EvalStatus::UnsupportedConstruct);
        };
        if is_static || is_abstract {
            return Err(EvalStatus::UnsupportedConstruct);
        }
        return Ok(EvaluatedCallable::InvokableObject { object: callback });
    };
    let Some((_, method)) = context.class_method(class.name(), "__invoke") else {
        if eval_dynamic_class_native_invokable_method_class(class.name(), context, values)?
            .is_some()
        {
            return Ok(EvaluatedCallable::InvokableObject { object: callback });
        }
        return Err(EvalStatus::UnsupportedConstruct);
    };
    if method.is_static() || method.is_abstract() {
        return Err(EvalStatus::UnsupportedConstruct);
    }
    Ok(EvaluatedCallable::InvokableObject { object: callback })
}

/// Normalizes one two-element object-method or static-method callable array.
pub(in crate::interpreter) fn eval_array_callable(
    callback: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvaluatedCallable, EvalStatus> {
    if values.array_len(callback)? != 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let zero = values.int(0)?;
    let one = values.int(1)?;
    let receiver = values.array_get(callback, zero)?;
    let method = values.array_get(callback, one)?;
    let method =
        String::from_utf8(values.string_bytes(method)?).map_err(|_| EvalStatus::RuntimeFatal)?;
    match values.type_tag(receiver)? {
        EVAL_TAG_OBJECT => {
            let native_dispatch = context
                .eval_object_callable_native_dispatch(callback, receiver, &method)
                .map(|(native_class, bridge_scope, called_class)| {
                    (
                        native_class.to_string(),
                        bridge_scope.to_string(),
                        called_class.to_string(),
                    )
                });
            let (native_class, bridge_scope, called_class) = native_dispatch
                .map(|(native_class, bridge_scope, called_class)| {
                    (Some(native_class), Some(bridge_scope), Some(called_class))
                })
                .unwrap_or((None, None, None));
            Ok(EvaluatedCallable::ObjectMethod {
                object: receiver,
                method,
                called_class,
                native_class,
                bridge_scope,
            })
        }
        EVAL_TAG_STRING => {
            let class_name = String::from_utf8(values.string_bytes(receiver)?)
                .map_err(|_| EvalStatus::RuntimeFatal)?;
            let called_class = context
                .eval_static_callable_called_class(callback, &class_name, &method)
                .map(str::to_string);
            let native_dispatch = context
                .eval_static_callable_native_dispatch(callback, &class_name, &method)
                .map(|(native_class, bridge_scope)| {
                    (native_class.to_string(), bridge_scope.to_string())
                });
            let (native_class, bridge_scope) = native_dispatch
                .map(|(native_class, bridge_scope)| (Some(native_class), Some(bridge_scope)))
                .unwrap_or((None, None));
            Ok(EvaluatedCallable::StaticMethod {
                class_name,
                method,
                called_class,
                native_class,
                bridge_scope,
            })
        }
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Normalizes one string callback name for eval dynamic callable dispatch.
pub(in crate::interpreter) fn eval_string_callable(
    callback: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<EvaluatedCallable, EvalStatus> {
    let callback = values.string_bytes(callback)?;
    let callback = String::from_utf8(callback).map_err(|_| EvalStatus::RuntimeFatal)?;
    if let Some((class_name, method)) = callback.split_once("::") {
        if class_name.is_empty() || method.is_empty() {
            return Err(EvalStatus::RuntimeFatal);
        }
        return Ok(EvaluatedCallable::StaticMethod {
            class_name: class_name.trim_start_matches('\\').to_string(),
            method: method.to_string(),
            called_class: None,
            native_class: None,
            bridge_scope: None,
        });
    }
    Ok(EvaluatedCallable::Named(
        callback.trim_start_matches('\\').to_ascii_lowercase(),
    ))
}

/// Invokes an already normalized callback with source-order positional values.
pub(in crate::interpreter) fn eval_evaluated_callable_with_values(
    callback: &EvaluatedCallable,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match callback {
        EvaluatedCallable::Named(name) => {
            eval_callable_with_values(name, evaluated_args, context, values)
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

/// Invokes a normalized callback through `call_user_func()` by-value argument semantics.
fn eval_evaluated_callable_with_call_user_func_values(
    callback: &EvaluatedCallable,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match callback {
        EvaluatedCallable::Named(name) => {
            eval_named_callable_with_call_user_func_values(name, evaluated_args, context, values)
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

/// Invokes a named callable through `call_user_func()` and warns for by-ref parameters.
fn eval_named_callable_with_call_user_func_values(
    name: &str,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(result) = eval_builtin_with_values(name, &evaluated_args, context, values)? {
        return Ok(result);
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

/// Invokes an invokable object through `call_user_func()` by-value argument semantics.
fn eval_invokable_object_with_call_user_func_values(
    object: RuntimeCellHandle,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_object_method_with_call_user_func_values(
        object,
        "__invoke",
        evaluated_args,
        context,
        values,
    )
}

/// Invokes an object-method callable through `call_user_func()` by-value semantics.
fn eval_object_method_with_call_user_func_values(
    object: RuntimeCellHandle,
    method: &str,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = positional_args(evaluated_args);
    if let Some(result) = eval_object_method_call_user_func_result(
        object,
        method,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    eval_method_call_result_with_evaluated_args(object, method, evaluated_args, context, values)
}

/// Attempts call-user-func by-value dispatch for eval-declared or generated object methods.
fn eval_object_method_call_user_func_result(
    object: RuntimeCellHandle,
    method_name: &str,
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
        let parameter_is_by_ref = eval_call_user_func_by_value_ref_flags(
            &format!("{}::{}", declaring_class.trim_start_matches('\\'), method.name()),
            method.params(),
            method.parameter_is_by_ref(),
            method.parameter_is_variadic(),
            evaluated_args.len(),
            values,
        )?;
        return eval_dynamic_method_with_values_and_ref_flags(
            &declaring_class,
            &called_class_name,
            &method,
            object,
            &parameter_is_by_ref,
            evaluated_args,
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
fn eval_native_object_method_call_user_func_result(
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
fn eval_native_object_method_call_user_func_result_for_class(
    object: RuntimeCellHandle,
    class_name: &str,
    method_name: &str,
    called_class_scope: Option<&str>,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some((declaring_class, _, is_static, is_abstract)) =
        eval_aot_method_dispatch_metadata_in_hierarchy(class_name, method_name, context, values)?
    else {
        return Ok(None);
    };
    if is_static || is_abstract {
        return Ok(None);
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

/// Invokes a static-method callable through `call_user_func()` by-value semantics.
fn eval_static_method_with_call_user_func_values(
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
fn eval_static_method_call_user_func_result(
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
        let parameter_is_by_ref = eval_call_user_func_by_value_ref_flags(
            &format!("{}::{}", declaring_class.trim_start_matches('\\'), method.name()),
            method.params(),
            method.parameter_is_by_ref(),
            method.parameter_is_variadic(),
            evaluated_args.len(),
            values,
        )?;
        return eval_dynamic_static_method_with_values_and_ref_flags(
            &declaring_class,
            &called_class,
            &method,
            &parameter_is_by_ref,
            evaluated_args,
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
    let Some((declaring_class, _, is_static, is_abstract)) =
        eval_aot_method_dispatch_metadata_in_hierarchy(
            &native_class,
            method_name,
            context,
            values,
        )?
    else {
        return Ok(None);
    };
    if !is_static || is_abstract {
        return Ok(None);
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
fn eval_call_user_func_by_value_ref_flags(
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

/// Invokes an already normalized callback with optional named-argument metadata.
pub(in crate::interpreter) fn eval_evaluated_callable_with_call_array_args(
    callback: &EvaluatedCallable,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match callback {
        EvaluatedCallable::Named(name) => {
            eval_callable_with_call_array_args(name, evaluated_args, context, values)
        }
        EvaluatedCallable::InvokableObject { object } => {
            eval_invokable_object_call_result(*object, evaluated_args, context, values)
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
                evaluated_args,
                bridge_scope.as_deref(),
                called_class.as_deref(),
                context,
                values,
            ),
            None => eval_method_call_result_with_evaluated_args(
                *object,
                method,
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
                eval_native_static_method_with_evaluated_args_unchecked_bridge_scope(
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
                Some(called_class) => eval_static_method_call_result_with_called_class(
                    class_name,
                    called_class,
                    method,
                    evaluated_args,
                    context,
                    values,
                ),
                None => eval_static_method_call_result(
                    class_name,
                    method,
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
    if evaluated_args
        .iter()
        .all(|arg| arg.name.is_none() && arg.ref_target.is_none())
    {
        let evaluated_args = evaluated_args.into_iter().map(|arg| arg.value).collect();
        return eval_callable_with_values(name, evaluated_args, context, values);
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
    if let Some(function) = context.function(name).cloned() {
        return eval_dynamic_function_with_evaluated_args(
            &function,
            evaluated_args,
            context,
            values,
        );
    }
    if let Some(function) = context.native_function(name) {
        let evaluated_args =
            bind_evaluated_native_function_args(&function, evaluated_args, context, values)?;
        return eval_native_function_with_values(function, evaluated_args, context, values);
    }
    Err(EvalStatus::UnsupportedConstruct)
}
