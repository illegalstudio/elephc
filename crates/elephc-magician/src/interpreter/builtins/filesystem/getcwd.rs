//! Purpose:
//! Declarative eval registry entry for `getcwd`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the current-working-directory helper.

eval_builtin! {
    name: "getcwd",
    area: Filesystem,
    params: [],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `getcwd` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_getcwd_declared_call(
    args: &[EvalExpr],
    _context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_getcwd(args, values)
}

/// Dispatches evaluated-argument calls for the `getcwd` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_getcwd_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [] => eval_getcwd_result(values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `getcwd()` with no arguments.
pub(in crate::interpreter) fn eval_builtin_getcwd(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_getcwd_result(values)
}

/// Returns the process current working directory as a boxed PHP string.
pub(in crate::interpreter) fn eval_getcwd_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let cwd = std::env::current_dir().map_err(|_| EvalStatus::RuntimeFatal)?;
    values.string(cwd.to_string_lossy().as_ref())
}
