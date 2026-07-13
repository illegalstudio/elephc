//! Purpose:
//! Eval registry entry and implementation wrapper for `system`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - Command execution delegates to the shell runner owned by `exec`.

use super::*;

eval_builtin! {
    name: "system",
    area: NetworkEnv,
    params: [command],
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates `system($command)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_system(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_process_command("system", args, context, scope, values)
}

/// Evaluates already materialized `system()` command arguments.
pub(in crate::interpreter) fn eval_system_result(
    command: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_process_command_result("system", command, values)
}
