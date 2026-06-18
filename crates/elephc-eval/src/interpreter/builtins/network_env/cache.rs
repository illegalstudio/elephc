//! Purpose:
//! Implements temp-dir and intentionally empty realpath-cache eval builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` re-exports.
//!
//! Key details:
//! - The eval runtime does not maintain a PHP realpath cache, so cache probes
//!   return stable empty/zero values.

use super::super::super::*;

/// Evaluates PHP `sys_get_temp_dir()` with no arguments.
pub(in crate::interpreter) fn eval_builtin_sys_get_temp_dir(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_sys_get_temp_dir_result(values)
}

/// Returns the same temporary directory literal as the native static builtin.
pub(in crate::interpreter) fn eval_sys_get_temp_dir_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.string("/tmp")
}

/// Evaluates PHP `realpath_cache_get()` with no arguments.
pub(in crate::interpreter) fn eval_builtin_realpath_cache_get(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_realpath_cache_get_result(values)
}

/// Returns elephc's intentionally empty realpath-cache view.
pub(in crate::interpreter) fn eval_realpath_cache_get_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.array_new(0)
}

/// Evaluates PHP `realpath_cache_size()` with no arguments.
pub(in crate::interpreter) fn eval_builtin_realpath_cache_size(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_realpath_cache_size_result(values)
}

/// Returns zero because elephc does not maintain a runtime realpath cache.
pub(in crate::interpreter) fn eval_realpath_cache_size_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.int(0)
}
