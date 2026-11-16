//! Purpose:
//! Declarative eval registry entry for `array_rand`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the array-rand hook.

use super::super::super::*;

eval_builtin! {
    contract: "array_rand",
    area: Array,
    direct: ArrayRand,
    values: ArrayRand,
}
/// Dispatches direct eval calls for the `array_rand` array builtin.
pub(in crate::interpreter) fn eval_array_rand_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_rand(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_rand` array builtin.
pub(in crate::interpreter) fn eval_array_rand_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [array] => eval_array_rand_result(*array, None, values),
        [array, num] => {
            let num = eval_int_value(*num, values)?;
            eval_array_rand_result(*array, Some(num), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `array_rand()` over one eval array expression.
pub(in crate::interpreter) fn eval_builtin_array_rand(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [array] => {
            let array = eval_expr(array, context, scope, values)?;
            eval_array_rand_result(array, None, values)
        }
        [array, num] => {
            let array = eval_expr(array, context, scope, values)?;
            let num = eval_expr(num, context, scope, values)?;
            let num = eval_int_value(num, values)?;
            eval_array_rand_result(array, Some(num), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Returns a valid random key from a non-empty eval array.
pub(in crate::interpreter) fn eval_array_rand_result(
    array: RuntimeCellHandle,
    num: Option<i64>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    if len == 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    match num {
        None | Some(1) => {
            let position = eval_random_position(len);
            values.array_iter_key(array, position)
        }
        Some(num) if num > 0 && num <= len as i64 => {
            let negative = num > (len as i64 >> 1);
            let sample = if negative { len as i64 - num } else { num } as usize;
            let mut selected = vec![false; len];
            let mut remaining = sample;
            let mut failures = 0usize;
            while remaining > 0 {
                let position = eval_random_position(len);
                if selected[position] {
                    failures += 1;
                    if failures > 50 {
                        return Err(EvalStatus::RuntimeFatal);
                    }
                } else {
                    selected[position] = true;
                    remaining -= 1;
                    failures = 0;
                }
            }
            let mut result = values.array_new(num as usize)?;
            let mut output = 0i64;
            for position in 0..len {
                if selected[position] != negative {
                    let value = values.array_iter_key(array, position)?;
                    let key = values.int(output)?;
                    result = values.array_set(result, key, value)?;
                    output += 1;
                }
            }
            Ok(result)
        }
        Some(_) => Err(EvalStatus::RuntimeFatal),
    }
}

/// Chooses a pseudo-random array position within `[0, len)`.
pub(in crate::interpreter) fn eval_random_position(len: usize) -> usize {
    (eval_random_u128() % (len as u128)) as usize
}
