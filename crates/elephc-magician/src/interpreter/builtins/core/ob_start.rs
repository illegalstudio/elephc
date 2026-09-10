//! Purpose:
//! Eval registry entry and implementation for `ob_start`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - Shares the runtime output-buffer stack with statically compiled code through
//!   `RuntimeValueOps`, including callable handlers, chunk size, and operation flags.
//! - The buffer owns successful registrations; failed starts retire their copied callback immediately.

use super::super::super::*;

eval_builtin! {
    contract: "ob_start",
    area: Core,
    direct: Core,
    values: Core,
}

/// Evaluates PHP `ob_start($callback = null, $chunk_size = 0, $flags = 112)`.
pub(in crate::interpreter) fn eval_builtin_ob_start(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() > 3 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_ob_start_result(&evaluated, context, values)
}

/// Starts a runtime output buffer, registering user handler callables so the
/// runtime flush paths can invoke them through the magician hook.
pub(in crate::interpreter) fn eval_ob_start_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args.len() > 3 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut handler = None;
    let mut name = "default output handler".to_string();
    if let Some(callback) = evaluated_args.first().copied() {
        if !values.is_null(callback)? {
            // Reject plain scalars up front like PHP; the interpreter resolves
            // every other shape (string names, closures, arrays, invokables) at
            // invocation time.
            let tag = values.type_tag(callback)?;
            if matches!(tag, 0 | 2 | 3) {
                eval_ob_echo_line(values, "Warning: ob_start(): no array or string given\n")?;
                eval_ob_echo_line(values, "Notice: ob_start(): Failed to create buffer\n")?;
                return values.bool_value(false);
            }
            name = if tag == 1 {
                String::from_utf8_lossy(&values.string_bytes(callback)?).into_owned()
            } else {
                "Closure::__invoke".to_string()
            };
            handler = Some(callback);
        }
    }
    let chunk_size = match evaluated_args.get(1).copied() {
        Some(chunk) => eval_int_value(chunk, values)?,
        None => 0,
    };
    let flags = match evaluated_args.get(2).copied() {
        Some(flags) => eval_int_value(flags, values)?,
        None => 112,
    };
    let handler_id = match handler {
        Some(callback) => {
            let retained = values.retain(callback)?;
            match crate::ffi::ob_handlers::register_ob_handler(context as *mut _, retained) {
                Some(id) => Some(id),
                None => {
                    eval_release_value(context, values, retained)?;
                    return Err(EvalStatus::RuntimeFatal);
                },
            }
        },
        None => None,
    };
    let started = values.ob_start_ex(handler_id, &name, chunk_size, flags);
    if !matches!(started, Ok(true)) {
        if let Some(id) = handler_id {
            if let Some(owner) = crate::ffi::ob_handlers::unregister_ob_handler(id, context as *mut _) {
                eval_release_value(context, values, owner)?;
            }
        }
    }
    let started = started?;
    values.bool_value(started)
}

/// Emits one diagnostic line through the eval echo path (so active output
/// buffers capture it exactly like PHP with display_errors enabled).
fn eval_ob_echo_line(
    values: &mut impl RuntimeValueOps,
    line: &str,
) -> Result<(), EvalStatus> {
    let cell = values.string_bytes_value(line.as_bytes())?;
    values.echo(cell)?;
    values.release(cell)
}
