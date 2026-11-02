//! Purpose:
//! Executes try, catch, and finally control-flow semantics.
//!
//! Called from:
//! - Statement dispatch for EvalIR exception handlers.
//!
//! Key details:
//! - Finally control overrides and throwable interface matching remain explicit.

use super::*;

/// Executes an eval `try` body and handles supported `catch` clauses.
pub(in crate::interpreter) fn execute_try_stmt(
    body: &[EvalStmt],
    catches: &[EvalCatch],
    finally_body: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    // PHP runs `catch` and `finally` in the class scope of the method that DECLARED them. The
    // interpreter keeps one class-scope stack per context and shares it across nested calls, so a
    // try body that unwound through deeper frames can leave an unrelated class on top -- the
    // Symfony `--web` request reached this with the generated container's class there while the
    // finally belonged to `EnvVarProcessor::getEnv`, and `$this->loaders` (private) was then
    // refused with `Cannot access private property`. Remembering the scope the try STATEMENT
    // started under and restoring it for the handlers keeps the visibility check on the same
    // class PHP would use, whatever the unwinding left behind.
    let entry_class_scope = context.current_class_scope().map(str::to_string);
    let control = match execute_statements(body, context, scope, values) {
        Ok(EvalControl::Throw(thrown)) => {
            let restored = restore_entry_class_scope(entry_class_scope.as_deref(), context);
            let caught = execute_matching_catch(thrown, catches, context, scope, values);
            if restored {
                context.pop_class_scope();
            }
            caught
        }
        Err(EvalStatus::UncaughtThrowable) => {
            let pending = context.take_pending_throw();
            if crate::eval_trace::enabled() {
                eprintln!(
                    "[elephc-eval-trace] phase=try_uncaught pending={} catches={}",
                    pending.is_some(),
                    catches.len(),
                );
            }
            let Some(thrown) = pending else {
                return Err(EvalStatus::UncaughtThrowable);
            };
            let restored = restore_entry_class_scope(entry_class_scope.as_deref(), context);
            let caught = execute_matching_catch(thrown, catches, context, scope, values);
            if restored {
                context.pop_class_scope();
            }
            caught
        }
        Ok(control) => Ok(control),
        Err(status) => Err(status),
    };
    // Native calls and nested eval report throws through the context, including from a catch.
    // Transfer that pending owner into control flow before finally runs or replaces the throw.
    let control = match control {
        Ok(control) => control,
        Err(EvalStatus::UncaughtThrowable) => context
            .take_pending_throw()
            .map(EvalControl::Throw)
            .ok_or(EvalStatus::UncaughtThrowable)?,
        Err(status) => {
            // A `try` can only consult its catches for `UncaughtThrowable`; every other status
            // leaves here without ever looking at them. If a throwable reaches an enclosing try
            // under some OTHER status, this is where it slipped past the one that should have
            // caught it -- so say which status did it.
            if crate::eval_trace::enabled() {
                eprintln!(
                    "[elephc-eval-trace] phase=try_bypass status={status:?} catches={}",
                    catches.len(),
                );
            }
            return Err(status);
        }
    };
    if finally_body.is_empty() {
        return Ok(control);
    }
    // A storage read must survive a finally block that replaces or unsets its source binding.
    let control = match control {
        EvalControl::Return(value) if value.is_borrowed() => {
            EvalControl::Return(values.retain(value)?)
        }
        EvalControl::Throw(value) if value.is_borrowed() => {
            EvalControl::Throw(values.retain(value)?)
        }
        control => control,
    };
    let restored = restore_entry_class_scope(entry_class_scope.as_deref(), context);
    let finally_result = execute_statements(finally_body, context, scope, values);
    if restored {
        context.pop_class_scope();
    }
    match finally_result {
        Ok(EvalControl::None) => Ok(control),
        Ok(finally_control) => {
            release_overridden_control(control, values)?;
            Ok(finally_control)
        }
        Err(status) => {
            release_overridden_control(control, values)?;
            Err(status)
        }
    }
}

