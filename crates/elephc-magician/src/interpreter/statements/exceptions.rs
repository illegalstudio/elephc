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
    let control = match execute_statements(body, context, scope, values) {
        Ok(EvalControl::Throw(thrown)) => {
            execute_matching_catch(thrown, catches, context, scope, values)?
        }
        Err(EvalStatus::UncaughtThrowable) => {
            let Some(thrown) = context.take_pending_throw() else {
                return Err(EvalStatus::UncaughtThrowable);
            };
            execute_matching_catch(thrown, catches, context, scope, values)?
        }
        Ok(control) => control,
        Err(status) => return Err(status),
    };
    if finally_body.is_empty() {
        return Ok(control);
    }
    match execute_statements(finally_body, context, scope, values) {
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

/// Releases a pending control-flow value when `finally` replaces that action.
pub(in crate::interpreter) fn release_overridden_control(
    control: EvalControl,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    match control {
        EvalControl::Return(value) | EvalControl::Throw(value) => values.release(value),
        EvalControl::None
        | EvalControl::ReturnVoid
        | EvalControl::Break
        | EvalControl::Continue => Ok(()),
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
        for replaced in set_scope_cell(
            context,
            scope,
            var_name.clone(),
            thrown,
            ScopeCellOwnership::Owned,
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
        if class_name.eq_ignore_ascii_case("Throwable") {
            return Ok(true);
        }
        if let Some(matched) = dynamic_object_is_a(thrown, class_name, false, context, values)? {
            if matched {
                return Ok(true);
            }
            continue;
        }
        if values.object_is_a(thrown, class_name, false)? {
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
