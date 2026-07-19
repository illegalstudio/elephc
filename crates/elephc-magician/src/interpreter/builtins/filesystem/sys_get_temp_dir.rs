//! Purpose:
//! Declarative eval registry entry and implementation for `sys_get_temp_dir`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Returns the same temporary directory literal as the native static builtin.

eval_builtin! {
    name: "sys_get_temp_dir",
    area: Filesystem,
    params: [],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates `sys_get_temp_dir()` with no arguments.
pub(in crate::interpreter) fn eval_sys_get_temp_dir_declared_call(
    args: &[EvalExpr],
    _context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_sys_get_temp_dir(args, values)
}

/// Evaluates `sys_get_temp_dir()` from already evaluated arguments.
pub(in crate::interpreter) fn eval_sys_get_temp_dir_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_sys_get_temp_dir_result(values)
}

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
