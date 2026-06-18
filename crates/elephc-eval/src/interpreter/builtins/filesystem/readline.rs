//! Purpose:
//! Implements eval's PHP `readline()` builtin against host stdin.
//!
//! Called from:
//! - `crate::interpreter::expressions::eval_positional_expr_call()`.
//! - Dynamic callable dispatch under `builtins::registry::dispatch`.
//!
//! Key details:
//! - EOF returns PHP `false`, matching the runtime `__rt_fgets` helper.
//! - Returned lines exclude a trailing LF or CRLF terminator.

use std::io;

use super::super::super::*;

/// Evaluates `readline([prompt])`.
pub(in crate::interpreter) fn eval_builtin_readline(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let prompt = match args {
        [] => None,
        [prompt] => Some(eval_expr(prompt, context, scope, values)?),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_readline_result(prompt, values)
}

/// Reads one line from host stdin after optionally echoing a prompt.
pub(in crate::interpreter) fn eval_readline_result(
    prompt: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(prompt) = prompt {
        values.echo(prompt)?;
    }
    let mut line = String::new();
    let read = io::stdin()
        .read_line(&mut line)
        .map_err(|_| EvalStatus::RuntimeFatal)?;
    if read == 0 {
        return values.bool_value(false);
    }
    if line.ends_with('\n') {
        line.pop();
        if line.ends_with('\r') {
            line.pop();
        }
    }
    values.string(&line)
}
