//! Purpose:
//! Declarative eval registry entry for `array_walk`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Direct calls stay on the source-sensitive by-reference path.

use super::super::super::*;

eval_builtin! {
    name: "array_walk",
    area: Array,
    params: [array: by_ref, callback],
    by_ref: [array],
    direct: none,
    values: ArrayMutating,
}
/// Dispatches by-value callable eval calls for the `array_walk` array mutator.
pub(in crate::interpreter) fn eval_array_walk_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array, callback] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    values.warning("array_walk(): Argument #1 ($array) must be passed by reference, value given")?;
    eval_array_walk_result(*array, *callback, context, values)
}
