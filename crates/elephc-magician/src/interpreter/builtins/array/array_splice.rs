//! Purpose:
//! Declarative eval registry entry for `array_splice`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Direct calls stay on the source-sensitive by-reference path.

use super::super::spec::EvalBuiltinDefaultValue;

use super::super::super::*;

eval_builtin! {
    name: "array_splice",
    area: Array,
    params: [
        array: by_ref,
        offset,
        length = EvalBuiltinDefaultValue::Null,
        replacement = EvalBuiltinDefaultValue::EmptyArray,
    ],
    by_ref: [array],
    direct: none,
    values: ArrayMutating,
}
/// Dispatches by-value callable eval calls for the `array_splice` array mutator.
pub(in crate::interpreter) fn eval_array_splice_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let result = match evaluated_args {
        [array, offset] => eval_array_splice_value_result(*array, *offset, None, values)?,
        [array, offset, length] => eval_array_splice_value_result(*array, *offset, Some(*length), values)?,
        [array, offset, length, _replacement] => eval_array_splice_value_result(*array, *offset, Some(*length), values)?,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.warning("array_splice(): Argument #1 ($array) must be passed by reference, value given")?;
    Ok(result)
}
