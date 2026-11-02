//! Purpose:
//! Executes eval-declared instance and static methods from prepared argument values.
//!
//! Called from:
//! - Method dispatch and callable invocation after target resolution.
//!
//! Key details:
//! - Called-class scope, reference modes, positional extraction, and writeback stay paired.

use super::*;

/// Returns the source site a class method's body runs under: its declaring file, that file's
/// directory, the method's first line and the `__FILE__` override naming the file.
fn dynamic_class_method_site(
    class_name: &str,
    method: &EvalClassMethod,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Option<(String, String, i64, Option<String>)> {
    // php keeps a trait method's declaring FILE: a `throw` inside
    // `CompiledUrlMatcherTrait::match()` is reported at the trait's `...Trait.php:70`, not at the
    // using class's (much shorter) file, which used to excerpt an empty window.
    let owner = method.trait_origin().unwrap_or(class_name);
    let file = match context.class_source_file(owner).map(str::to_string) {
        Some(file) => file,
        // A trait imported from a class-like the build already declared AOT has no runtime
        // include to record it; its declaring file survives in the generated reflection
        // metadata, which is the same answer `ReflectionClass::getFileName()` uses.
        None => match values.reflection_class_source_file(owner) {
            Ok(Some(file)) => file,
            _ => return None,
        },
    };
    let dir = std::path::Path::new(&file)
        .parent()
        .map(|parent| parent.to_string_lossy().into_owned())
        .unwrap_or_default();
    let line = method
        .source_location()
        .map_or(context.call_line(), |location| location.start_line() as i64);
    Some((file.clone(), dir, line, Some(file)))
}

/// Installs a class declaration's physical source as the active magic-constant site.
fn enter_dynamic_class_method_source(
    class_name: &str,
    method: &EvalClassMethod,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Option<(String, String, i64, Option<String>)> {
    let (file, dir, line, file_magic) =
        dynamic_class_method_site(class_name, method, context, values)?;
    let previous = context.call_site();
    context.set_call_site(file, dir, line);
    context.set_file_magic_override(file_magic);
    Some(previous)
}

/// Restores the caller's source metadata after a dynamic class method returns.
fn leave_dynamic_class_method_source(
    previous: Option<(String, String, i64, Option<String>)>,
    context: &mut ElephcEvalContext,
) {
    if let Some((file, dir, line, override_file)) = previous {
        context.set_call_site(file, dir, line);
        context.set_file_magic_override(override_file);
    }
}

/// Executes one eval-declared class method with `$this` bound in method scope.
pub(in crate::interpreter) fn eval_dynamic_method_with_values(
    class_name: &str,
    called_class_name: &str,
    method: &EvalClassMethod,
    object: RuntimeCellHandle,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_dynamic_method_with_values_and_ref_flags(
        class_name,
        called_class_name,
        method,
        object,
        method.parameter_is_by_ref(),
        evaluated_args,
        context,
        values,
    )
}

/// Executes one eval-declared class method with caller-selected by-ref binding flags.
pub(in crate::interpreter) fn eval_dynamic_method_with_values_and_ref_flags(
    class_name: &str,
    called_class_name: &str,
    method: &EvalClassMethod,
    object: RuntimeCellHandle,
    parameter_is_by_ref: &[bool],
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_dynamic_method_with_values_and_ref_mode(
        class_name,
        called_class_name,
        method,
        object,
        parameter_is_by_ref,
        evaluated_args,
        EvalByRefBindingMode::RequireTarget,
        context,
        values,
    )
}

/// Executes one eval-declared class method with caller-selected by-ref mode.
pub(in crate::interpreter) fn eval_dynamic_method_with_values_and_ref_mode(
    class_name: &str,
    called_class_name: &str,
    method: &EvalClassMethod,
    object: RuntimeCellHandle,
    parameter_is_by_ref: &[bool],
    evaluated_args: Vec<EvaluatedCallArg>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let specialized = specialize_generic_method(class_name, method, &evaluated_args, context, values)?;
    let method = &specialized;
    let qualified_method_name =
        format!("{}::{}", class_name.trim_start_matches('\\'), method.name());
    if crate::eval_trace::enabled() {
        let call_site = context.call_site();
        eprintln!(
            "[elephc-eval-trace] phase=dynamic_method_start method={qualified_method_name:?} called_class={called_class_name:?} object_identity={:?} file={:?} line={}",
            values.object_identity(object).ok(),
            call_site.0,
            call_site.2,
        );
    }
    let static_names = static_var_names(method.body());
    context.push_function(qualified_method_name.clone());
    context.push_class_scope(class_name.to_string());
    context.push_called_class_scope(called_class_name.to_string());
    context.push_method_magic_scope(class_name, method);
    // PHP reports the bare method name in `function` and the declaring class in `class`; the
    // receiver makes the frame an instance call, so `type` becomes `->`.
    let frame_args: Vec<_> = evaluated_args.iter().map(|arg| arg.value).collect();
    let frame_arg_count = frame_args.len();
    // Captured BEFORE binding so a refusal can name what actually arrived: a bare parameter
    // list says which type was wanted and never which one was offered.
    let traced_arg_tags: Vec<String> = if crate::eval_trace::enabled() {
        frame_args
            .iter()
            .map(|value| {
                let tag = values.type_tag(*value).unwrap_or(u64::MAX);
                // An object argument is named by CLASS: a refusal that only says "tag 6" cannot
                // distinguish "wrong class" from "class the context cannot resolve at all".
                let identity = values.object_identity(*value).unwrap_or(0);
                match crate::interpreter::runtime_object_class_name(*value, values) {
                    Ok(class_name) => format!("{tag}:{identity}:{class_name}"),
                    Err(_) => {
                        // An object whose class cannot be named is either a live object of an
                        // unregistered class or a FREED one whose storage was recycled. The
                        // header tells them apart: a live object payload starts with its class
                        // id and carries its refcount at -12.
                        let payload = values.raw_value_word(*value).unwrap_or(0);
                        let header = (tag == EVAL_TAG_OBJECT && payload >= 4096).then(|| unsafe {
                            let base = payload as *const u8;
                            (
                                *(base as *const u64),
                                *(base.sub(12) as *const u32),
                            )
                        });
                        format!("{tag}:{identity}:<unresolved>:header={header:?}")
                    }
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    let call_frame = EvalCallFrame::method(
        class_name,
        method.name(),
        Some(object),
        Some(frame_args),
        context,
    );
    context.push_call_frame(call_frame);
    let binding = match bind_evaluated_function_args_with_ref_mode(
        &qualified_method_name,
        method.params(),
        method.parameter_types(),
        method.parameter_defaults(),
        parameter_is_by_ref,
        method.parameter_is_variadic(),
        evaluated_args,
        by_ref_mode,
        context,
        values,
    ) {
        Ok(args) => args,
        Err(status) => {
            // Binding runs BEFORE the body, so a refusal here leaves no statement trace at all:
            // the last thing the log shows is `dynamic_method_start`, and the caller's frame
            // then reports a bare "unsupported <caller expression>". Naming the parameter list
            // here is what separates a binding refusal from a body refusal in one pass.
            if crate::eval_trace::enabled() {
                // The THROWABLE is what says why: an ArgumentCountError means a parameter default
                // did not materialize, a TypeError means an argument did not match. Peeked and put
                // back, so the caller still unwinds with it.
                let thrown = context.take_pending_throw();
                let thrown_class = thrown
                    .map(|value| {
                        let name = crate::interpreter::runtime_object_class_name(value, values)
                            .unwrap_or_else(|_| "<unnamed>".to_string());
                        context.set_pending_throw(value);
                        name
                    })
                    .unwrap_or_else(|| "<none>".to_string());
                eprintln!(
                    "[elephc-eval-trace] phase=dynamic_method_bind_error method={qualified_method_name:?} status={status:?} thrown={thrown_class} arg_count={frame_arg_count} arg_tags={traced_arg_tags:?} params={:?} types={:?} defaults={:?}",
                    method.params(),
                    method.parameter_types(),
                    method.parameter_defaults().len(),
                );
            }
            context.pop_call_frame();
            context.pop_magic_scope();
            context.pop_called_class_scope();
            context.pop_class_scope();
            context.pop_function();
            return Err(status);
        }
    };
    let BoundEvalFunctionArgs {
        params: binding_params,
        parameter_is_by_ref: binding_by_ref,
        args: evaluated_args,
        mut frame,
    } = binding;
    let mut method_scope = ElephcEvalScope::new();
    // A generator method's scope OUTLIVES this call, so its `$this` cannot stay a borrow of the
    // caller's reference: the receiver has to be kept alive by the generator itself. That is
    // also what makes PHP's ordering come out right for an `IteratorAggregate` whose
    // `getIterator()` is a generator — the aggregate survives until the generator is destroyed.
    if eval_body_is_generator(method.body()) {
        method_scope.set("this", values.retain(object)?, ScopeCellOwnership::Owned);
    } else {
        method_scope.set("this", object, ScopeCellOwnership::Borrowed);
    }
    let scope_parameter_is_by_ref =
        method_scope_parameter_ref_flags(&binding_by_ref, &evaluated_args, by_ref_mode);
    let binding_result = bind_method_scope_args(
        &mut method_scope,
        &binding_params,
        &scope_parameter_is_by_ref,
        &evaluated_args,
        context,
        values,
    );
    frame.bind_scope(&method_scope);
    frame.set_sensitive_parameters(method.parameter_attributes(), method.parameter_is_variadic());
    context.push_function_args_with_backtrace(
        frame,
        Some(class_name.trim_start_matches('\\').to_string()),
        Some(object),
        false,
    );
    let binding_result = binding_result.and_then(|()| {
        separate_array_scope_args(
            &mut method_scope,
            method.params(),
            &scope_parameter_is_by_ref,
            values,
        )
    });
    // A method whose body contains `yield` returns a Generator without executing a line, and its
    // activation travels with it: the class scope a later `next()` runs under is this method's,
    // not the resumer's.
    if eval_body_is_generator(method.body()) {
        let generator = binding_result.and_then(|()| {
            retain_generator_scope_args(
                &mut method_scope,
                method.params(),
                &scope_parameter_is_by_ref,
                &evaluated_args,
                values,
            )?;
            eval_generator_new(
                method.body(),
                std::mem::replace(&mut method_scope, ElephcEvalScope::new()),
                eval_method_activation(
                    qualified_method_name.clone(),
                    class_name,
                    called_class_name,
                    method,
                    dynamic_class_method_site(class_name, method, context, values),
                ),
                context,
                values,
            )
        });
        let generator = release_function_args(generator, context, values);
        context.pop_magic_scope();
        context.pop_called_class_scope();
        context.pop_class_scope();
        context.pop_call_frame();
        context.pop_function();
        return generator;
    }
    let previous_source = enter_dynamic_class_method_source(class_name, method, context, values);
    // The RETURN statement needs to know whether this body hands back a reference, and the flag
    // is saved and restored around the body so a nested call cannot leak its own answer out.
    // A body runs under the strict-types mode of the file that DECLARED it, not the file that
    // called it: php scopes the directive to the code doing the coercing, and a `return` is
    // written in the callee. The argument binding above already ran under the caller's mode,
    // which is the other half of the same rule.
    // A body's own `SourceLine` markers move the current line; the caller's is put back after,
    // so a diagnostic raised later in the CALLING statement still names the caller's line rather
    // than wherever the callee happened to stop.
    let previous_call_line = context.call_line();
    let previous_strict_types = context.set_strict_types(method.strict_types());
    let previous_by_ref = context.set_returns_by_ref(method.returns_by_ref());
    let result = binding_result.and_then(|()| execute_statements(method.body(), context, &mut method_scope, values));
    context.set_returns_by_ref(previous_by_ref);
    let persist_result = persist_static_locals(
        context,
        &qualified_method_name,
        &static_names,
        &method_scope,
        values,
    );
    let writeback_result = write_back_method_ref_args(
        method.params(),
        &evaluated_args,
        &method_scope,
        context,
        values,
    );
    let return_result = match (persist_result, writeback_result, result) {
        (Err(status), _, _) | (_, Err(status), _) | (_, _, Err(status)) => Err(status),
        (Ok(()), Ok(()), Ok(control)) => eval_declared_return_control_value(
            method.return_type(),
            Some(class_name),
            Some(called_class_name),
            control,
            context,
            values,
        ),
    };
    // Restored only now, AFTER the declared return type has been checked: that check coerces
    // the returned value, and the coercion is the callee's, so it obeys the callee's file.
    context.set_strict_types(previous_strict_types);
    context.set_call_line(previous_call_line);
    let return_result = retain_static_local_return(
        return_result,
        &static_names,
        &method_scope,
        values,
    );
    let return_result = release_function_args(return_result, context, values);
    leave_dynamic_class_method_source(previous_source, context);
    context.pop_magic_scope();
    context.pop_called_class_scope();
    context.pop_class_scope();
    let return_result = finish_activation_scope(&mut method_scope, return_result, context, values);
    context.pop_call_frame();
    context.pop_function();
    if crate::eval_trace::enabled() {
        let call_site = context.call_site();
        eprintln!(
            "[elephc-eval-trace] phase=dynamic_method_end method={qualified_method_name:?} success={} file={:?} line={}",
            return_result.is_ok(),
            call_site.0,
            call_site.2,
        );
    }
    return_result
}

/// Executes one eval-declared static class method without binding `$this`.
pub(in crate::interpreter) fn eval_dynamic_static_method_with_values(
    class_name: &str,
    called_class_name: &str,
    method: &EvalClassMethod,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_dynamic_static_method_with_values_and_ref_flags(
        class_name,
        called_class_name,
        method,
        method.parameter_is_by_ref(),
        evaluated_args,
        context,
        values,
    )
}

/// Executes one eval-declared static method with caller-selected by-ref binding flags.
pub(in crate::interpreter) fn eval_dynamic_static_method_with_values_and_ref_flags(
    class_name: &str,
    called_class_name: &str,
    method: &EvalClassMethod,
    parameter_is_by_ref: &[bool],
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_dynamic_static_method_with_values_and_ref_mode(
        class_name,
        called_class_name,
        method,
        parameter_is_by_ref,
        evaluated_args,
        EvalByRefBindingMode::RequireTarget,
        context,
        values,
    )
}

/// Executes one eval-declared static method with caller-selected by-ref mode.
pub(in crate::interpreter) fn eval_dynamic_static_method_with_values_and_ref_mode(
    class_name: &str,
    called_class_name: &str,
    method: &EvalClassMethod,
    parameter_is_by_ref: &[bool],
    evaluated_args: Vec<EvaluatedCallArg>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let specialized = specialize_generic_method(class_name, method, &evaluated_args, context, values)?;
    let method = &specialized;
    let qualified_method_name =
        format!("{}::{}", class_name.trim_start_matches('\\'), method.name());
    let static_names = static_var_names(method.body());
    context.push_function(qualified_method_name.clone());
    context.push_class_scope(class_name.to_string());
    context.push_called_class_scope(called_class_name.to_string());
    context.push_method_magic_scope(class_name, method);
    // A static call has no receiver, so PHP omits `object` and reports `type` as `::`.
    let frame_args = evaluated_args.iter().map(|arg| arg.value).collect();
    let call_frame = EvalCallFrame::method(class_name, method.name(), None, Some(frame_args), context);
    context.push_call_frame(call_frame);
    let binding = match bind_evaluated_function_args_with_ref_mode(
        &qualified_method_name,
        method.params(),
        method.parameter_types(),
        method.parameter_defaults(),
        parameter_is_by_ref,
        method.parameter_is_variadic(),
        evaluated_args,
        by_ref_mode,
        context,
        values,
    ) {
        Ok(args) => args,
        Err(status) => {
            context.pop_call_frame();
            context.pop_magic_scope();
            context.pop_called_class_scope();
            context.pop_class_scope();
            context.pop_function();
            return Err(status);
        }
    };
    let BoundEvalFunctionArgs {
        params: binding_params,
        parameter_is_by_ref: binding_by_ref,
        args: evaluated_args,
        mut frame,
    } = binding;
    let mut method_scope = ElephcEvalScope::new();
    let scope_parameter_is_by_ref =
        method_scope_parameter_ref_flags(&binding_by_ref, &evaluated_args, by_ref_mode);
    let binding_result = bind_method_scope_args(
        &mut method_scope,
        &binding_params,
        &scope_parameter_is_by_ref,
        &evaluated_args,
        context,
        values,
    );
    frame.bind_scope(&method_scope);
    frame.set_sensitive_parameters(method.parameter_attributes(), method.parameter_is_variadic());
    context.push_function_args_with_backtrace(
        frame,
        Some(class_name.trim_start_matches('\\').to_string()),
        None,
        true,
    );
    let binding_result = binding_result.and_then(|()| {
        separate_array_scope_args(
            &mut method_scope,
            method.params(),
            &scope_parameter_is_by_ref,
            values,
        )
    });
    // A method whose body contains `yield` returns a Generator without executing a line, and its
    // activation travels with it: the class scope a later `next()` runs under is this method's,
    // not the resumer's.
    if eval_body_is_generator(method.body()) {
        let generator = binding_result.and_then(|()| {
            retain_generator_scope_args(
                &mut method_scope,
                method.params(),
                &scope_parameter_is_by_ref,
                &evaluated_args,
                values,
            )?;
            eval_generator_new(
                method.body(),
                std::mem::replace(&mut method_scope, ElephcEvalScope::new()),
                eval_method_activation(
                    qualified_method_name.clone(),
                    class_name,
                    called_class_name,
                    method,
                    dynamic_class_method_site(class_name, method, context, values),
                ),
                context,
                values,
            )
        });
        let generator = release_function_args(generator, context, values);
        context.pop_magic_scope();
        context.pop_called_class_scope();
        context.pop_class_scope();
        context.pop_call_frame();
        context.pop_function();
        return generator;
    }
    let previous_source = enter_dynamic_class_method_source(class_name, method, context, values);
    // The RETURN statement needs to know whether this body hands back a reference, and the flag
    // is saved and restored around the body so a nested call cannot leak its own answer out.
    // A body runs under the strict-types mode of the file that DECLARED it, not the file that
    // called it: php scopes the directive to the code doing the coercing, and a `return` is
    // written in the callee. The argument binding above already ran under the caller's mode,
    // which is the other half of the same rule.
    // A body's own `SourceLine` markers move the current line; the caller's is put back after,
    // so a diagnostic raised later in the CALLING statement still names the caller's line rather
    // than wherever the callee happened to stop.
    let previous_call_line = context.call_line();
    let previous_strict_types = context.set_strict_types(method.strict_types());
    let previous_by_ref = context.set_returns_by_ref(method.returns_by_ref());
    let result = binding_result.and_then(|()| execute_statements(method.body(), context, &mut method_scope, values));
    context.set_returns_by_ref(previous_by_ref);
    let persist_result = persist_static_locals(
        context,
        &qualified_method_name,
        &static_names,
        &method_scope,
        values,
    );
    let writeback_result = write_back_method_ref_args(
        method.params(),
        &evaluated_args,
        &method_scope,
        context,
        values,
    );
    let return_result = match (persist_result, writeback_result, result) {
        (Err(status), _, _) | (_, Err(status), _) | (_, _, Err(status)) => Err(status),
        (Ok(()), Ok(()), Ok(control)) => eval_declared_return_control_value(
            method.return_type(),
            Some(class_name),
            Some(called_class_name),
            control,
            context,
            values,
        ),
    };
    // Restored only now, AFTER the declared return type has been checked: that check coerces
    // the returned value, and the coercion is the callee's, so it obeys the callee's file.
    context.set_strict_types(previous_strict_types);
    context.set_call_line(previous_call_line);
    let return_result = retain_static_local_return(
        return_result,
        &static_names,
        &method_scope,
        values,
    );
    let return_result = release_function_args(return_result, context, values);
    leave_dynamic_class_method_source(previous_source, context);
    context.pop_magic_scope();
    context.pop_called_class_scope();
    context.pop_class_scope();
    let return_result = finish_activation_scope(&mut method_scope, return_result, context, values);
    context.pop_call_frame();
    context.pop_function();
    return_result
}

/// Wraps positional method arguments into the shared dynamic-call binding shape.
pub(in crate::interpreter) fn positional_args(
    args: Vec<RuntimeCellHandle>,
) -> Vec<EvaluatedCallArg> {
    args.into_iter()
        .map(|value| EvaluatedCallArg {
            name: None,
            value: value.borrowed(),
            ref_target: None,
        })
        .collect()
}

/// Extracts positional runtime values and rejects named args before runtime method dispatch.
pub(in crate::interpreter) fn positional_evaluated_arg_values(
    args: Vec<EvaluatedCallArg>,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    if args.iter().any(|arg| arg.name.is_some()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(args.into_iter().map(|arg| arg.value).collect())
}
