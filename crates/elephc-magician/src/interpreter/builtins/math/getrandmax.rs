//! Purpose:
//! Eval registry entry and implementation for `getrandmax`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - An alias of `mt_getrandmax()`: always 2147483647.

use super::super::super::*;

eval_builtin! {
    contract: "getrandmax",
    area: Math,
    direct: Getrandmax,
    values: Getrandmax,
}

/// Evaluates PHP `getrandmax()`, which takes no arguments.
pub(in crate::interpreter) fn eval_builtin_getrandmax(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_getrandmax_values_result(values)
}

/// Returns php's `PHP_MT_RAND_MAX`.
pub(in crate::interpreter) fn eval_getrandmax_values_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.int(2_147_483_647)
}
