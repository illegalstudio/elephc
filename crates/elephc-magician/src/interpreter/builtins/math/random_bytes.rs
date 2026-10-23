//! Purpose:
//! Eval registry entry and implementation for `random_bytes`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - The bytes are read from the operating system's CSPRNG (`/dev/urandom`), the same
//!   source class the compiled helper uses; there is no pseudo-random fallback.
//! - A `$length` below 1 raises php-src's catchable `ValueError`; an unreadable generator
//!   throws an `Exception` with php-src's message (`Random\RandomException` is not declared).
//! - A string `$length` follows the weak-mode `int` parameter like the compiled lowering: a
//!   numeric string is converted, anything else raises the catchable `TypeError`.

use std::io::Read;

use super::super::super::*;

eval_builtin! {
    contract: "random_bytes",
    area: Math,
    direct: RandomBytes,
    values: RandomBytes,
}

/// php-src's `ValueError` wording for a `$length` below 1.
const RANDOM_BYTES_LENGTH_MESSAGE: &str =
    "random_bytes(): Argument #1 ($length) must be greater than 0";

/// php-src's `TypeError` wording for a `$length` string that is not numeric.
const RANDOM_BYTES_LENGTH_TYPE_MESSAGE: &str =
    "random_bytes(): Argument #1 ($length) must be of type int, string given";

/// php-src's message when the operating system cannot provide randomness.
const RANDOM_BYTES_FAILURE_MESSAGE: &str =
    "Failed to retrieve randomness from the operating system";

/// Evaluates PHP `random_bytes()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_random_bytes(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [length] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let length = eval_expr(length, context, scope, values)?;
    eval_random_bytes_values_result(&[length], context, values)
}

/// Dispatches by-value `random_bytes()` calls after argument binding.
pub(in crate::interpreter) fn eval_random_bytes_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [length] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let length = if values.type_tag(*length)? == EVAL_TAG_STRING {
        match eval_random_bytes_string_length(&values.string_bytes(*length)?) {
            Some(length) => length,
            None => {
                return eval_random_bytes_throw(
                    "TypeError", RANDOM_BYTES_LENGTH_TYPE_MESSAGE, context, values,
                );
            }
        }
    } else {
        eval_int_value(*length, values)?
    };
    if length < 1 {
        return eval_random_bytes_throw("ValueError", RANDOM_BYTES_LENGTH_MESSAGE, context, values);
    }
    let mut bytes = vec![0u8; usize::try_from(length).map_err(|_| EvalStatus::RuntimeFatal)?];
    let filled = std::fs::File::open("/dev/urandom").and_then(|mut source| source.read_exact(&mut bytes));
    if filled.is_err() {
        return eval_random_bytes_throw("Exception", RANDOM_BYTES_FAILURE_MESSAGE, context, values);
    }
    values.string_bytes_value(&bytes)
}

/// Converts a numeric `$length` string the way PHP's weak-mode `int` parameter does: PHP
/// whitespace may surround an integer or float spelling (a float truncates toward zero);
/// anything else, including a leading-numeric string such as `"3abc"`, is `None`.
fn eval_random_bytes_string_length(bytes: &[u8]) -> Option<i64> {
    let is_space = |byte: &u8| matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c);
    let start = bytes.iter().position(|byte| !is_space(byte))?;
    let end = bytes.iter().rposition(|byte| !is_space(byte))? + 1;
    let text = std::str::from_utf8(&bytes[start..end]).ok()?;
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    let (mantissa, exponent) = match unsigned.find(['e', 'E']) {
        Some(at) => (&unsigned[..at], Some(&unsigned[at + 1..])),
        None => (unsigned, None),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    if whole.is_empty() && fraction.is_empty() || !digits(whole) || !digits(fraction) {
        return None;
    }
    if let Some(exponent) = exponent {
        let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        if exponent.is_empty() || !digits(exponent) {
            return None;
        }
    }
    if exponent.is_none() && !mantissa.contains('.') {
        if let Ok(value) = text.parse::<i64>() {
            return Some(value);
        }
    }
    text.parse::<f64>().ok().map(|value| value as i64)
}

/// Raises one catchable throwable with a php-src message.
fn eval_random_bytes_throw<T>(
    class: &str,
    message: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    let exception = values.new_object(class)?;
    let message = values.string(message)?;
    let code = values.int(0)?;
    values.construct_object(exception, vec![message, code])?;
    context.set_pending_throw(exception);
    Err(EvalStatus::UncaughtThrowable)
}
