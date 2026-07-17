//! Purpose:
//! Eval registry entry and implementation wrapper for `passthru`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - Command execution delegates to the shell runner owned by `exec`.

use super::*;

eval_builtin! {
    name: "passthru",
    area: NetworkEnv,
    params: [command],
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates `passthru($command)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_passthru(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_process_command("passthru", args, context, scope, values)
}

/// Evaluates already materialized `passthru()` command arguments.
pub(in crate::interpreter) fn eval_passthru_result(
    command: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_process_command_result("passthru", command, values)
}
