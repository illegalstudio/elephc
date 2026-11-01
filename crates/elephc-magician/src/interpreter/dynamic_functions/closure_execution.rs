//! Purpose:
//! Executes eval-declared functions and closures with bound scope and captures.
//!
//! Called from:
//! - Dynamic function dispatch and callable/Closure invocation paths.
//!
//! Key details:
//! - Bound `$this`, class scope, reference captures, and static locals survive execution.

use super::*;

/// Evaluates an eval-declared function after its positional arguments are prepared.
pub(in crate::interpreter) fn eval_dynamic_function_with_values(
    function: &EvalFunction,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = evaluated_args
        .into_iter()
        .map(|value| EvaluatedCallArg {
            name: None,
            value: value.borrowed(),
            ref_target: None,
        })
        .collect();
    eval_dynamic_function_with_evaluated_args(function, evaluated_args, context, values)
}

/// Evaluates an eval-declared function after call arguments preserve names and ref targets.
pub(in crate::interpreter) fn eval_dynamic_function_with_evaluated_args(
    function: &EvalFunction,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_dynamic_function_with_evaluated_args_and_ref_flags(
        function,
        function.parameter_is_by_ref(),
        evaluated_args,
        context,
        values,
    )
}

/// Evaluates an eval-declared function with caller-selected by-ref binding flags.
pub(in crate::interpreter) fn eval_dynamic_function_with_evaluated_args_and_ref_flags(
    function: &EvalFunction,
    parameter_is_by_ref: &[bool],
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_dynamic_function_with_evaluated_args_and_ref_mode(
        function,
        parameter_is_by_ref,
        evaluated_args,
        EvalByRefBindingMode::RequireTarget,
        context,
        values,
    )
}

