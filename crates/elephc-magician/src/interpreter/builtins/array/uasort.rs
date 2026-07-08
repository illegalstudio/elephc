//! Purpose:
//! Declarative eval registry entry for `uasort`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Direct calls stay on the source-sensitive by-reference path.

use super::super::super::*;

eval_builtin! {
    name: "uasort",
    area: Array,
    params: [array: by_ref, callback],
    by_ref: [array],
    direct: none,
    values: ArrayMutating,
}
/// Dispatches by-value callable eval calls for the `uasort` array mutator.
pub(in crate::interpreter) fn eval_uasort_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array, callback] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    super::array_pop::eval_warn_array_by_value("uasort", values)?;
    eval_user_sort_value_result("uasort", *array, *callback, context, values)
}
