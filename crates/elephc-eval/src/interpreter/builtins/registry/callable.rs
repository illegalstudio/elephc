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
    let callback = eval_callable(callback, context, values)?;
    if !values.is_array_like(arg_array)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let evaluated_args = eval_array_call_arg_values(arg_array, values)?;
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
    let callback = eval_callable(*callback, context, values)?;
    eval_evaluated_callable_with_values(&callback, callback_args.to_vec(), context, values)
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
        return eval_array_callable(callback, values);
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
        return Ok(EvaluatedCallable::InvokableObject { object: callback });
    };
    let Some((_, method)) = context.class_method(class.name(), "__invoke") else {
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
        EVAL_TAG_OBJECT => Ok(EvaluatedCallable::ObjectMethod {
            object: receiver,
            method,
        }),
        EVAL_TAG_STRING => {
            let class_name = String::from_utf8(values.string_bytes(receiver)?)
                .map_err(|_| EvalStatus::RuntimeFatal)?;
            Ok(EvaluatedCallable::StaticMethod { class_name, method })
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
        });
    }
    Ok(EvaluatedCallable::Named(
        callback.trim_start_matches('\\').to_ascii_lowercase(),
    ))
}

/// Normalizes one string function callback name for builtin callback positions.
pub(in crate::interpreter) fn eval_callable_name(
    callback: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let callback = values.string_bytes(callback)?;
    let callback = String::from_utf8(callback).map_err(|_| EvalStatus::RuntimeFatal)?;
    let callback = callback.trim_start_matches('\\').to_ascii_lowercase();
    if callback.contains("::") {
        return Err(EvalStatus::UnsupportedConstruct);
    }
    Ok(callback)
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
            eval_method_call_result(*object, "__invoke", evaluated_args, context, values)
        }
        EvaluatedCallable::ObjectMethod { object, method } => {
            eval_method_call_result(*object, method, evaluated_args, context, values)
        }
        EvaluatedCallable::StaticMethod { class_name, method } => eval_static_method_call_result(
            class_name,
            method,
            positional_args(evaluated_args),
            context,
            values,
        ),
    }
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
            eval_method_call_result_with_evaluated_args(
                *object,
                "__invoke",
                evaluated_args,
                context,
                values,
            )
        }
        EvaluatedCallable::ObjectMethod { object, method } => {
            eval_method_call_result_with_evaluated_args(
                *object,
                method,
                evaluated_args,
                context,
                values,
            )
        }
        EvaluatedCallable::StaticMethod { class_name, method } => {
            eval_static_method_call_result(class_name, method, evaluated_args, context, values)
        }
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
        return eval_native_function_with_values(function, evaluated_args, values);
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
    if evaluated_args.iter().all(|arg| arg.name.is_none()) {
        let evaluated_args = evaluated_args.into_iter().map(|arg| arg.value).collect();
        return eval_callable_with_values(name, evaluated_args, context, values);
    }
    if eval_php_visible_builtin_exists(name) {
        let evaluated_args = bind_evaluated_builtin_args(name, evaluated_args, values)?;
        let Some(result) = eval_builtin_with_values(name, &evaluated_args, context, values)? else {
            return Err(EvalStatus::UnsupportedConstruct);
        };
        return Ok(result);
    }
    if let Some(function) = context.function(name).cloned() {
        let evaluated_args = bind_evaluated_function_args(function.params(), evaluated_args)?;
        return eval_dynamic_function_with_values(&function, evaluated_args, context, values);
    }
    if let Some(function) = context.native_function(name) {
        if function.param_names().len() != function.param_count() {
            return Err(EvalStatus::RuntimeFatal);
        }
        let evaluated_args = bind_evaluated_function_args(function.param_names(), evaluated_args)?;
        return eval_native_function_with_values(function, evaluated_args, values);
    }
    Err(EvalStatus::UnsupportedConstruct)
}
