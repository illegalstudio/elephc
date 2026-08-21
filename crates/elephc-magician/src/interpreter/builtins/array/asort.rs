//! Purpose:
//! Declarative eval registry entry for `asort`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Direct calls stay on the source-sensitive by-reference path.

use super::super::super::*;

eval_builtin! {
    contract: "asort",
    area: Array,
    direct: none,
    values: ArrayMutating,
}
/// Dispatches by-value callable eval calls for the `asort` array mutator.
pub(in crate::interpreter) fn eval_asort_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    super::array_pop::eval_warn_array_by_value("asort", values)?;
    super::sort::eval_array_sort_value_result("asort", *array, context, values)
}
