//! Purpose:
//! Implements PHP `zend_version()` for the active runtime eval compatibility profile.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - The result follows the same profile published to eval for PHP version constants.

use super::super::super::*;

eval_builtin! {
    contract: "zend_version",
    area: Core,
    direct: Core,
    values: Core,
}

/// Evaluates a direct zero-argument `zend_version()` call.
pub(in crate::interpreter) fn eval_builtin_zend_version(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_zend_version_result(values)
}

/// Evaluates `zend_version()` from an already materialized empty argument list.
pub(in crate::interpreter) fn eval_zend_version_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_zend_version_result(values)
}

/// Returns the Zend Engine version string corresponding to the active PHP profile.
fn eval_zend_version_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.string(&crate::eval_php_profile::eval_zend_version_string())
}
