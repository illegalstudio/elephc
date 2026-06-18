//! Purpose:
//! Implements `array_keys()` and `array_values()` eval projections.
//!
//! Called from:
//! - `crate::interpreter::builtins::arrays::filters` re-exports.
//!
//! Key details:
//! - Projection output is always a sequential indexed runtime array.

use super::super::super::super::*;

/// Evaluates PHP array projection builtins over one eval array expression.
pub(in crate::interpreter) fn eval_builtin_array_projection(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    eval_array_projection_result(name, array, values)
}

/// Builds the indexed result array for `array_keys()` or `array_values()`.
pub(in crate::interpreter) fn eval_array_projection_result(
    name: &str,
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = values.array_new(len)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = match name {
            "array_keys" => key,
            "array_values" => values.array_get(array, key)?,
            _ => return Err(EvalStatus::UnsupportedConstruct),
        };
        let index = values.int(position as i64)?;
        result = values.array_set(result, index, value)?;
    }
    Ok(result)
}
