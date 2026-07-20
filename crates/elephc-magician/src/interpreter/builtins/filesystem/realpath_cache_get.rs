//! Purpose:
//! Declarative eval registry entry and implementation for `realpath_cache_get`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Eval does not maintain a PHP realpath cache, so this returns a stable empty array.

eval_builtin! {
    name: "realpath_cache_get",
    area: Filesystem,
    params: [],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates `realpath_cache_get()` with no arguments.
pub(in crate::interpreter) fn eval_realpath_cache_get_declared_call(
    args: &[EvalExpr],
    _context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_realpath_cache_get(args, values)
}

/// Evaluates `realpath_cache_get()` from already evaluated arguments.
pub(in crate::interpreter) fn eval_realpath_cache_get_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_realpath_cache_get_result(values)
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
