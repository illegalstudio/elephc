//! Purpose:
//! Declarative eval registry entry and implementation for `proc_nice`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem` direct and by-value dispatch.
//!
//! Key details:
//! - Adjusts the scheduling priority of the whole process through `nice(3)` in the shared
//!   `elephc_proc` crate; failure (an unprivileged increase of priority) warns and answers false.

eval_builtin! {
    contract: "proc_nice",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for `proc_nice` through the area dispatcher.
pub(in crate::interpreter) fn eval_proc_nice_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [priority] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let priority = eval_expr(priority, context, scope, values)?;
    eval_proc_nice_result(priority, values)
}

/// Dispatches evaluated-argument calls for `proc_nice` through the area dispatcher.
pub(in crate::interpreter) fn eval_proc_nice_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [priority] => eval_proc_nice_result(*priority, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Changes the process priority by the requested increment.
pub(in crate::interpreter) fn eval_proc_nice_result(
    priority: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let increment = eval_int_value(priority, values)?;
    if elephc_proc::elephc_proc_nice(increment) == -1000 {
        values.warning(
            "proc_nice(): Only a super user may attempt to increase the priority of a process",
        )?;
        return values.bool_value(false);
    }
    values.bool_value(true)
}