/// Evaluates an eval-declared function with caller-selected by-ref mode.
pub(in crate::interpreter) fn eval_dynamic_function_with_evaluated_args_and_ref_mode(
    function: &EvalFunction,
    parameter_is_by_ref: &[bool],
    evaluated_args: Vec<EvaluatedCallArg>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let static_names = static_var_names(function.body());
    context.push_function(function.name());
    // PHP names the frame after the callee and positions it at the caller, so both halves are
    // captured here, before the body can move the call site.
    let frame_args = evaluated_args.iter().map(|arg| arg.value).collect();
    let call_frame = EvalCallFrame::function(function.name(), Some(frame_args), context);
    context.push_call_frame(call_frame);
    // Named functions have global lexical scope even when a method calls them. Empty sentinels
    // hide the caller's class and late-static scopes until this activation returns.
    context.push_class_scope(String::new());
    context.push_called_class_scope(String::new());
    let bound_call = match bind_evaluated_function_args_with_ref_mode(
        function.name(),
        function.params(),
        function.parameter_types(),
        function.parameter_defaults(),
        parameter_is_by_ref,
        function.parameter_is_variadic(),
        evaluated_args,
        by_ref_mode,
        context,
        values,
    ) {
        Ok(args) => args,
        Err(status) => {
            context.pop_called_class_scope();
            context.pop_class_scope();
            context.pop_call_frame();
            context.pop_function();
            return Err(status);
        }
    };
    let BoundEvalFunctionArgs {
        params: binding_params,
        parameter_is_by_ref: binding_by_ref,
        args: evaluated_args,
        mut frame,
    } = bound_call;
    let scope_parameter_is_by_ref =
        method_scope_parameter_ref_flags(&binding_by_ref, &evaluated_args, by_ref_mode);
    let mut function_scope = ElephcEvalScope::new();
    let binding_result = bind_method_scope_args(
        &mut function_scope,
        &binding_params,
        &scope_parameter_is_by_ref,
        &evaluated_args,
        context,
        values,
    );
    let binding_result = binding_result.and_then(|()| {
        separate_array_scope_args(
            &mut function_scope,
            function.params(),
            &scope_parameter_is_by_ref,
            values,
        )
    });
    frame.bind_scope(&function_scope);
    frame.set_sensitive_parameters(function.parameter_attributes(), function.parameter_is_variadic());
    context.push_function_args(frame);
    // A body containing `yield` does not run here at all. PHP evaluates the arguments, binds
    // them, and hands back a Generator whose body starts only when it is first asked for a
    // value, so the bound scope becomes the generator's own and outlives this call.
    if eval_body_is_generator(function.body()) {
        let generator = binding_result.and_then(|()| {
            retain_generator_scope_args(
                &mut function_scope,
                function.params(),
                &scope_parameter_is_by_ref,
                &evaluated_args,
                values,
            )?;
            eval_generator_new(
                function.body(),
                std::mem::replace(&mut function_scope, ElephcEvalScope::new()),
                eval_plain_function_activation(function.name()),
                context,
                values,
            )
        });
        let generator = release_function_args(generator, context, values);
        context.pop_called_class_scope();
        context.pop_class_scope();
        context.pop_call_frame();
        context.pop_function();
        return generator;
    }
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
    let previous_strict_types = context.set_strict_types(function.strict_types());
    let previous_by_ref = context.set_returns_by_ref(function.returns_by_ref());
    let result = binding_result.and_then(|()| execute_statements(function.body(), context, &mut function_scope, values));
    context.set_returns_by_ref(previous_by_ref);
    let persist_result = persist_static_locals(
        context,
        function.name(),
        &static_names,
        &function_scope,
        values,
    );
    let writeback_result = write_back_method_ref_args(
        function.params(),
        &evaluated_args,
        &function_scope,
        context,
        values,
    );
    let return_result = match (persist_result, writeback_result, result) {
        (Err(status), _, _) | (_, Err(status), _) | (_, _, Err(status)) => Err(status),
        (Ok(()), Ok(()), Ok(control)) => eval_declared_return_control_value(
            function.return_type(),
            None,
            None,
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
        &function_scope,
        values,
    );
    let return_result = release_function_args(return_result, context, values);
    context.pop_called_class_scope();
    context.pop_class_scope();
    let return_result = finish_activation_scope(&mut function_scope, return_result, context, values);
    context.pop_call_frame();
    context.pop_function();
    return_result
}

/// Evaluates one runtime eval closure after callback arguments preserve names and ref targets.
pub(in crate::interpreter) fn eval_closure_with_evaluated_args(
    closure: &EvalClosure,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let binding = closure.declaring_class_scope().map(|class_scope| EvalClosureBinding {
        this_object: closure.declaring_this(),
        class_scope: class_scope.to_string(),
        called_class: closure
            .declaring_called_class_scope()
            .unwrap_or(class_scope)
            .to_string(),
    });
    if crate::eval_trace::enabled() {
        eprintln!(
            "[elephc-eval-trace] phase=closure_invoke function={:?} lexical_class={:?} called_class={:?} binding={}",
            closure.function().name(),
            closure.declaring_class_scope(),
            closure.declaring_called_class_scope(),
            binding.is_some(),
        );
    }
    eval_closure_with_optional_binding(
        closure,
        function_ref_flags(closure),
        EvalByRefBindingMode::RequireTarget,
        binding,
        evaluated_args,
        context,
        values,
    )
}

/// Evaluates one runtime eval closure with `$this` and an optional binding scope.
pub(in crate::interpreter) fn eval_closure_with_evaluated_args_and_bound_this_scope(
    closure: &EvalClosure,
    bound_this: RuntimeCellHandle,
    bound_scope: Option<String>,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if closure.is_static() {
        values.warning("Cannot bind an instance to a static closure")?;
        return values.null();
    }
    let called_class = eval_closure_bound_object_class_name(bound_this, context, values)?;
    let class_scope = bound_scope.unwrap_or_else(|| called_class.clone());
    eval_closure_with_optional_binding(
        closure,
        function_ref_flags(closure),
        EvalByRefBindingMode::RequireTarget,
        Some(EvalClosureBinding {
            this_object: Some(bound_this),
            class_scope,
            called_class,
        }),
        evaluated_args,
        context,
        values,
    )
}

/// Evaluates a runtime eval closure with `$this`, scope, and caller-selected by-ref flags.
pub(in crate::interpreter) fn eval_closure_with_evaluated_args_and_bound_this_scope_ref_flags(
    closure: &EvalClosure,
    bound_this: RuntimeCellHandle,
    bound_scope: Option<String>,
    parameter_is_by_ref: &[bool],
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if closure.is_static() {
        values.warning("Cannot bind an instance to a static closure")?;
        return values.null();
    }
    let called_class = eval_closure_bound_object_class_name(bound_this, context, values)?;
    let class_scope = bound_scope.unwrap_or_else(|| called_class.clone());
    eval_closure_with_optional_binding(
        closure,
        parameter_is_by_ref,
        EvalByRefBindingMode::RequireTarget,
        Some(EvalClosureBinding {
            this_object: Some(bound_this),
            class_scope,
            called_class,
        }),
        evaluated_args,
        context,
        values,
    )
}

/// Evaluates a runtime eval closure with `$this`, scope, and caller-selected by-ref mode.
pub(in crate::interpreter) fn eval_closure_with_evaluated_args_and_bound_this_scope_ref_mode(
    closure: &EvalClosure,
    bound_this: RuntimeCellHandle,
    bound_scope: Option<String>,
    parameter_is_by_ref: &[bool],
    evaluated_args: Vec<EvaluatedCallArg>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if closure.is_static() {
        values.warning("Cannot bind an instance to a static closure")?;
        return values.null();
    }
    let called_class = eval_closure_bound_object_class_name(bound_this, context, values)?;
    let class_scope = bound_scope.unwrap_or_else(|| called_class.clone());
    eval_closure_with_optional_binding(
        closure,
        parameter_is_by_ref,
        by_ref_mode,
        Some(EvalClosureBinding {
            this_object: Some(bound_this),
            class_scope,
            called_class,
        }),
        evaluated_args,
        context,
        values,
    )
}

/// Evaluates one runtime eval closure with a class scope but no `$this` binding.
pub(in crate::interpreter) fn eval_closure_with_evaluated_args_and_bound_scope(
    closure: &EvalClosure,
    bound_scope: Option<String>,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(class_scope) = bound_scope else {
        return eval_closure_with_evaluated_args(closure, evaluated_args, context, values);
    };
    eval_closure_with_optional_binding(
        closure,
        function_ref_flags(closure),
        EvalByRefBindingMode::RequireTarget,
        Some(EvalClosureBinding {
            this_object: None,
            called_class: class_scope.clone(),
            class_scope,
        }),
        evaluated_args,
        context,
        values,
    )
}

/// Evaluates a runtime eval closure with scope-only binding and caller-selected by-ref flags.
pub(in crate::interpreter) fn eval_closure_with_evaluated_args_and_bound_scope_ref_flags(
    closure: &EvalClosure,
    bound_scope: Option<String>,
    parameter_is_by_ref: &[bool],
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(class_scope) = bound_scope else {
        // No explicit rebinding: the closure keeps the `$this` and class scope it was declared
        // with, exactly as a direct `$closure(...)` call does. Passing no binding at all lost
        // them, so `call_user_func($default, ...)` on Laravel's
        // `fn () => ImplicitRouteBinding::resolveForRoute($this->container, $route)` warned
        // "Undefined variable $this" and resolved against a null container.
        let binding = closure.declaring_class_scope().map(|class_scope| EvalClosureBinding {
            this_object: closure.declaring_this(),
            class_scope: class_scope.to_string(),
            called_class: closure
                .declaring_called_class_scope()
                .unwrap_or(class_scope)
                .to_string(),
        });
        return eval_closure_with_optional_binding(
            closure,
            parameter_is_by_ref,
            EvalByRefBindingMode::RequireTarget,
            binding,
            evaluated_args,
            context,
            values,
        );
    };
    eval_closure_with_optional_binding(
        closure,
        parameter_is_by_ref,
        EvalByRefBindingMode::RequireTarget,
        Some(EvalClosureBinding {
            this_object: None,
            called_class: class_scope.clone(),
            class_scope,
        }),
        evaluated_args,
        context,
        values,
    )
}

/// Evaluates a scope-only runtime eval closure with caller-selected by-ref mode.
pub(in crate::interpreter) fn eval_closure_with_evaluated_args_and_bound_scope_ref_mode(
    closure: &EvalClosure,
    bound_scope: Option<String>,
    parameter_is_by_ref: &[bool],
    evaluated_args: Vec<EvaluatedCallArg>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(class_scope) = bound_scope else {
        return eval_closure_with_optional_binding(
            closure,
            parameter_is_by_ref,
            by_ref_mode,
            None,
            evaluated_args,
            context,
            values,
        );
    };
    eval_closure_with_optional_binding(
        closure,
        parameter_is_by_ref,
        by_ref_mode,
        Some(EvalClosureBinding {
            this_object: None,
            called_class: class_scope.clone(),
            class_scope,
        }),
        evaluated_args,
        context,
        values,
    )
}

/// Class binding metadata for a runtime eval closure invocation.
struct EvalClosureBinding {
    this_object: Option<RuntimeCellHandle>,
    class_scope: String,
    called_class: String,
}

/// Installs the physical file that declared a closure until its deferred body returns.
fn enter_closure_declaration_source(
    closure: &EvalClosure,
    context: &mut ElephcEvalContext,
) -> Option<(String, String, i64, Option<String>)> {
    let (file, dir, line, file_magic) = closure.declaring_call_site()?.clone();
    let previous = context.call_site();
    context.set_call_site(file, dir, line);
    context.set_file_magic_override(file_magic);
    Some(previous)
}

/// Restores the caller's source frame after a closure body completes or fails.
fn leave_closure_declaration_source(
    previous: Option<(String, String, i64, Option<String>)>,
    context: &mut ElephcEvalContext,
) {
    if let Some((file, dir, line, file_magic)) = previous {
        context.set_call_site(file, dir, line);
        context.set_file_magic_override(file_magic);
    }
}

/// Returns the name PHP gives a closure in diagnostics.
///
/// PHP 8.4 names a closure after where it is declared, `{closure:<file>:<line>}`; earlier
/// profiles print `{closure}`. Eval records the declaration line but neither the enclosing
/// function (PHP names a closure declared inside `f` as `{closure:f():<line>}`) nor the eval
/// site it was declared in, so this uses the current eval site. That is exact for a closure
/// declared at the top level of the eval fragment calling it.
fn eval_closure_display_name(function: &EvalFunction, context: &ElephcEvalContext) -> String {
    let file = context.eval_file_magic();
    match function.source_location() {
        Some(location)
            if crate::eval_php_profile::eval_php_minor_version() >= 4 && !file.is_empty() =>
        {
            format!("{{closure:{file}:{}}}", location.start_line())
        }
        _ => "{closure}".to_string(),
    }
}

/// Returns the closure function's declared by-reference parameter flags.
fn function_ref_flags(closure: &EvalClosure) -> &[bool] {
    closure.function().parameter_is_by_ref()
}

/// Evaluates one runtime eval closure with optional class and `$this` binding metadata.
///
/// Closures that capture a variable by reference keep a COPY of its cell for the length of the
/// activation and write it back on the way out. A recursive closure (`$walk = function ($n) use
/// (&$results, &$walk) { ... $walk($child); ... }`) therefore ran its inner activations against
/// the target variable while the outer one kept writing a stale cell over their work -- Laravel's
/// `Arr::dot()` returned only the top level. Before this activation starts, every activation
/// still waiting on a by-reference capture publishes its cell to the target; once this one has
/// finished, those that did not change theirs pick up what it left.
fn eval_closure_with_optional_binding(
    closure: &EvalClosure,
    parameter_is_by_ref: &[bool],
    by_ref_mode: EvalByRefBindingMode<'_>,
    binding: Option<EvalClosureBinding>,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let published = publish_active_capture_frames(context, values)?;
    let result = eval_closure_with_optional_binding_inner(
        closure,
        parameter_is_by_ref,
        by_ref_mode,
        binding,
        evaluated_args,
        context,
        values,
    );
    let refreshed = refresh_active_capture_frames(&published, context, values);
    match (result, refreshed) {
        (Err(status), _) | (Ok(_), Err(status)) => Err(status),
        (Ok(value), Ok(())) => Ok(value),
    }
}

/// One by-reference capture of a live activation.
struct ActiveCapture {
    name: String,
    target: EvalReferenceTarget,
    /// The cell this activation's entry held when it last agreed with the target. An entry that
    /// still holds it has not been written by the activation since, so it may adopt what a
    /// nested activation left in the target.
    synced: Option<RuntimeCellHandle>,
}

/// One live closure activation holding by-reference captures, innermost last.
struct ActiveCaptureFrame {
    scope: *mut ElephcEvalScope,
    captures: Vec<ActiveCapture>,
}

/// Which capture of which frame a nested activation published: frame index, scope, capture index.
type PublishedCapture = (usize, *mut ElephcEvalScope, usize);

thread_local! {
    /// Activations currently running a closure with by-reference captures.
    static ACTIVE_CAPTURE_FRAMES: std::cell::RefCell<Vec<ActiveCaptureFrame>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Removes an activation from the live capture frames when it ends, however it ends.
struct ActiveCaptureFrameGuard {
    registered: bool,
}

impl Drop for ActiveCaptureFrameGuard {
    fn drop(&mut self) {
        if self.registered {
            ACTIVE_CAPTURE_FRAMES.with(|frames| {
                frames.borrow_mut().pop();
            });
        }
    }
}

/// Registers an activation's by-reference captures and returns the guard that unregisters them.
fn register_active_capture_frame(
    scope: &mut ElephcEvalScope,
    captures: &[EvalClosureCaptureBinding],
) -> ActiveCaptureFrameGuard {
    let by_ref: Vec<ActiveCapture> = captures
        .iter()
        .filter_map(|capture| {
            capture.by_ref_target().map(|target| ActiveCapture {
                name: capture.name().to_string(),
                target: target.clone(),
                synced: scope.visible_cell(capture.name()),
            })
        })
        .collect();
    if by_ref.is_empty() {
        return ActiveCaptureFrameGuard { registered: false };
    }
    ACTIVE_CAPTURE_FRAMES.with(|frames| {
        frames.borrow_mut().push(ActiveCaptureFrame {
            scope: scope as *mut ElephcEvalScope,
            captures: by_ref,
        });
    });
    ActiveCaptureFrameGuard { registered: true }
}

/// Writes every waiting activation's by-reference capture cells to their targets.
///
/// Returns what was published so `refresh_active_capture_frames` can revisit exactly those.
fn publish_active_capture_frames(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<PublishedCapture>, EvalStatus> {
    let snapshot: Vec<(*mut ElephcEvalScope, Vec<(String, EvalReferenceTarget)>)> =
        ACTIVE_CAPTURE_FRAMES.with(|frames| {
            frames
                .borrow()
                .iter()
                .map(|frame| {
                    (
                        frame.scope,
                        frame
                            .captures
                            .iter()
                            .map(|capture| (capture.name.clone(), capture.target.clone()))
                            .collect(),
                    )
                })
                .collect()
        });
    let mut published = Vec::new();
    for (index, (scope_ptr, captures)) in snapshot.into_iter().enumerate() {
        let Some(scope) = (unsafe { scope_ptr.as_ref() }) else {
            continue;
        };
        for (capture_index, (name, target)) in captures.into_iter().enumerate() {
            let Some(cell) = scope
                .entry(&name)
                .filter(|entry| entry.flags().is_visible() && entry.flags().by_ref)
                .map(|entry| entry.cell())
            else {
                continue;
            };
            write_back_method_ref_target(&target, cell, context, values)?;
            ACTIVE_CAPTURE_FRAMES.with(|frames| {
                if let Some(capture) = frames
                    .borrow_mut()
                    .get_mut(index)
                    .filter(|frame| frame.scope == scope_ptr)
                    .and_then(|frame| frame.captures.get_mut(capture_index))
                {
                    capture.synced = Some(cell);
                }
            });
            published.push((index, scope_ptr, capture_index));
        }
    }
    Ok(published)
}

/// Points activations that did not touch a capture since publishing it at what the target holds.
fn refresh_active_capture_frames(
    published: &[PublishedCapture],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for (index, scope_ptr, capture_index) in published {
        let capture = ACTIVE_CAPTURE_FRAMES.with(|frames| {
            frames
                .borrow()
                .get(*index)
                .filter(|frame| frame.scope == *scope_ptr)
                .and_then(|frame| frame.captures.get(*capture_index))
                .map(|capture| (capture.name.clone(), capture.target.clone(), capture.synced))
        });
        let Some((name, target, synced)) = capture else {
            continue;
        };
        let Some(scope) = (unsafe { scope_ptr.as_mut() }) else {
            continue;
        };
        if visible_scope_cell(context, scope, &name) != synced {
            continue;
        }
        let Some(current) = live_variable_capture_cell(&target, context) else {
            continue;
        };
        if Some(current) == synced {
            continue;
        }
        for replaced in
            scope.set_respecting_references(name.clone(), current, ScopeCellOwnership::Borrowed)
        {
            eval_release_value(context, values, replaced)?;
        }
        ACTIVE_CAPTURE_FRAMES.with(|frames| {
            if let Some(capture) = frames
                .borrow_mut()
                .get_mut(*index)
                .filter(|frame| frame.scope == *scope_ptr)
                .and_then(|frame| frame.captures.get_mut(*capture_index))
            {
                capture.synced = Some(current);
            }
        });
    }
    Ok(())
}

/// Runs one runtime eval closure activation; see `eval_closure_with_optional_binding`.
fn eval_closure_with_optional_binding_inner(
    closure: &EvalClosure,
    parameter_is_by_ref: &[bool],
    by_ref_mode: EvalByRefBindingMode<'_>,
    binding: Option<EvalClosureBinding>,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let function = closure.function();
    let static_names = static_var_names(function.body());
    // The frame keeps the FUNCTION name, which php prints as `{closure}`; only the static slots
    // move to the per-object key.
    context.push_static_slot_key(closure.slot_key());
    let bound_class_pushed = binding.is_some();
    // Captured before `binding` is consumed below, because a generator closure keeps the scopes
    // it was created under rather than inheriting whoever resumes it.
    let bound_class_scope = binding.as_ref().map(|binding| binding.class_scope.clone());
    let bound_called_class = binding.as_ref().map(|binding| binding.called_class.clone());
    let previous_source = enter_closure_declaration_source(closure, context);
    context.push_function(function.name());
    if let Some(binding) = &binding {
        context.push_class_scope(binding.class_scope.clone());
        context.push_called_class_scope(binding.called_class.clone());
    }
    // A closure is a plain-function frame in PHP even when it is bound to a class: the trace
    // shows no `class` key for it, only the generated closure name.
    let frame_args = evaluated_args.iter().map(|arg| arg.value).collect();
    let call_frame = EvalCallFrame::function(function.name(), Some(frame_args), context);
    context.push_call_frame(call_frame);
    let bound_call = match bind_evaluated_function_args_with_ref_mode(
        &eval_closure_display_name(function, context),
        function.params(),
        function.parameter_types(),
        function.parameter_defaults(),
        parameter_is_by_ref,
        function.parameter_is_variadic(),
        evaluated_args,
        by_ref_mode,
        context,
        values,
    ) {
        Ok(args) => args,
        Err(status) => {
            if bound_class_pushed {
                context.pop_called_class_scope();
                context.pop_class_scope();
            }
            context.pop_call_frame();
            context.pop_function();
            context.pop_static_slot_key();
            leave_closure_declaration_source(previous_source, context);
            return Err(status);
        }
    };
    let BoundEvalFunctionArgs {
        params: binding_params,
        parameter_is_by_ref: binding_by_ref,
        args: evaluated_args,
        mut frame,
    } = bound_call;
    let mut function_scope = ElephcEvalScope::new();
    let bound_capture_cells = bind_closure_captures(&mut function_scope, closure.captures(), context);
    let backtrace_class = binding
        .as_ref()
        .map(|binding| binding.class_scope.clone());
    let backtrace_object = binding.as_ref().and_then(|binding| binding.this_object);
    if let Some(object) = backtrace_object {
        function_scope.set("this", object, ScopeCellOwnership::Borrowed);
    }
    let scope_parameter_is_by_ref =
        method_scope_parameter_ref_flags(&binding_by_ref, &evaluated_args, by_ref_mode);
    let binding_result = bind_method_scope_args(
        &mut function_scope,
        &binding_params,
        &scope_parameter_is_by_ref,
        &evaluated_args,
        context,
        values,
    );
    frame.bind_scope(&function_scope);
    frame.set_sensitive_parameters(function.parameter_attributes(), function.parameter_is_variadic());
    context.push_function_args_with_backtrace(
        frame,
        backtrace_class,
        backtrace_object,
        backtrace_object.is_none(),
    );
    let binding_result = binding_result.and_then(|()| {
        separate_array_scope_args(
            &mut function_scope,
            function.params(),
            &scope_parameter_is_by_ref,
            values,
        )
    });
    let _capture_frame = if eval_body_is_generator(function.body()) {
        ActiveCaptureFrameGuard { registered: false }
    } else {
        register_active_capture_frame(&mut function_scope, closure.captures())
    };
    // A closure whose body contains `yield` is a generator function too: calling it produces the
    // Generator object and runs nothing, and the scope built above — captures, `$this` and the
    // bound arguments — becomes the generator's own.
    if eval_body_is_generator(function.body()) {
        let generator = binding_result.and_then(|()| {
            eval_generator_new(
                function.body(),
                std::mem::replace(&mut function_scope, ElephcEvalScope::new()),
                eval_closure_activation(
                    function.name(),
                    bound_class_scope,
                    bound_called_class,
                    closure.declaring_call_site().cloned(),
                ),
                context,
                values,
            )
        });
        let generator = release_function_args(generator, context, values);
        context.pop_call_frame();
        if bound_class_pushed {
            context.pop_called_class_scope();
            context.pop_class_scope();
        }
        context.pop_function();
        context.pop_static_slot_key();
        leave_closure_declaration_source(previous_source, context);
        return generator;
    }
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
    let previous_strict_types = context.set_strict_types(function.strict_types());
    let previous_by_ref = context.set_returns_by_ref(function.returns_by_ref());
    let result = binding_result.and_then(|()| execute_statements(function.body(), context, &mut function_scope, values));
    context.set_returns_by_ref(previous_by_ref);
    let persist_result = persist_static_locals(
        context,
        function.name(),
        &static_names,
        &function_scope,
        values,
    );
    let capture_writeback_result =
        write_back_closure_ref_captures(
            closure.captures(),
            &bound_capture_cells,
            &function_scope,
            context,
            values,
        )
            .and_then(|()| keep_closure_ref_capture_cells(closure, &function_scope, context, values));
    let arg_writeback_result = write_back_method_ref_args(
        function.params(),
        &evaluated_args,
        &function_scope,
        context,
        values,
    );
    let return_result = match (
        persist_result,
        capture_writeback_result,
        arg_writeback_result,
        result,
    ) {
        (Err(status), _, _, _)
        | (_, Err(status), _, _)
        | (_, _, Err(status), _)
        | (_, _, _, Err(status)) => Err(status),
        (Ok(()), Ok(()), Ok(()), Ok(control)) => eval_declared_return_control_value(
            function.return_type(),
            None,
            None,
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
        &function_scope,
        values,
    );
    if bound_class_pushed {
        context.pop_called_class_scope();
        context.pop_class_scope();
    }
    let return_result = release_function_args(return_result, context, values);
    let return_result = finish_activation_scope(&mut function_scope, return_result, context, values);
    context.pop_call_frame();
    context.pop_function();
    context.pop_static_slot_key();
    leave_closure_declaration_source(previous_source, context);
    return_result
}

/// Returns the PHP class name used as the bound scope for `Closure::call()`.
pub(in crate::interpreter) fn eval_closure_bound_object_class_name(
    object: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    if values.type_tag(object)? != EVAL_TAG_OBJECT {
        return Err(EvalStatus::RuntimeFatal);
    }
    if let Ok(identity) = values.object_identity(object) {
        if let Some(class) = context.dynamic_object_class(identity) {
            return Ok(class.name().to_string());
        }
    }
    runtime_object_class_name(object, values)
}

/// Seeds one closure activation scope with values captured when the closure was created.
///
/// A by-reference capture reads what its variable holds NOW while the defining activation is
/// still alive: that activation may have reassigned it since the closure was created (`$n = 5`
/// after `use (&$n)`), releasing the cell the capture started with. Once the activation has
/// returned, the closure's own cell -- kept current after every call -- is the value.
fn bind_closure_captures(
    function_scope: &mut ElephcEvalScope,
    captures: &[EvalClosureCaptureBinding],
    context: &ElephcEvalContext,
) -> Vec<(String, RuntimeCellHandle)> {
    let mut bound_cells = Vec::new();
    for capture in captures {
        if let Some(target) = capture.by_ref_target().cloned() {
            let cell = live_variable_capture_cell(&target, context).unwrap_or(capture.value());
            bound_cells.push((capture.name().to_string(), cell));
            function_scope.set_reference(
                capture.name().to_string(),
                capture.name().to_string(),
                cell,
                ScopeCellOwnership::Borrowed,
            );
            function_scope.set_reference_target(capture.name().to_string(), target);
        } else {
            function_scope.set(
                capture.name().to_string(),
                capture.value(),
                ScopeCellOwnership::Borrowed,
            );
        }
    }
    bound_cells
}

/// Returns the cell a by-reference capture's variable holds, while its activation is alive.
fn live_variable_capture_cell(
    target: &EvalReferenceTarget,
    context: &ElephcEvalContext,
) -> Option<RuntimeCellHandle> {
    let EvalReferenceTarget::Variable { scope, name } = target else {
        return None;
    };
    let scope = unsafe { scope.live_mut() }?;
    visible_scope_cell(context, scope, name)
}

/// Stores each by-reference capture's final cell back on the registered closure.
///
/// The write-back above updates the DEFINING variable, which is gone once that activation
/// returns; the closure's own binding is then the only place the value survives, so the next
/// call of `function () use (&$n) { return ++$n; }` must start from what this one left.
fn keep_closure_ref_capture_cells(
    closure: &EvalClosure,
    function_scope: &ElephcEvalScope,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for capture in closure.captures() {
        if capture.by_ref_target().is_none() {
            continue;
        }
        let Some(cell) = function_scope.visible_cell(capture.name()) else {
            continue;
        };
        let retained = values.retain(cell)?;
        let released = context
            .replace_closure_capture_value(closure.slot_key(), capture.name(), retained)
            .unwrap_or(retained);
        eval_release_value(context, values, released)?;
    }
    Ok(())
}

/// Writes modified by-reference closure captures back to their defining caller targets.
fn write_back_closure_ref_captures(
    captures: &[EvalClosureCaptureBinding],
    bound_cells: &[(String, RuntimeCellHandle)],
    function_scope: &ElephcEvalScope,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for capture in captures {
        let Some(target) = capture.by_ref_target() else {
            continue;
        };
        let Some(entry) = function_scope
            .entry(capture.name())
            .filter(|entry| entry.flags().is_visible() && entry.flags().by_ref)
        else {
            continue;
        };
        let final_cell = entry.cell();
        // The cell the closure started from: a plain assignment inside the body already wrote
        // through to the target, so the target no longer holds it by now.
        let bound_cell = bound_cells
            .iter()
            .find(|(name, _)| name == capture.name())
            .map(|(_, cell)| *cell);
        write_back_method_ref_target(target, final_cell, context, values)?;
        if let Some(defining) = capture.defining_target() {
            sync_defining_variable(defining, bound_cell, final_cell, context, values)?;
        }
    }
    Ok(())
}

/// Moves the defining scope's own entry for a by-reference capture to the closure's final cell.
///
/// `function f(&$p) { $g = function () use (&$p) { $p = 5; }; $g(); return $p; }`: the capture's
/// write-back target is the CALLER's variable, so `$p` in `f`'s scope kept the cell the closure
/// replaced -- freed by then -- and read back garbage. The entry is only moved while it still
/// holds the cell the closure bound, which keeps a loop variable rebound since the capture
/// (`foreach (... as &$item)`) pointing at its new element.
fn sync_defining_variable(
    defining: &EvalReferenceTarget,
    bound_cell: Option<RuntimeCellHandle>,
    final_cell: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let EvalReferenceTarget::Variable { scope, name } = defining else {
        return Ok(());
    };
    let Some(scope) = (unsafe { scope.live_mut() }) else {
        return Ok(());
    };
    let Some(current) = visible_scope_cell(context, scope, name) else {
        return Ok(());
    };
    if Some(current) != bound_cell || current == final_cell {
        return Ok(());
    }
    for replaced in scope.set_respecting_references(name.clone(), final_cell, ScopeCellOwnership::Borrowed) {
        eval_release_value(context, values, replaced)?;
    }
    Ok(())
}

/// Persists static local variables from one eval-declared function activation.
pub(in crate::interpreter) fn persist_static_locals(
    context: &mut ElephcEvalContext,
    function_name: &str,
    names: &[String],
    scope: &ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for name in names {
        if let Some(cell) = scope.visible_cell(name) {
            if context.static_local(function_name, name) == Some(cell) { continue; }
            // The store keeps this value until the NEXT call, long after the activation's scope
            // is drained, so it has to hold a reference of its own. Without it a body that
            // reassigned the static — the only reason the value is worth persisting — handed the
            // store a cell the scope was about to release, and the following call gave that dead
            // cell back a second time when it replaced it.
            let cell = values.retain(cell)?;
            if let Some(replaced) =
                context.set_static_local(function_name.to_string(), name.clone(), cell)
            {
                values.release(replaced)?;
            }
        }
    }
    Ok(())
}

/// Retains a borrowed return before its activation releases local owners.
pub(in crate::interpreter) fn retain_static_local_return(
    result: Result<RuntimeCellHandle, EvalStatus>,
    _static_names: &[String],
    _scope: &ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let result = result?;
    if result.is_borrowed() { values.retain(result) } else { Ok(result) }
}

/// One source-order static local declaration and its initializer expression.
#[derive(Clone)]
pub(in crate::interpreter) struct EvalStaticVarInitializer {
    pub name: String,
    pub init: EvalExpr,
}

/// Returns the distinct static local names declared anywhere in an eval function body.
pub(in crate::interpreter) fn static_var_names(body: &[EvalStmt]) -> Vec<String> {
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    visit_static_var_declarations(body, &mut seen, &mut |name, _| {
        names.push(name.to_string());
    });
    names
}

/// Returns static local declarations and initializers in first-seen source order.
pub(in crate::interpreter) fn static_var_initializers(
    body: &[EvalStmt],
) -> Vec<EvalStaticVarInitializer> {
    let mut vars = Vec::new();
    let mut seen = std::collections::HashSet::new();
    visit_static_var_declarations(body, &mut seen, &mut |name, init| {
        vars.push(EvalStaticVarInitializer {
            name: name.to_string(),
            init: init.clone(),
        });
    });
    vars
}

/// Visits distinct static local declarations in first-seen source order.
fn visit_static_var_declarations(
    body: &[EvalStmt],
    seen: &mut std::collections::HashSet<String>,
    visitor: &mut impl FnMut(&str, &EvalExpr),
) {
    for stmt in body {
        match stmt {
            EvalStmt::StaticVar { name, init } => {
                if seen.insert(name.clone()) {
                    visitor(name, init);
                }
            }
            EvalStmt::DoWhile { body, .. }
            | EvalStmt::Foreach { body, .. }
            | EvalStmt::For { body, .. }
            | EvalStmt::While { body, .. } => visit_static_var_declarations(body, seen, visitor),
            EvalStmt::FunctionDecl { .. } | EvalStmt::GenericDecl(_) | EvalStmt::SourceLine(_) => {}
            // A `declare(…) { … }` body is ordinary statements in the SAME scope, so a `static`
            // written inside one is the enclosing function's, exactly as in an `if` branch.
            EvalStmt::DeclareTicks { body, .. } | EvalStmt::DeclareDirective { body, .. } => {
                if let Some(body) = body {
                    visit_static_var_declarations(body, seen, visitor);
                }
            }
            EvalStmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                visit_static_var_declarations(then_branch, seen, visitor);
                visit_static_var_declarations(else_branch, seen, visitor);
            }
            EvalStmt::Switch { cases, .. } => {
                for case in cases {
                    visit_static_var_declarations(&case.body, seen, visitor);
                }
            }
            EvalStmt::Try {
                body,
                catches,
                finally_body,
            } => {
                visit_static_var_declarations(body, seen, visitor);
                for catch in catches {
                    visit_static_var_declarations(&catch.body, seen, visitor);
                }
                visit_static_var_declarations(finally_body, seen, visitor);
            }
            EvalStmt::DeclareStrictTypes(_)
            | EvalStmt::ArrayAppendVar { .. }
            | EvalStmt::ArrayAppend { .. }
            | EvalStmt::ArrayAppendReferenceBind { .. }
            | EvalStmt::ArrayDestructure { .. }
            | EvalStmt::ArrayReferenceBind { .. }
            | EvalStmt::ArraySetVar { .. }
            | EvalStmt::Break(_)
            | EvalStmt::ClassDecl(_)
            | EvalStmt::Continue(_)
            | EvalStmt::Echo(_)
            | EvalStmt::EnumDecl(_)
            | EvalStmt::Expr(_)
            | EvalStmt::Global { .. }
            | EvalStmt::Goto(_)
            | EvalStmt::InterfaceDecl(_)
            | EvalStmt::Label(_)
            | EvalStmt::DynamicPropertyArrayAppend { .. }
            | EvalStmt::DynamicPropertyArraySet { .. }
            | EvalStmt::DynamicPropertyCompoundAssign { .. }
            | EvalStmt::DynamicPropertyIncDec { .. }
            | EvalStmt::DynamicPropertyReferenceBind { .. }
            | EvalStmt::DynamicPropertySet { .. }
            | EvalStmt::DynamicStaticPropertyArrayAppend { .. }
            | EvalStmt::DynamicStaticPropertyArraySet { .. }
            | EvalStmt::DynamicStaticPropertyIncDec { .. }
            | EvalStmt::DynamicStaticPropertyReferenceBind { .. }
            | EvalStmt::DynamicStaticPropertyNameArrayAppend { .. }
            | EvalStmt::DynamicStaticPropertyNameArraySet { .. }
            | EvalStmt::DynamicStaticPropertyNameIncDec { .. }
            | EvalStmt::DynamicStaticPropertyNameReferenceBind { .. }
            | EvalStmt::DynamicStaticPropertyNameSet { .. }
            | EvalStmt::DynamicStaticPropertySet { .. }
            | EvalStmt::PropertyReferenceBind { .. }
            | EvalStmt::PropertyArrayAppend { .. }
            | EvalStmt::PropertyArraySet { .. }
            | EvalStmt::PropertyCompoundAssign { .. }
            | EvalStmt::PropertyIncDec { .. }
            | EvalStmt::PropertySet { .. }
            | EvalStmt::ReferenceAssign { .. }
            | EvalStmt::VarReferenceBind { .. }
            | EvalStmt::Return(_)
            | EvalStmt::StaticPropertyArrayAppend { .. }
            | EvalStmt::StaticPropertyArraySet { .. }
            | EvalStmt::StaticPropertyIncDec { .. }
            | EvalStmt::StaticPropertyReferenceBind { .. }
            | EvalStmt::StaticPropertySet { .. }
            | EvalStmt::TypedVar { .. }
            | EvalStmt::StoreVar { .. }
            | EvalStmt::Throw(_)
            | EvalStmt::TraitDecl(_)
            | EvalStmt::UnsetArrayElement { .. }
            | EvalStmt::UnsetDynamicProperty { .. }
            | EvalStmt::UnsetDynamicStaticProperty { .. }
            | EvalStmt::UnsetDynamicStaticPropertyName { .. }
            | EvalStmt::UnsetProperty { .. }
            | EvalStmt::UnsetStaticProperty { .. }
            | EvalStmt::UnsetDynamicVar { .. }
            | EvalStmt::UnsetVar { .. }
            | EvalStmt::GcCollect => {}
        }
    }
}
