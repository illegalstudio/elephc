//! Purpose:
//! Eval registry entry and implementation for `mt_rand`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Eval mirrors the existing `rand()` range behavior for `mt_rand()`.

use super::super::super::*;

eval_builtin! {
    name: "mt_rand",
    area: Math,
    params: [min, max],
    direct: MtRand,
    values: MtRand,
}

/// Evaluates PHP `mt_rand()` over zero args or an inclusive range.
pub(in crate::interpreter) fn eval_builtin_mt_rand(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_rand(args, context, scope, values)
}

/// Dispatches by-value `mt_rand()` calls after argument binding.
pub(in crate::interpreter) fn eval_mt_rand_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_rand_values_result(evaluated_args, values)
}
