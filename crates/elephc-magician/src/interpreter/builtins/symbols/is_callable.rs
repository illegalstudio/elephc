//! Purpose:
//! Eval registry entry and implementation for `is_callable`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Direct and dynamic-ref paths preserve `$callable_name` writeback.
//! - Syntax-only callable checks avoid resolving non-object string targets.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "is_callable",
    area: Symbols,
    params: [
        value,
        syntax_only = EvalBuiltinDefaultValue::Bool(false),
        callable_name: by_ref = EvalBuiltinDefaultValue::Null
    ],
    by_ref: [callable_name],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Evaluates direct `is_callable(...)` calls inside an eval fragment.
pub(in crate::interpreter) fn eval_is_callable_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_is_callable(args, context, scope, values)
}

/// Evaluates materialized `is_callable(...)` arguments.
pub(in crate::interpreter) fn eval_is_callable_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_is_callable_with_values(evaluated_args, context, values)
}

/// Evaluates `is_callable()` over full eval call metadata so `$callable_name` stays writable.
pub(in crate::interpreter) fn eval_builtin_is_callable_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = eval_call_arg_values(args, context, scope, values)?;
    eval_is_callable_call_with_evaluated_args_from_scope(
        &evaluated_args,
        Some(scope),
        context,
        values,
    )
}

