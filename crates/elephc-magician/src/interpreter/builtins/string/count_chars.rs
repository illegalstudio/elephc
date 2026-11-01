//! Purpose:
//! Declarative eval registry entry and implementation for `count_chars`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Mirrors php-src exactly: modes 0, 1, and 2 return byte-value keyed tallies (all bytes,
//!   used bytes, unused bytes) and modes 3 and 4 return the used / unused byte values as a
//!   string, always in ascending byte order.
//! - A mode outside `0..=4` is php-src's catchable `ValueError`, raised through eval's
//!   pending-throw state so `catch (ValueError $e)` behaves as it does under the compiler.

eval_builtin! {
    contract: "count_chars",
    area: String,
    direct: CountChars,
    values: CountChars,
}

use super::super::super::*;

/// Evaluates PHP `count_chars(...)` over one subject and its optional mode.
pub(in crate::interpreter) fn eval_builtin_count_chars(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [subject] => {
            let subject = eval_expr(subject, context, scope, values)?;
            eval_count_chars_result(subject, None, context, values)
        }
        [subject, mode] => {
            let subject = eval_expr(subject, context, scope, values)?;
            let mode = eval_expr(mode, context, scope, values)?;
            eval_count_chars_result(subject, Some(mode), context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Tallies an already evaluated subject and materializes the requested `$mode` result.
pub(in crate::interpreter) fn eval_count_chars_result(
    subject: RuntimeCellHandle,
    mode: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(subject)?;
    let mode = match mode {
        Some(mode) => eval_int_value(mode, values)?,
        None => 0,
    };
    if !(0..=4).contains(&mode) {
        return eval_count_chars_mode_error(context, values);
    }
    let mut tally = [0i64; 256];
    for byte in bytes {
        tally[byte as usize] += 1;
    }

    if mode >= 3 {
        let wanted_used = mode == 3;
        let rendered = (0u32..256)
            .filter(|index| (tally[*index as usize] != 0) == wanted_used)
            .map(|index| index as u8)
            .collect::<Vec<u8>>();
        return values.string_bytes_value(&rendered);
    }

    // Modes 1 and 2 emit a sparse subset of the byte values, so the tally is built as an
    // associative array: an indexed array would pad every skipped byte value with an empty
    // element. Mode 0 uses the same shape so all three tally modes read identically.
    let mut result = values.assoc_new(256)?;
    for index in 0..256usize {
        let count = tally[index];
        let keep = match mode {
            0 => true,
            1 => count != 0,
            _ => count == 0,
        };
        if !keep {
            continue;
        }
        let key = values.int(index as i64)?;
        let value = values.int(count)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Raises PHP's catchable `ValueError` for a `$mode` outside `0..=4`.
fn eval_count_chars_mode_error<T>(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_builtin_exception_with_code("ValueError", "count_chars(): Argument #2 ($mode) must be between 0 and 4 (inclusive)", 0, context, values)
}
