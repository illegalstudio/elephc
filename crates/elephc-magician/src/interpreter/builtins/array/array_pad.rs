//! Purpose:
//! Declarative eval registry entry for `array_pad`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the array-pad hook.
//! - `$length` is bounded the way php-src bounds it, before any allocation: an unrepresentable
//!   magnitude (`PHP_INT_MIN`) or one past the maximum allowed array size is refused instead of
//!   driving the interpreter through a multi-billion-element build loop.

use super::super::super::*;
use crate::interpreter::builtins::collection_builder::EvalArrayBuilder;

/// The largest `array_pad()` `$length` magnitude reference PHP will build an array for.
///
/// php-src rejects anything past this bound with
/// `ValueError: array_pad(): Argument #2 ($length) must not exceed the maximum allowed array size`
/// before it looks at the input array, so the bound is a plain constant. The AOT path raises that
/// catchable `ValueError`; eval refuses the call the same way it refuses a negative
/// `array_fill()` count.
const ARRAY_PAD_MAX_LENGTH: i64 = 1_073_741_824;

eval_builtin! {
    contract: "array_pad",
    area: Array,
    direct: ArrayPad,
    values: ArrayPad,
}
/// Dispatches direct eval calls for the `array_pad` array builtin.
pub(in crate::interpreter) fn eval_array_pad_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_pad(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_pad` array builtin.
pub(in crate::interpreter) fn eval_array_pad_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array, length, value] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    eval_array_pad_result(*array, *length, *value, values)
}

/// Evaluates PHP `array_pad()` over array, target length, and pad value expressions.
pub(in crate::interpreter) fn eval_builtin_array_pad(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array, length, value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    let length = eval_expr(length, context, scope, values)?;
    let value = eval_expr(value, context, scope, values)?;
    eval_array_pad_result(array, length, value, values)
}

/// Builds an `array_pad()` result by copying values and padding left or right.
///
/// A `$length` whose magnitude is unrepresentable or past `ARRAY_PAD_MAX_LENGTH` is refused before
/// any array is allocated, so the build loop below always runs a bounded number of times.
pub(in crate::interpreter) fn eval_array_pad_result(
    array: RuntimeCellHandle,
    length: RuntimeCellHandle,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let target = eval_int_value(length, values)?;
    let target_len = target
        .checked_abs()
        .filter(|magnitude| *magnitude <= ARRAY_PAD_MAX_LENGTH)
        .ok_or(EvalStatus::RuntimeFatal)
        .and_then(|value| usize::try_from(value).map_err(|_| EvalStatus::RuntimeFatal))?;
    let result_len = usize::max(len, target_len);
    let pad_count = result_len.saturating_sub(len);
    let mut result = EvalArrayBuilder::indexed(values, result_len)?;
    let left = if target < 0 { pad_count } else { 0 };
    for index in 0..result_len {
        match index.checked_sub(left) {
            Some(position) if position < len => result.update(|values, owners, _| {
                let key = owners.stage(values.array_iter_key(array, position)?);
                let element = owners.stage(values.array_get(array, key)?);
                let key = owners.stage(values.int(i64::try_from(index)
                    .map_err(|_| EvalStatus::RuntimeFatal)?)?);
                Ok(Some((key, element)))
            })?,
            _ => result.index(index, |_| Ok(value.borrowed()))?,
        }
    }
    Ok(result.finish())
}
