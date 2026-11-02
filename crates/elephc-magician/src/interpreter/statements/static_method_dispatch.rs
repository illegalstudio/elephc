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
    eval_reject_fiber_switch_during_pcntl_dispatch(
        &class_name,
        method_name,
        context,
        values,
    )?;
    // Every step below can refuse, and until this trace existed a refusal anywhere in the chain
    // surfaced as one anonymous fatal with no way to tell which step it was.
    if crate::eval_trace::enabled() {
        eprintln!(
            "[elephc-eval-trace] phase=static_method_dispatch class={class_name:?} method={method_name:?} context_owns={} args={}",
            context.has_class(&class_name),
            evaluated_args.len(),
        );
    }
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
    // `parent::createFromFormat()` from an interpreted subclass of a native class resolves the
    // class to the native parent, but the late-static class is still the subclass: php's
    // `DateTime::createFromFormat()` returns `static`, and Carbon's `rawCreateFromFormat()` relies
    // on getting a Carbon back. Without the called class the factory built a plain DateTime.
    let late_static_class = (!called_class_name.eq_ignore_ascii_case(&class_name)
        && context.has_class(&called_class_name))
    .then_some(called_class_name.as_str());
    let native = eval_native_static_syntax_method_result(
        &class_name,
        late_static_class,
        method_name,
        evaluated_args.clone(),
        lexical_scope,
        context,
        values,
    );
    if crate::eval_trace::enabled() {
        eprintln!(
            "[elephc-eval-trace] phase=static_method_native class={class_name:?} method={method_name:?} outcome={}",
            match &native {
                Ok(Some(_)) => "found".to_string(),
                Ok(None) => "absent".to_string(),
                Err(status) => format!("{status:?}"),
            },
        );
    }
    if let Some(result) = native? {
        return Ok(result);
    }
    // PHP gives registered SPL autoloaders one chance to materialize an otherwise unknown class
    // before reporting the static call as undefined. Re-dispatch after a successful load so the
    // normal eval-declared or AOT-native method paths remain the single execution mechanism.
    //
    // The retry is only sound for a class NOTHING knows yet. `eval_spl_autoload_class` answers
    // true as soon as the target exists, without loading anything, so for a class the runtime
    // already has — an AOT class whose static method simply is not the one being called — it
    // reported success on every attempt and this re-dispatch took the identical path again. That
    // is not a deep recursion, it is an unbounded one: four tests aborted the whole test PROCESS
    // with a stack overflow rather than failing.
    if !values.class_exists(&class_name)?
        && eval_spl_autoload_class(&class_name, context, values)?
    {
        // The autoloader may have made the name known as an ALIAS (`class_alias($real, $alias)`,
        // which is exactly what a framework's facade loader does). The names above were resolved
        // BEFORE that alias existed, so they still spell the alias; resolve them again, or the
        // late-static class of the call stays the alias spelling the compiled side cannot map to
        // a class (`static::class` read "Route" and `static::$prop` read another class's slot).
        let resolved_class_name = context
            .resolve_class_name(&class_name)
            .unwrap_or_else(|| class_name.clone());
        let called_class_name = if called_class_name.eq_ignore_ascii_case(&class_name) {
            resolved_class_name.clone()
        } else {
            called_class_name
        };
        return eval_static_method_call_result_resolved(
            resolved_class_name,
            called_class_name,
            method_name,
            evaluated_args,
            lexical_scope,
            context,
            values,
        );
    }
    // The last resort, and the one whose failure said nothing. Everything above either found the
    // method or threw a PHP error naming it; reaching here means the class is known to neither the
    // context nor the autoloaders, so the AOT bridge is being asked for it blind. When that
    // answers with a status the interpreter can only report an anonymous fatal, which is what a
    // Symfony `class_exists()` reaching `ClassExistenceResource::throwOnRequiredClass` looked like.
    let result =
        eval_native_static_method_with_evaluated_args(&class_name, method_name, evaluated_args, context, values);
    if let Err(status) = &result {
        if crate::eval_trace::enabled() {
            let call_site = context.call_site();
            eprintln!(
                "[elephc-eval-trace] phase=static_method_unresolved class={class_name:?} method={method_name:?} status={status:?} context_owns={} file={:?} line={}",
                context.has_class(&class_name),
                call_site.0,
                call_site.2,
            );
        }
    }
    result
}
