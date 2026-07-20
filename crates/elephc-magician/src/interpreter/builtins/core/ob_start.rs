//! Purpose:
//! Eval registry entry and implementation for `ob_start`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - Shares the runtime output-buffer stack with statically compiled code via the
//! -   `RuntimeValueOps` ob hooks, so eval'd and static output interleave correctly.
//! - User output handlers are unsupported: a non-null `$callback` raises a warning
//! -   and returns false without starting a buffer; `chunk_size`/`flags` are inert.

use super::super::super::*;

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "ob_start",
    area: Core,
    params: [
        callback = EvalBuiltinDefaultValue::Null,
        chunk_size = EvalBuiltinDefaultValue::Int(0),
        flags = EvalBuiltinDefaultValue::Int(112)
    ],
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
    let mut handler_id = None;
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
            let retained = values.retain(callback)?;
            let Some(id) =
                crate::ffi::ob_handlers::register_ob_handler(context as *mut _, retained)
            else {
                return Err(EvalStatus::RuntimeFatal);
            };
            handler_id = Some(id);
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
    let started = values.ob_start_ex(handler_id, &name, chunk_size, flags)?;
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
