//! Purpose:
//! String padding builtin.
//!
//! Called from:
//! - `crate::interpreter::builtins::strings` re-exports.
//!
//! Key details:
//! - Runtime cells remain opaque and string bytes are obtained through `RuntimeValueOps`.

use super::super::super::*;
use super::super::*;

/// Evaluates PHP `str_pad(...)` over a string, target length, pad string, and pad mode.
pub(in crate::interpreter) fn eval_builtin_str_pad(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value, length] => {
            let value = eval_expr(value, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            eval_str_pad_result(value, length, None, None, values)
        }
        [value, length, pad_string] => {
            let value = eval_expr(value, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            let pad_string = eval_expr(pad_string, context, scope, values)?;
            eval_str_pad_result(value, length, Some(pad_string), None, values)
        }
        [value, length, pad_string, pad_type] => {
            let value = eval_expr(value, context, scope, values)?;
            let length = eval_expr(length, context, scope, values)?;
            let pad_string = eval_expr(pad_string, context, scope, values)?;
            let pad_type = eval_expr(pad_type, context, scope, values)?;
            eval_str_pad_result(value, length, Some(pad_string), Some(pad_type), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Pads one byte string to a PHP target length using cyclic pad bytes.
pub(in crate::interpreter) fn eval_str_pad_result(
    value: RuntimeCellHandle,
    length: RuntimeCellHandle,
    pad_string: Option<RuntimeCellHandle>,
    pad_type: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let target_length = eval_int_value(length, values)?;
    let Ok(target_length) = usize::try_from(target_length) else {
        return values.string_bytes_value(&bytes);
    };
    if target_length <= bytes.len() {
        return values.string_bytes_value(&bytes);
    }

    let pad_string = match pad_string {
        Some(pad_string) => values.string_bytes(pad_string)?,
        None => b" ".to_vec(),
    };
    if pad_string.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let pad_type = match pad_type {
        Some(pad_type) => eval_int_value(pad_type, values)?,
        None => 1,
    };
    let (left_pad, right_pad) = eval_str_pad_sides(target_length - bytes.len(), pad_type)?;
    let capacity = bytes
        .len()
        .checked_add(left_pad)
        .and_then(|size| size.checked_add(right_pad))
        .ok_or(EvalStatus::RuntimeFatal)?;
    let mut output = Vec::with_capacity(capacity);
    eval_append_repeated_pad(&mut output, &pad_string, left_pad);
    output.extend_from_slice(&bytes);
    eval_append_repeated_pad(&mut output, &pad_string, right_pad);
    values.string_bytes_value(&output)
}

/// Splits a `str_pad()` pad budget into left and right byte counts.
pub(in crate::interpreter) fn eval_str_pad_sides(
    pad_budget: usize,
    pad_type: i64,
) -> Result<(usize, usize), EvalStatus> {
    match pad_type {
        0 => Ok((pad_budget, 0)),
        1 => Ok((0, pad_budget)),
        2 => Ok((pad_budget / 2, pad_budget - (pad_budget / 2))),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Appends `count` bytes by cycling through the provided non-empty pad string.
pub(in crate::interpreter) fn eval_append_repeated_pad(
    output: &mut Vec<u8>,
    pad_string: &[u8],
    count: usize,
) {
    for index in 0..count {
        output.push(pad_string[index % pad_string.len()]);
    }
}
