//! Purpose:
//! Eval registry entry and implementation wrapper for `shell_exec`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - Command execution delegates to the shell runner owned by `exec`.

use super::*;

eval_builtin! {
    name: "shell_exec",
    area: NetworkEnv,
    params: [command],
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates `shell_exec($command)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_shell_exec(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_process_command("shell_exec", args, context, scope, values)
}

/// Evaluates already materialized `shell_exec()` command arguments.
pub(in crate::interpreter) fn eval_shell_exec_result(
    command: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_process_command_result("shell_exec", command, values)
}
