//! Purpose:
//! Eval registry entry and implementation for `mt_getrandmax`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Always php's `PHP_MT_RAND_MAX`, 2147483647.

use super::super::super::*;

eval_builtin! {
    contract: "mt_getrandmax",
    area: Math,
    direct: MtGetrandmax,
    values: MtGetrandmax,
}

/// Evaluates PHP `mt_getrandmax()`, which takes no arguments.
pub(in crate::interpreter) fn eval_builtin_mt_getrandmax(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_mt_getrandmax_values_result(values)
}

/// Returns php's `PHP_MT_RAND_MAX`.
pub(in crate::interpreter) fn eval_mt_getrandmax_values_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.int(2_147_483_647)
}
