//! Purpose:
//! Declarative eval registry entry and implementation for `realpath_cache_size`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Eval does not maintain a PHP realpath cache, so this returns zero.

eval_builtin! {
    name: "realpath_cache_size",
    area: Filesystem,
    params: [],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates `realpath_cache_size()` with no arguments.
pub(in crate::interpreter) fn eval_realpath_cache_size_declared_call(
    args: &[EvalExpr],
    _context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_realpath_cache_size(args, values)
}

/// Evaluates `realpath_cache_size()` from already evaluated arguments.
pub(in crate::interpreter) fn eval_realpath_cache_size_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_realpath_cache_size_result(values)
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
