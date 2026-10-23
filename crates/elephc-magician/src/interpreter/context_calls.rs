//! Purpose:
//! Exposes persistent-context function, object, and metadata operations to generated code.
//!
//! Called from:
//! - The Magician FFI function/object call and symbol-query adapters.
//!
//! Key details:
//! - Arguments arrive already evaluated and borrow the caller's runtime cells.
//! - Generic templates specialize before ordinary dispatch, and thrown owners become outcomes.

use super::*;

/// Executes a zero-argument function declared in the shared eval context.
pub fn execute_context_function_zero_args(
    context: &mut ElephcEvalContext,
    name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    execute_context_function(context, name, Vec::new(), values)
}

/// Executes a function declared in the shared eval context with prepared argument cells.
pub fn execute_context_function(
    context: &mut ElephcEvalContext,
    name: &str,
    args: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match execute_context_function_outcome(context, name, args, values)? {
        EvalOutcome::Value(result) => Ok(result),
        EvalOutcome::Throwable(error) => {
            context.set_pending_throw(error);
            Err(EvalStatus::UncaughtThrowable)
        }
    }
}

/// Executes a function declared in the shared eval context and preserves thrown cells.
pub fn execute_context_function_outcome(
    context: &mut ElephcEvalContext,
    name: &str,
    args: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    let result = (|| {
        let arguments = args.into_iter().map(|value| EvaluatedCallArg { name: None, value, ref_target: None }).collect::<Vec<_>>();
        let function = match context.function(name).cloned() {
            Some(function) => function,
            None => generic_function_for_call(name, &arguments, context, values)?.ok_or(EvalStatus::UnsupportedConstruct)?,
        };
        eval_dynamic_function_with_evaluated_args(&function, arguments, context, values)
    })();
    match result {
        Ok(result) => Ok(EvalOutcome::Value(result)),
        Err(EvalStatus::UncaughtThrowable) => context.take_pending_throw().map(EvalOutcome::Throwable).ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Executes a named eval-context callable with arguments from a PHP array container.
pub fn execute_context_function_call_array(
    context: &mut ElephcEvalContext,
    name: &str,
    arg_array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match execute_context_function_call_array_outcome(context, name, arg_array, values)? {
        EvalOutcome::Value(result) => Ok(result),
        EvalOutcome::Throwable(error) => {
            context.set_pending_throw(error);
            Err(EvalStatus::UncaughtThrowable)
        }
    }
}

/// Executes a named eval-context callable from an argument array and preserves thrown cells.
pub fn execute_context_function_call_array_outcome(
    context: &mut ElephcEvalContext,
    name: &str,
    arg_array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    if !values.is_array_like(arg_array)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let result = with_eval_array_call_arguments(arg_array, context, values, |arguments, context, values| {
        eval_callable_with_call_array_args(name, arguments, context, values)
    });
    match result {
        Ok(result) => Ok(EvalOutcome::Value(result)),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Executes a callback value with a prepared argument array in the shared eval context.
pub fn execute_context_callable_call_array_outcome(
    context: &mut ElephcEvalContext,
    callback: RuntimeCellHandle,
    arg_array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    match eval_call_user_func_array_with_values(callback, arg_array, context, values) {
        Ok(result) => Ok(EvalOutcome::Value(result)),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Rebinds `$this` on an eval `Closure` object on behalf of generated `Closure::bind` code.
pub fn execute_context_closure_bind_this(
    context: &mut ElephcEvalContext,
    closure: RuntimeCellHandle,
    new_this: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_closure_bind_this_for_ffi(closure, new_this, context, values)
}

/// Probes whether a callback value is callable in the shared eval context.
pub fn execute_context_is_callable(
    context: &ElephcEvalContext,
    callback: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    eval_is_callable_value(callback, None, context, values)
}

/// Constructs a class declared in the shared eval context with prepared positional arguments.
pub fn execute_context_new_object_outcome(
    context: &mut ElephcEvalContext,
    name: &str,
    args: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    execute_context_try_new_object_outcome(context, name, args, values)?
        .ok_or(EvalStatus::UnsupportedConstruct)
}

/// Attempts to construct an eval-declared class, returning `None` when it is absent.
pub fn execute_context_try_new_object_outcome(
    context: &mut ElephcEvalContext,
    name: &str,
    args: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalOutcome>, EvalStatus> {
    let evaluated_args = args
        .into_iter()
        .map(|value| EvaluatedCallArg {
            name: None,
            value,
            ref_target: None,
        })
        .collect::<Vec<_>>();
    let mut scope = ElephcEvalScope::new();
    let result = (|| {
        let name = ensure_generic_class(name, Some(&evaluated_args), "__construct", context, &mut scope, values)?;
        let Some(class) = context.class(&name).cloned() else { return Ok(None); };
        eval_dynamic_class_new_object(&class, evaluated_args, context, &mut scope, values).map(Some)
    })();
    match result {
        Ok(result) => Ok(result.map(EvalOutcome::Value)),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .map(Some)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Calls a method on a value that may be an eval-created object.
pub fn execute_context_method_call_outcome(
    context: &mut ElephcEvalContext,
    object: RuntimeCellHandle,
    method: &str,
    args: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    match eval_method_call_result(object, method, args, context, values) {
        Ok(result) => Ok(EvalOutcome::Value(result)),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Converts one boxed value through eval's PHP string-context semantics.
///
/// Dynamic objects dispatch their eval-declared `__toString()` implementation, while an
/// exception raised by that method is returned as an uncaught throwable outcome for the C ABI.
pub fn execute_context_string_outcome(
    context: &mut ElephcEvalContext,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    match eval_string_context_value(value, context, values) {
        Ok(result) => Ok(EvalOutcome::Value(result)),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Calls a static method on a class-like symbol known to the shared eval context.
pub fn execute_context_static_method_call_outcome(
    context: &mut ElephcEvalContext,
    class_name: &str,
    method: &str,
    args: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    let evaluated_args = args
        .into_iter()
        .map(|value| EvaluatedCallArg {
            name: None,
            value,
            ref_target: None,
        })
        .collect::<Vec<_>>();
    let result = (|| {
        let name = ensure_generic_class(class_name, Some(&evaluated_args), method, context, &mut ElephcEvalScope::new(), values)?;
        eval_static_method_call_result(&name, method, evaluated_args, context, values)
    })();
    match result {
        Ok(result) => Ok(EvalOutcome::Value(result)),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Resolves object class-name builtins against eval dynamic-object metadata first.
pub fn execute_context_object_class_name(
    context: &mut ElephcEvalContext,
    lookup: &str,
    object_or_class: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match lookup {
        "get_class" => eval_get_class_result(object_or_class, context, values),
        "get_parent_class" => eval_get_parent_class_result(object_or_class, context, values),
        "get_debug_type" => eval_get_debug_type_result(object_or_class, context, values),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Resolves class/interface/trait relation metadata through eval dynamic metadata.
pub fn execute_context_class_relation(
    context: &mut ElephcEvalContext,
    name: &str,
    target: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_class_relation_result(name, &[target], context, values)
}

/// Fetches a class-like constant through eval dynamic metadata and runtime fallback hooks.
pub fn execute_context_class_constant_fetch(
    context: &mut ElephcEvalContext,
    class_name: &str,
    constant_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    let result = (|| {
        let name = ensure_generic_class(class_name, None, "", context, &mut ElephcEvalScope::new(), values)?;
        eval_class_constant_fetch_result(&name, constant_name, context, values)
    })();
    match result {
        Ok(result) => Ok(EvalOutcome::Value(result)),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Reads a static property through eval dynamic metadata and runtime fallback hooks.
pub fn execute_context_static_property_get(
    context: &mut ElephcEvalContext,
    class_name: &str,
    property_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    let result = (|| {
        let name = ensure_generic_class(class_name, None, "", context, &mut ElephcEvalScope::new(), values)?;
        eval_static_property_get_result(&name, property_name, context, values)
    })();
    match result {
        Ok(result) => Ok(EvalOutcome::Value(result)),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Writes a static property through eval dynamic metadata and runtime fallback hooks.
pub fn execute_context_static_property_set(
    context: &mut ElephcEvalContext,
    class_name: &str,
    property_name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalOutcome>, EvalStatus> {
    let result = (|| {
        let name = ensure_generic_class(class_name, None, "", context, &mut ElephcEvalScope::new(), values)?;
        eval_static_property_set_result(&name, property_name, value, context, values)
    })();
    match result {
        Ok(()) => Ok(None),
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalOutcome::Throwable)
            .map(Some)
            .ok_or(EvalStatus::UncaughtThrowable),
        Err(status) => Err(status),
    }
}

/// Tests an object relation against eval dynamic-object metadata before AOT metadata.
pub fn execute_context_object_is_a(
    context: &mut ElephcEvalContext,
    object: RuntimeCellHandle,
    target_class: &str,
    exclude_self: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if values.type_tag(object)? != EVAL_TAG_OBJECT {
        return Ok(false);
    }
    let target_class = target_class.trim_start_matches('\\');
    let resolved_target_class = context
        .resolve_class_like_name(target_class)
        .unwrap_or_else(|| target_class.to_string());
    dynamic_object_is_a(
        object,
        &resolved_target_class,
        exclude_self,
        context,
        values,
    )?
    .map_or_else(
        || values.object_is_a(object, &resolved_target_class, exclude_self),
        Ok,
    )
}

/// Tests an object relation when the target is a runtime string or object cell.
pub fn execute_context_object_is_a_dynamic(
    context: &mut ElephcEvalContext,
    object: RuntimeCellHandle,
    target: RuntimeCellHandle,
    exclude_self: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let target_class = match values.type_tag(target)? {
        EVAL_TAG_STRING => {
            let bytes = values.string_bytes(target)?;
            let target = String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
            target.trim_start_matches('\\').to_string()
        }
        EVAL_TAG_OBJECT => {
            let identity = values.object_identity(target)?;
            if let Some(class) = context.dynamic_object_class(identity) {
                class.name().to_string()
            } else {
                let class_name = values.object_class_name(target)?;
                let bytes = values.string_bytes(class_name);
                values.release(class_name)?;
                let class_name = String::from_utf8(bytes?).map_err(|_| EvalStatus::RuntimeFatal)?;
                class_name.trim_start_matches('\\').to_string()
            }
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    execute_context_object_is_a(context, object, &target_class, exclude_self, values)
}

/// Tests whether a method or property exists through eval dynamic metadata.
pub fn execute_context_member_exists(
    context: &mut ElephcEvalContext,
    name: &str,
    target: RuntimeCellHandle,
    member: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let result = eval_member_exists_result(name, &[target, member], context, values)?;
    let exists = values.truthy(result)?;
    values.release(result)?;
    Ok(exists)
}
