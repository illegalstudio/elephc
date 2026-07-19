//! Purpose:
//! Eval registry entry and implementation for `pi`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - `pi()` accepts no arguments and returns the platform `f64` PI constant.

use super::super::super::*;

eval_builtin! {
    name: "pi",
    area: Math,
    params: [],
    direct: Pi,
    values: Pi,
}

/// Evaluates PHP `pi()` with no eval arguments.
pub(in crate::interpreter) fn eval_builtin_pi(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_pi_result(values)
}

/// Returns PHP `pi()` as an already evaluated builtin result.
pub(in crate::interpreter) fn eval_pi_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.float(std::f64::consts::PI)
}
