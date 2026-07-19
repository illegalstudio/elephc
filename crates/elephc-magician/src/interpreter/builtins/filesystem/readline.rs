//! Purpose:
//! Declarative eval registry entry for `readline`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Reads from host stdin and returns false at EOF.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "readline",
    area: Filesystem,
    params: [prompt = EvalBuiltinDefaultValue::Null],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates `readline([prompt])`.
pub(in crate::interpreter) fn eval_readline_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_readline(args, context, scope, values)
}

/// Evaluates `readline()` from already evaluated arguments.
pub(in crate::interpreter) fn eval_readline_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let prompt = match evaluated_args {
        [] => None,
        [prompt] => Some(*prompt),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_readline_result(prompt, values)
}

use std::io;

/// Evaluates `readline([prompt])` from unevaluated eval expressions.
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
