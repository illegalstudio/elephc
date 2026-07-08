//! Purpose:
//! Declarative eval registry entry for `array_pop`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Direct calls stay on the source-sensitive by-reference path.

use super::super::super::*;

eval_builtin! {
    name: "array_pop",
    area: Array,
    params: [array: by_ref],
    by_ref: [array],
    direct: none,
    values: ArrayMutating,
}
/// Dispatches by-value callable eval calls for the `array_pop` array mutator.
pub(in crate::interpreter) fn eval_array_pop_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    eval_warn_array_by_value("array_pop", values)?;
    eval_array_pop_shift_value_result("array_pop", *array, values)
}

/// Emits the standard by-value warning for array mutator callable calls.
pub(in crate::interpreter) fn eval_warn_array_by_value(
    name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    values.warning(&format!(
        "{name}(): Argument #1 ($array) must be passed by reference, value given"
    ))
}
