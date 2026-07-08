//! Purpose:
//! Declarative eval registry entry for `range`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the integer range hook.

use super::super::super::*;

eval_builtin! {
    name: "range",
    area: Array,
    params: [start, end],
    direct: Range,
    values: Range,
}
/// Dispatches direct eval calls for the `range` array builtin.
pub(in crate::interpreter) fn eval_range_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_range(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `range` array builtin.
pub(in crate::interpreter) fn eval_range_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [start, end] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    eval_range_result(*start, *end, values)
}