/// Re-enters the class scope a `try` statement started under, when the stack has moved on.
///
/// Returns whether a scope was pushed, so the caller pops exactly what it pushed. Pushing rather
/// than rewriting the stack keeps every other frame's entry intact, which matters because the
/// frames below are still live and will pop their own entries as they unwind.
fn restore_entry_class_scope(
    entry_class_scope: Option<&str>,
    context: &mut ElephcEvalContext,
) -> bool {
    let Some(entry) = entry_class_scope else {
        return false;
    };
    if context.current_class_scope() == Some(entry) {
        return false;
    }
    context.push_class_scope(entry.to_string());
    true
}

/// Releases a pending control-flow value when `finally` replaces that action.
pub(in crate::interpreter) fn release_overridden_control(
    control: EvalControl,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    match control {
        EvalControl::Return(value) | EvalControl::Throw(value) => values.release(value),
        EvalControl::None
        | EvalControl::ReturnVoid
        | EvalControl::Break(_)
        | EvalControl::Continue(_)
        | EvalControl::Goto(_) => Ok(()),
    }
}

/// Executes the first supported catch clause for a thrown eval object.
pub(in crate::interpreter) fn execute_matching_catch(
    thrown: RuntimeCellHandle,
    catches: &[EvalCatch],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let mut matched = None;
    for catch in catches {
        if catch_types_match_thrown(thrown, &catch.class_names, context, values)? {
            matched = Some(catch);
            break;
        }
    }
    let Some(catch) = matched else {
        return Ok(EvalControl::Throw(thrown));
    };
    if let Some(var_name) = &catch.var_name {
        for replaced in set_owned_scope_cell(
            context,
            scope,
            var_name.clone(),
            thrown,
            values,
        )? {
            values.release(replaced)?;
        }
    } else {
        values.release(thrown)?;
    }
    execute_statements(&catch.body, context, scope, values)
}

/// Returns true when any type in one catch clause accepts the thrown object.
pub(in crate::interpreter) fn catch_types_match_thrown(
    thrown: RuntimeCellHandle,
    class_names: &[String],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    for class_name in class_names {
        let class_name = class_name.trim_start_matches('\\');
        // Throwable uses the same interface matcher as other catch types. Native
        // control-flow objects, including coroutine cancellation, are not Throwables.
        let dynamic = dynamic_object_is_a(thrown, class_name, false, context, values)?;
        let native = match dynamic {
            Some(_) => None,
            None => Some(values.object_is_a(thrown, class_name, false)?),
        };
        // Which catch clause accepted a throwable -- and which declined -- is the first thing worth
        // knowing when an exception escapes a `try` that should have caught it. Exceptional path
        // only: nothing reaches here unless something was actually thrown.
        if crate::eval_trace::enabled() {
            eprintln!(
                "[elephc-eval-trace] phase=catch_match class={class_name:?} dynamic={dynamic:?} native={native:?}",
            );
        }
        if dynamic == Some(true) || native == Some(true) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Returns whether one name is a PHP native enum interface.
pub(in crate::interpreter) fn eval_builtin_enum_interface_name(name: &str) -> bool {
    let name = name.trim_start_matches('\\');
    name.eq_ignore_ascii_case("UnitEnum") || name.eq_ignore_ascii_case("BackedEnum")
}

/// Returns whether one name is PHP's native backed-enum interface.
pub(super) fn eval_builtin_backed_enum_interface_name(name: &str) -> bool {
    name.trim_start_matches('\\')
        .eq_ignore_ascii_case("BackedEnum")
}

/// Returns whether one name is PHP's native Throwable interface.
pub(super) fn eval_builtin_throwable_interface_name(name: &str) -> bool {
    name.trim_start_matches('\\')
        .eq_ignore_ascii_case("Throwable")
}

/// Returns whether one name is visible as a native/runtime interface to eval.
pub(in crate::interpreter) fn eval_runtime_interface_exists(
    name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(eval_builtin_enum_interface_name(name) || values.interface_exists(name)?)
}
