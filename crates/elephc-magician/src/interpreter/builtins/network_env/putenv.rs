//! Purpose:
//! Eval registry entry and implementation for `putenv`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - Assignments mutate the host process environment for the current eval process.

use super::*;

eval_builtin! {
    name: "putenv",
    area: NetworkEnv,
    params: [assignment],
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates PHP `putenv($assignment)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_putenv(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [assignment] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let assignment = eval_expr(assignment, context, scope, values)?;
    eval_putenv_result(assignment, values)
}

/// Applies one `putenv()` assignment to the host environment.
pub(in crate::interpreter) fn eval_putenv_result(
    assignment: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let assignment = values.string_bytes(assignment)?;
    if let Some(separator) = assignment.iter().position(|byte| *byte == b'=') {
        let name = String::from_utf8_lossy(&assignment[..separator]);
        let value = String::from_utf8_lossy(&assignment[separator + 1..]);
        std::env::set_var(name.as_ref(), value.as_ref());
    } else {
        let name = String::from_utf8_lossy(&assignment);
        std::env::remove_var(name.as_ref());
    }
    values.bool_value(true)
}
