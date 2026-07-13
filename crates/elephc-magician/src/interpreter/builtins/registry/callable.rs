//! Purpose:
//! Resolves PHP callbacks into normalized callable targets.
//! Invocation strategies live in focused child modules.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry` re-exports.
//!
//! Key details:
//! - Helpers are scoped to the eval interpreter and operate on already parsed
//!   EvalIR call metadata or evaluated runtime-cell handles.
//! - Callback normalization remains centralized while dispatch is grouped by
//!   evaluated, object, static, and call-array entry surfaces.

mod array_dispatch;
mod execution;
mod object_dispatch;
mod static_dispatch;

use super::*;

pub(in crate::interpreter) use array_dispatch::*;
pub(in crate::interpreter) use execution::*;
use object_dispatch::*;
use static_dispatch::*;

/// Distinguishes PHP's invokable-object callback form from an explicit method callback.
#[derive(Clone, Copy, PartialEq, Eq)]
enum EvalObjectCallbackKind {
    /// A bare object callback whose `__invoke` magic method is callable regardless of visibility.
    InvokableObject,
    /// An explicit object-method callback that must pass normal visibility validation.
    Method,
}

/// Dispatches `call_user_func_array` with optional lexical scope for special class receivers.
pub(in crate::interpreter) fn eval_call_user_func_array_with_values_from_scope(
    callback: RuntimeCellHandle,
    arg_array: RuntimeCellHandle,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let callback = eval_call_user_func_callback(
        callback,
        "call_user_func_array",
        lexical_scope,
        context,
        values,
    )?;
    if !values.is_array_like(arg_array)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let evaluated_args = eval_array_call_arg_values(arg_array, context, values)?;
    eval_evaluated_callable_with_call_array_args(&callback, evaluated_args, context, values)
}

