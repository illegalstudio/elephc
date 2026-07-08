//! Purpose:
//! Declarative eval registry entry for `ksort`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Direct calls stay on the source-sensitive by-reference path.

use super::super::super::*;

eval_builtin! {
    name: "ksort",
    area: Array,
    params: [array: by_ref],
    by_ref: [array],
    direct: none,
    values: ArrayMutating,
}
/// Dispatches by-value callable eval calls for the `ksort` array mutator.
pub(in crate::interpreter) fn eval_ksort_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    super::array_pop::eval_warn_array_by_value("ksort", values)?;
    eval_array_sort_value_result(*array, values)
}
