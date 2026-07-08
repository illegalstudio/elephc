//! Purpose:
//! Declarative eval registry entry for `spl_classes`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the SPL classes helper.

eval_builtin! {
    name: "spl_classes",
    area: Symbols,
    params: [],
    direct: Symbols,
    values: Symbols,
}

use super::super::eval_static_string_array_result;
use super::super::super::*;

/// Dispatches direct eval calls for the `spl_classes` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_spl_classes_declared_call(
    args: &[EvalExpr],
    _context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::spl_classes::eval_builtin_spl_classes(args, values)
}

/// Dispatches evaluated-argument calls for the `spl_classes` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_spl_classes_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args.is_empty() { super::spl_classes::eval_spl_classes_result(values) } else { Err(EvalStatus::RuntimeFatal) }
}

/// Evaluates PHP `spl_classes()` with no arguments.
pub(in crate::interpreter) fn eval_builtin_spl_classes(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_spl_classes_result(values)
}

/// Builds the static class-name list returned by eval `spl_classes()`.
pub(in crate::interpreter) fn eval_spl_classes_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_static_string_array_result(EVAL_SPL_CLASS_NAMES, values)
}
