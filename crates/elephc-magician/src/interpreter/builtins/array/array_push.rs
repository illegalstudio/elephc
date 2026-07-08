//! Purpose:
//! Declarative eval registry entry for `array_push`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Direct calls stay on the source-sensitive by-reference path.

use super::super::super::*;

eval_builtin! {
    name: "array_push",
    area: Array,
    params: [array: by_ref],
    variadic: values,
    by_ref: [array],
    direct: none,
    values: ArrayMutating,
}
/// Dispatches by-value callable eval calls for the `array_push` array mutator.
pub(in crate::interpreter) fn eval_array_push_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((array, inserted)) = evaluated_args.split_first() else { return Err(EvalStatus::RuntimeFatal); };
    super::array_pop::eval_warn_array_by_value("array_push", values)?;
    eval_array_push_unshift_count_result(*array, inserted.len(), values)
}
