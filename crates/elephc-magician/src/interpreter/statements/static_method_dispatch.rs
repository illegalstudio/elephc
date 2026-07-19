//! Purpose:
//! Resolves and dispatches eval-declared static method calls.
//!
//! Called from:
//! - Static-call expression evaluation.
//!
//! Key details:
//! - Called-class scope and private-method resolution are preserved across entry points.

use super::*;

/// Dispatches a static method call to an eval-declared static method.
pub(in crate::interpreter) fn eval_static_method_call_result(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let receiver = resolve_eval_static_method_receiver(class_name, context)?;
    eval_static_method_call_result_resolved(
        receiver.dispatch_class,
        receiver.called_class,
        method_name,
        evaluated_args,
        None,
        context,
        values,
    )
}

/// Dispatches a static-syntax method call from an expression scope that may hold `$this`.
pub(in crate::interpreter) fn eval_static_method_call_result_from_scope(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    scope: &ElephcEvalScope,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let receiver = resolve_eval_static_method_receiver(class_name, context)?;
    eval_static_method_call_result_resolved(
        receiver.dispatch_class,
        receiver.called_class,
        method_name,
        evaluated_args,
        Some(scope),
        context,
        values,
    )
}

/// Dispatches a static method call using a first-class callable's captured called class.
pub(in crate::interpreter) fn eval_static_method_call_result_with_called_class(
    class_name: &str,
    called_class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let class_name = resolve_eval_static_member_class_name(class_name, context)?;
    let called_class_name = context
        .resolve_class_name(called_class_name)
        .unwrap_or_else(|| called_class_name.trim_start_matches('\\').to_string());
    eval_static_method_call_result_resolved(
        class_name,
        called_class_name,
        method_name,
        evaluated_args,
        None,
        context,
        values,
    )
}

/// Dispatches a static method call after lookup and late-static names have been resolved.
pub(super) fn eval_static_method_call_result_resolved(
    class_name: String,
    called_class_name: String,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(result) = eval_closure_static_method_result(
        &class_name,
        method_name,
        evaluated_args.clone(),
        lexical_scope,
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_builtin_property_hook_type_static_method_result(
        &class_name,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if let Some(result) = eval_reflection_method_create_from_method_name_result(
        &class_name,
        method_name,
        evaluated_args.clone(),
        context,
        values,
    )? {
        return Ok(result);
    }
    if eval_enum_static_builtin_applies(&class_name, method_name, context).is_some() {
        return eval_enum_builtin_static_method_result(
            &class_name,
            method_name,
            evaluated_args,
            context,
            values,
        );
    }
    if let Some((declaring_class, method)) =
        eval_dynamic_static_method_for_call(&class_name, method_name, context)
    {
        if method.is_abstract() {
            return eval_throw_abstract_method_call_error(
                &declaring_class,
                method.name(),
                context,
                values,
            );
        }
        if validate_eval_member_access(&declaring_class, method.visibility(), context).is_err() {
            if let Some(result) = eval_magic_static_method_call(
                &class_name,
                &called_class_name,
                method_name,
                evaluated_args,
                context,
                values,
            )? {
                return Ok(result);
            }
            return eval_throw_method_access_error(
                &declaring_class,
                method.name(),
                method.visibility(),
                context,
                values,
            );
        }
        if !method.is_static() {
            if let Some(object) =
                eval_static_syntax_instance_receiver(&class_name, lexical_scope, context, values)?
            {
                return eval_dynamic_method_with_values(
                    &declaring_class,
                    &called_class_name,
                    &method,
                    object,
                    evaluated_args,
                    context,
                    values,
                );
            }
            return eval_throw_non_static_method_call_error(
                &declaring_class,
                method.name(),
                context,
                values,
            );
        }
        return eval_dynamic_static_method_with_values(
            &declaring_class,
            &called_class_name,
            &method,
            evaluated_args,
            context,
            values,
        );
    }
    if context.has_class(&class_name) {
        if let Some(parent) = context.class_native_parent_name(&class_name) {
            if let Some(result) = eval_native_static_syntax_method_result(
                &parent,
                Some(&called_class_name),
                method_name,
                evaluated_args.clone(),
                lexical_scope,
                context,
                values,
            )?
            {
                return Ok(result);
            }
        }
    }
    if context.has_class(&class_name)
        || context.has_interface(&class_name)
        || context.has_trait(&class_name)
        || context.has_enum(&class_name)
    {
        if let Some(result) = eval_magic_static_method_call(
            &class_name,
            &called_class_name,
            method_name,
            evaluated_args,
            context,
            values,
        )? {
            return Ok(result);
        }
        return eval_throw_undefined_method_call_error(
            &class_name,
            method_name,
            context,
            values,
        );
    }
    if let Some(result) = eval_native_static_syntax_method_result(
        &class_name,
        None,
        method_name,
        evaluated_args.clone(),
        lexical_scope,
        context,
        values,
    )? {
        return Ok(result);
    }
    eval_native_static_method_with_evaluated_args(
        &class_name,
        method_name,
        evaluated_args,
        context,
        values,
    )
}
