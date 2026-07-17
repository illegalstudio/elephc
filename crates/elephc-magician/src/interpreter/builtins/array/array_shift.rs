//! Purpose:
//! Declarative eval registry entry for `array_shift`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Direct calls stay on the source-sensitive by-reference path.

use super::super::super::*;

eval_builtin! {
    name: "array_shift",
    area: Array,
    params: [array: by_ref],
    by_ref: [array],
    direct: none,
    values: ArrayMutating,
}
/// Dispatches by-value callable eval calls for the `array_shift` array mutator.
pub(in crate::interpreter) fn eval_array_shift_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    super::array_pop::eval_warn_array_by_value("array_shift", values)?;
    super::array_pop::eval_array_pop_shift_value_result("array_shift", *array, values)
}