/// Evaluates `is_callable()` from already evaluated arguments that may retain ref targets.
pub(in crate::interpreter) fn eval_is_callable_call_with_evaluated_args(
    evaluated_args: &[EvaluatedCallArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_is_callable_call_with_evaluated_args_from_scope(evaluated_args, None, context, values)
}

/// Evaluates materialized `is_callable()` args with optional special-class callable scope.
fn eval_is_callable_call_with_evaluated_args_from_scope(
    evaluated_args: &[EvaluatedCallArg],
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (bound, _) = bind_evaluated_ref_builtin_args(
        &["value", "syntax_only", "callable_name"],
        evaluated_args,
        false,
    )?;
    let value = required_evaluated_ref_arg(&bound, 0)?;
    let syntax_only = optional_evaluated_ref_arg(&bound, 1)
        .map(|arg| values.truthy(arg.value))
        .transpose()?
        .unwrap_or(false);
    let callable_name_target = optional_evaluated_ref_arg(&bound, 2)
        .map(|arg| arg.ref_target.clone().ok_or(EvalStatus::RuntimeFatal))
        .transpose()?;
    eval_is_callable_result(
        value.value,
        syntax_only,
        callable_name_target.as_ref(),
        lexical_scope,
        context,
        values,
    )
}

/// Evaluates by-value dynamic `is_callable()` arguments without `$callable_name` writeback.
pub(in crate::interpreter) fn eval_is_callable_with_values(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [value] => eval_is_callable_result(*value, false, None, None, context, values),
        [value, syntax_only] => {
            let syntax_only = values.truthy(*syntax_only)?;
            eval_is_callable_result(*value, syntax_only, None, None, context, values)
        }
        [value, syntax_only, _callable_name] => {
            let syntax_only = values.truthy(*syntax_only)?;
            eval_is_callable_result(*value, syntax_only, None, None, context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates positional `is_callable()` arguments inside an eval fragment.
pub(in crate::interpreter) fn eval_builtin_is_callable(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_is_callable_result(value, false, None, Some(scope), context, values)
        }
        [value, syntax_only] => {
            let value = eval_expr(value, context, scope, values)?;
            let syntax_only = eval_expr(syntax_only, context, scope, values)?;
            let syntax_only = values.truthy(syntax_only)?;
            eval_is_callable_result(value, syntax_only, None, Some(scope), context, values)
        }
        [value, syntax_only, callable_name] => {
            let value = eval_expr(value, context, scope, values)?;
            let syntax_only = eval_expr(syntax_only, context, scope, values)?;
            let syntax_only = values.truthy(syntax_only)?;
            let (_, callable_name_target) =
                eval_call_arg_value(callable_name, context, scope, values)?;
            let callable_name_target = callable_name_target.ok_or(EvalStatus::RuntimeFatal)?;
            eval_is_callable_result(
                value,
                syntax_only,
                Some(&callable_name_target),
                Some(scope),
                context,
                values,
            )
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Returns whether one runtime value is callable from the current eval scope.
pub(in crate::interpreter) fn eval_is_callable_value(
    value: RuntimeCellHandle,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let callback = match lexical_scope {
        Some(scope) => eval_callable_from_scope(value, context, scope, values),
        None => eval_callable(value, context, values),
    };
    let Ok(callback) = callback else {
        return Ok(false);
    };
    eval_callable_probe_exists(&callback, context, values)
}

/// Evaluates `is_callable()` and writes PHP's display callable name when requested.
fn eval_is_callable_result(
    value: RuntimeCellHandle,
    syntax_only: bool,
    callable_name_target: Option<&EvalReferenceTarget>,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let callable_name = callable_name_target
        .map(|_| eval_callable_display_name(value, context, values))
        .transpose()?;
    let callable = if syntax_only {
        eval_is_callable_syntax_only(value, context, values)?
    } else {
        eval_is_callable_value(value, lexical_scope, context, values)?
    };
    if let Some((target, name)) = callable_name_target.zip(callable_name.as_deref()) {
        let name = values.string(name)?;
        eval_write_direct_ref_target(
            target,
            name,
            context,
            values,
            Some(ScopeCellOwnership::Owned),
        )?;
    }
    values.bool_value(callable)
}

/// Returns PHP's syntax-only callable result without requiring the target to exist.
fn eval_is_callable_syntax_only(
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if values.type_tag(value)? == EVAL_TAG_STRING {
        return Ok(true);
    }
    if values.type_tag(value)? == EVAL_TAG_OBJECT {
        return eval_is_callable_value(value, None, context, values);
    }
    if values.is_array_like(value)? {
        return eval_callable_array_display_name(value, context, values).map(|name| name.is_some());
    }
    Ok(false)
}

/// Builds PHP's `$callable_name` output for one probed callable value.
fn eval_callable_display_name(
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    if values.type_tag(value)? == EVAL_TAG_STRING {
        let bytes = values.string_bytes(value)?;
        return String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal);
    }
    if values.type_tag(value)? == EVAL_TAG_OBJECT {
        let class_name = eval_callable_object_class_name(value, context, values)?;
        return Ok(format!("{class_name}::__invoke"));
    }
    if values.is_array_like(value)? {
        return Ok(eval_callable_array_display_name(value, context, values)?
            .unwrap_or_else(|| String::from("Array")));
    }
    let string = values.cast_string(value)?;
    let bytes = values.string_bytes(string);
    values.release(string)?;
    String::from_utf8(bytes?).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Builds PHP's `$callable_name` output for a syntactically valid callable array.
fn eval_callable_array_display_name(
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    if values.array_len(value)? != 2 {
        return Ok(None);
    }
    let zero = values.int(0)?;
    let one = values.int(1)?;
    let receiver = values.array_get(value, zero)?;
    let method = values.array_get(value, one)?;
    if values.type_tag(method)? != EVAL_TAG_STRING {
        return Ok(None);
    }
    let method =
        String::from_utf8(values.string_bytes(method)?).map_err(|_| EvalStatus::RuntimeFatal)?;
    let receiver_name = match values.type_tag(receiver)? {
        EVAL_TAG_OBJECT => eval_callable_object_class_name(receiver, context, values)?,
        EVAL_TAG_STRING => String::from_utf8(values.string_bytes(receiver)?)
            .map_err(|_| EvalStatus::RuntimeFatal)?,
        _ => return Ok(None),
    };
    Ok(Some(format!("{receiver_name}::{method}")))
}

/// Returns the PHP-visible class name used when formatting callable object probes.
fn eval_callable_object_class_name(
    object: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let identity = values.object_identity(object)?;
    if context.closure_object_target(identity).is_some() {
        return Ok(String::from("Closure"));
    }
    if let Some(class_name) = context.dynamic_object_class_name(identity) {
        return Ok(class_name);
    }
    runtime_object_class_name(object, values)
}

/// Returns whether a normalized eval callback has an invokable target.
fn eval_callable_probe_exists(
    callback: &EvaluatedCallable,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    match callback {
        EvaluatedCallable::Named { name, .. } => Ok(context.has_closure(name)
            || super::function_exists::eval_function_probe_exists(context, name)),
        EvaluatedCallable::BoundClosure { name, .. } => Ok(context.has_closure(name)
            || super::function_exists::eval_function_probe_exists(context, name)),
        EvaluatedCallable::InvokableObject { object } => {
            eval_object_method_callable_probe(*object, "__invoke", context, values)
        }
        EvaluatedCallable::ObjectMethod {
            object,
            method,
            native_class,
            ..
        } => {
            if native_class.is_some() {
                Ok(true)
            } else {
                eval_object_method_callable_probe(*object, method, context, values)
            }
        }
        EvaluatedCallable::StaticMethod {
            class_name,
            method,
            native_class,
            ..
        } => {
            if native_class.is_some() {
                Ok(true)
            } else {
                eval_static_method_callable_probe(class_name, method, context, values)
            }
        }
    }
}

/// Returns whether one object method can be called from the current eval scope.
fn eval_object_method_callable_probe(
    object: RuntimeCellHandle,
    method_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        return Ok(false);
    };
    if context.closure_object_target(identity).is_some()
        && method_name.eq_ignore_ascii_case("__invoke")
    {
        return Ok(true);
    }
    let Some(class) = context.dynamic_object_class(identity) else {
        return eval_aot_object_method_callable_probe(object, method_name, context, values);
    };
    if eval_enum_static_builtin_applies(class.name(), method_name, context).is_some() {
        return Ok(true);
    }
    let Some((declaring_class, method)) =
        eval_dynamic_method_for_call(class.name(), method_name, context)
    else {
        if eval_dynamic_class_native_method_callable_probe(
            class.name(),
            method_name,
            context,
            values,
        )? {
            return Ok(true);
        }
        return Ok(eval_instance_magic_method_callable_probe(
            class.name(),
            context,
        ));
    };
    if method.is_abstract() {
        return Ok(false);
    }
    if method_name.eq_ignore_ascii_case("__invoke") {
        return Ok(true);
    }
    Ok(validate_eval_member_access(&declaring_class, method.visibility(), context).is_ok()
        || eval_instance_magic_method_callable_probe(class.name(), context))
}

/// Returns whether one static method can be called from the current eval scope.
fn eval_static_method_callable_probe(
    class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let class_name = resolve_eval_static_member_class_name(class_name, context)?;
    if eval_enum_static_builtin_applies(&class_name, method_name, context).is_some() {
        return Ok(true);
    }
    if let Some((declaring_class, method)) = context.class_method(&class_name, method_name) {
        if !method.is_static() || method.is_abstract() {
            return Ok(false);
        }
        return Ok(validate_eval_member_access(&declaring_class, method.visibility(), context)
            .is_ok()
            || eval_static_magic_method_callable_probe(&class_name, context));
    }
    if context.has_class(&class_name)
        || context.has_interface(&class_name)
        || context.has_trait(&class_name)
        || context.has_enum(&class_name)
    {
        if eval_static_magic_method_callable_probe(&class_name, context) {
            return Ok(true);
        }
        if let Some(parent) = context.class_native_parent_name(&class_name) {
            return eval_aot_static_method_callable_probe(
                &parent,
                method_name,
                context,
                values,
            );
        }
        return Ok(false);
    }
    eval_aot_static_method_callable_probe(&class_name, method_name, context, values)
}

/// Returns whether a generated/AOT object method can be called from the current eval scope.
fn eval_aot_object_method_callable_probe(
    object: RuntimeCellHandle,
    method_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let class_name = runtime_object_class_name(object, values)?;
    eval_aot_class_method_callable_probe(&class_name, method_name, context, values)
}

/// Returns whether an eval class can call a generated/AOT parent instance method.
fn eval_dynamic_class_native_method_callable_probe(
    class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some(parent) = context.class_native_parent_name(class_name) else {
        return Ok(false);
    };
    eval_aot_class_method_callable_probe(&parent, method_name, context, values)
}

/// Returns whether one generated/AOT class instance method can be called from eval.
fn eval_aot_class_method_callable_probe(
    class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some((declaring_class, visibility, _, is_abstract)) =
        eval_aot_method_dispatch_metadata_in_hierarchy(&class_name, method_name, context, values)?
    else {
        return eval_aot_instance_magic_method_callable_probe(&class_name, context, values);
    };
    if is_abstract {
        return Ok(false);
    }
    Ok(validate_eval_member_access(&declaring_class, visibility, context).is_ok()
        || eval_aot_instance_magic_method_callable_probe(&class_name, context, values)?)
}

/// Returns whether a generated/AOT static method can be called from the current eval scope.
fn eval_aot_static_method_callable_probe(
    class_name: &str,
    method_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let Some((declaring_class, visibility, is_static, is_abstract)) =
        eval_aot_method_dispatch_metadata_in_hierarchy(class_name, method_name, context, values)?
    else {
        return eval_aot_static_magic_method_callable_probe(class_name, context, values);
    };
    if !is_static || is_abstract {
        return Ok(false);
    }
    Ok(validate_eval_member_access(&declaring_class, visibility, context).is_ok()
        || eval_aot_static_magic_method_callable_probe(class_name, context, values)?)
}

/// Returns whether a generated/AOT class has a callable instance `__call()` fallback.
fn eval_aot_instance_magic_method_callable_probe(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(eval_aot_method_dispatch_metadata_in_hierarchy(class_name, "__call", context, values)?
        .is_some_and(|(_, _, is_static, is_abstract)| !is_static && !is_abstract))
}

/// Returns whether a generated/AOT class has a callable static `__callStatic()` fallback.
fn eval_aot_static_magic_method_callable_probe(
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

/// Returns whether an eval class has a callable instance `__call()` fallback.
fn eval_instance_magic_method_callable_probe(
    class_name: &str,
    context: &ElephcEvalContext,
) -> bool {
    context
        .class_method(class_name, "__call")
        .is_some_and(|(_, method)| !method.is_static() && !method.is_abstract())
}

/// Returns whether an eval class has a callable static `__callStatic()` fallback.
fn eval_static_magic_method_callable_probe(class_name: &str, context: &ElephcEvalContext) -> bool {
    context
        .class_method(class_name, "__callStatic")
        .is_some_and(|(_, method)| method.is_static() && !method.is_abstract())
}