/// Dispatches `call_user_func` with optional lexical scope for special class receivers.
pub(in crate::interpreter) fn eval_call_user_func_with_values_from_scope(
    evaluated_args: Vec<RuntimeCellHandle>,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((callback, callback_args)) = evaluated_args.split_first() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let callback =
        eval_call_user_func_callback(*callback, "call_user_func", lexical_scope, context, values)?;
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
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvaluatedCallable, EvalStatus> {
    match eval_callable_with_optional_scope(callback, context, lexical_scope, values) {
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
    eval_callable_with_optional_scope(callback, context, None, values)
}

/// Normalizes one PHP callback while retaining the current method scope when available.
pub(in crate::interpreter) fn eval_callable_from_scope(
    callback: RuntimeCellHandle,
    context: &ElephcEvalContext,
    scope: &ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvaluatedCallable, EvalStatus> {
    eval_callable_with_optional_scope(callback, context, Some(scope), values)
}

/// Normalizes one PHP callback with optional scope-sensitive special class receivers.
pub(in crate::interpreter) fn eval_callable_with_optional_scope(
    callback: RuntimeCellHandle,
    context: &ElephcEvalContext,
    lexical_scope: Option<&ElephcEvalScope>,
    values: &mut impl RuntimeValueOps,
) -> Result<EvaluatedCallable, EvalStatus> {
    if values.type_tag(callback)? == EVAL_TAG_OBJECT {
        return eval_object_callable(callback, context, values);
    }
    if values.is_array_like(callback)? {
        return eval_array_callable(callback, context, lexical_scope, values);
    }
    eval_string_callable(callback, context, lexical_scope, values)
}

/// Normalizes one invokable eval object for dynamic callable dispatch.
pub(in crate::interpreter) fn eval_object_callable(
    callback: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvaluatedCallable, EvalStatus> {
    let identity = values.object_identity(callback)?;
    if let Some(target) = context.closure_object_target(identity) {
        return Ok(eval_closure_object_target_callable(target));
    }
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

/// Converts a PHP-visible eval `Closure` object target into the shared callback enum.
fn eval_closure_object_target_callable(target: &EvalClosureObjectTarget) -> EvaluatedCallable {
    match target {
        EvalClosureObjectTarget::Named(name) => EvaluatedCallable::Named {
            name: name.clone(),
            display_name: name.clone(),
        },
        EvalClosureObjectTarget::BoundNamed {
            name,
            bound_this,
            bound_scope,
        } => EvaluatedCallable::BoundClosure {
            name: name.clone(),
            bound_this: *bound_this,
            bound_scope: bound_scope.clone(),
        },
        EvalClosureObjectTarget::InvokableObject { object } => EvaluatedCallable::InvokableObject {
            object: *object,
        },
        EvalClosureObjectTarget::ObjectMethod {
            object,
            method,
            called_class,
            native_class,
            bridge_scope,
        } => EvaluatedCallable::ObjectMethod {
            object: *object,
            method: method.clone(),
            called_class: called_class.clone(),
            native_class: native_class.clone(),
            bridge_scope: bridge_scope.clone(),
        },
        EvalClosureObjectTarget::StaticMethod {
            class_name,
            method,
            called_class,
            native_class,
            bridge_scope,
        } => EvaluatedCallable::StaticMethod {
            class_name: class_name.clone(),
            method: method.clone(),
            called_class: called_class.clone(),
            native_class: native_class.clone(),
            bridge_scope: bridge_scope.clone(),
        },
    }
}

/// Normalizes one two-element object-method or static-method callable array.
pub(in crate::interpreter) fn eval_array_callable(
    callback: RuntimeCellHandle,
    context: &ElephcEvalContext,
    lexical_scope: Option<&ElephcEvalScope>,
    values: &mut impl RuntimeValueOps,
) -> Result<EvaluatedCallable, EvalStatus> {
    if values.array_len(callback)? != 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let zero = values.int(0)?;
    let one = match values.int(1) {
        Ok(one) => one,
        Err(status) => {
            values.release(zero)?;
            return Err(status);
        }
    };
    let receiver = match values.array_get(callback, zero) {
        Ok(receiver) => receiver,
        Err(status) => {
            values.release(zero)?;
            values.release(one)?;
            return Err(status);
        }
    };
    let method = match values.array_get(callback, one) {
        Ok(method) => method,
        Err(status) => {
            values.release(zero)?;
            values.release(one)?;
            return Err(status);
        }
    };
    values.release(zero)?;
    values.release(one)?;
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
            if let Some(callable) = eval_special_class_array_callable(
                &class_name,
                &method,
                lexical_scope,
                context,
                values,
            )? {
                return Ok(callable);
            }
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

/// Resolves deprecated `self`/`static`/`parent` callable arrays inside method scope.
fn eval_special_class_array_callable(
    class_name: &str,
    method: &str,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvaluatedCallable>, EvalStatus> {
    if !eval_callable_array_receiver_is_special_class_name(class_name) {
        return Ok(None);
    }
    let Some(scope) = lexical_scope else {
        return Ok(None);
    };
    let receiver = resolve_eval_static_method_receiver(class_name, context)?;
    let use_instance_receiver =
        !eval_special_class_array_static_method_exists(&receiver.dispatch_class, method, context, values)?;
    if use_instance_receiver {
        if let Some(object) =
            eval_static_syntax_instance_receiver(&receiver.dispatch_class, Some(scope), context, values)?
        {
            return Ok(Some(EvaluatedCallable::ObjectMethod {
                object,
                method: method.to_string(),
                called_class: Some(receiver.called_class),
                native_class: None,
                bridge_scope: None,
            }));
        }
    }
    Ok(Some(EvaluatedCallable::StaticMethod {
        class_name: receiver.dispatch_class,
        method: method.to_string(),
        called_class: Some(receiver.called_class),
        native_class: None,
        bridge_scope: None,
    }))
}

/// Returns whether a callable-array receiver is PHP's deprecated special class string.
fn eval_callable_array_receiver_is_special_class_name(class_name: &str) -> bool {
    matches!(
        class_name.trim_start_matches('\\').to_ascii_lowercase().as_str(),
        "self" | "static" | "parent"
    )
}

/// Returns whether a special class callable array names a real static method.
fn eval_special_class_array_static_method_exists(
    class_name: &str,
    method: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if let Some((_, method)) = eval_dynamic_static_method_for_call(class_name, method, context) {
        return Ok(method.is_static());
    }
    let native_class = if context.has_class(class_name) {
        let Some(parent) = context.class_native_parent_name(class_name) else {
            return Ok(false);
        };
        parent
    } else if context.has_interface(class_name)
        || context.has_trait(class_name)
        || context.has_enum(class_name)
    {
        return Ok(false);
    } else {
        class_name.to_string()
    };
    Ok(eval_aot_method_dispatch_metadata_in_hierarchy(&native_class, method, context, values)?
        .is_some_and(|(_, _, is_static, _)| is_static))
}

/// Normalizes one string callback name for eval dynamic callable dispatch.
/// Uses method lexical scope only for PHP APIs that resolve deprecated `self::`,
/// `static::`, and `parent::` string callbacks through the current method.
pub(in crate::interpreter) fn eval_string_callable(
    callback: RuntimeCellHandle,
    context: &ElephcEvalContext,
    lexical_scope: Option<&ElephcEvalScope>,
    values: &mut impl RuntimeValueOps,
) -> Result<EvaluatedCallable, EvalStatus> {
    let callback = values.string_bytes(callback)?;
    let callback = String::from_utf8(callback).map_err(|_| EvalStatus::RuntimeFatal)?;
    if let Some((class_name, method)) = callback.split_once("::") {
        if class_name.is_empty() || method.is_empty() {
            return Err(EvalStatus::RuntimeFatal);
        }
        if let Some(callable) = eval_special_class_array_callable(
            class_name,
            method,
            lexical_scope,
            context,
            values,
        )? {
            return Ok(callable);
        }
        return Ok(EvaluatedCallable::StaticMethod {
            class_name: class_name.trim_start_matches('\\').to_string(),
            method: method.to_string(),
            called_class: None,
            native_class: None,
            bridge_scope: None,
        });
    }
    let display_name = callback.trim_start_matches('\\').to_string();
    Ok(EvaluatedCallable::Named {
        name: display_name.to_ascii_lowercase(),
        display_name,
    })
}
