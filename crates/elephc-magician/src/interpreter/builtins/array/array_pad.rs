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
    super::array_arg_check::eval_check_array_args("array_pad", &[array], context, values)?;
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
    let mut result = values.array_new(result_len)?;
    let mut output_index = 0usize;

    if target < 0 {
        let (padded, next_index) =
            eval_array_pad_append_repeated(result, output_index, pad_count, value, values)?;
        result = padded;
        output_index = next_index;
    }

    for position in 0..len {
        let source_key = values.array_iter_key(array, position)?;
        let source_value = values.array_get(array, source_key)?;
        let target_key = i64::try_from(output_index).map_err(|_| EvalStatus::RuntimeFatal)?;
        let target_key = values.int(target_key)?;
        result = values.array_set(result, target_key, source_value)?;
        output_index += 1;
    }

    if target > 0 {
        result = eval_array_pad_append_repeated(result, output_index, pad_count, value, values)?.0;
    }

    Ok(result)
}

/// Appends the same pad value at consecutive indexed positions in an array result.
pub(in crate::interpreter) fn eval_array_pad_append_repeated(
    mut array: RuntimeCellHandle,
    start_index: usize,
    count: usize,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, usize), EvalStatus> {
    let mut next_index = start_index;
    for _ in 0..count {
        let key = i64::try_from(next_index).map_err(|_| EvalStatus::RuntimeFatal)?;
        let key = values.int(key)?;
        array = values.array_set(array, key, value)?;
        next_index += 1;
    }
    Ok((array, next_index))
}
