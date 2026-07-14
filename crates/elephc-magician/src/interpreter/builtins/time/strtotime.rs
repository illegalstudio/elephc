//! Purpose:
//! Eval registry entry and implementation for `strtotime`.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` direct and by-value dispatch.
//!
//! Key details:
//! - The supported parser subset normalizes fixed-width ISO dates through `mktime`.

use super::super::*;
use super::*;

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "strtotime",
    area: Time,
    params: [datetime, baseTimestamp = EvalBuiltinDefaultValue::Null],
    direct: Time,
    values: Time,
}

/// Evaluates PHP `strtotime(datetime, baseTimestamp = null)` for eval's supported subset.
pub(in crate::interpreter) fn eval_builtin_strtotime(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [datetime] => {
            let datetime = eval_expr(datetime, context, scope, values)?;
            eval_strtotime_result(datetime, None, context, values)
        }
        [datetime, base_timestamp] => {
            let datetime = eval_expr(datetime, context, scope, values)?;
            let base_timestamp = eval_expr(base_timestamp, context, scope, values)?;
            eval_strtotime_result(datetime, Some(base_timestamp), context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Parses one eval `strtotime()` input and boxes the resulting timestamp.
pub(in crate::interpreter) fn eval_strtotime_result(
    datetime: RuntimeCellHandle,
    base_timestamp: Option<RuntimeCellHandle>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(datetime)?;
    let base_timestamp = match base_timestamp {
        Some(base_timestamp) if !values.is_null(base_timestamp)? => {
            Some(eval_int_value(base_timestamp, values)?)
        }
        _ => None,
    };
    let timestamp = eval_strtotime_bytes(&bytes, base_timestamp, context)?;
    values.int(timestamp)
}

/// Parses eval's supported `strtotime()` strings into local Unix timestamps.
pub(in crate::interpreter) fn eval_strtotime_bytes(
    bytes: &[u8],
    base_timestamp: Option<i64>,
    context: &ElephcEvalContext,
) -> Result<i64, EvalStatus> {
    let bytes = eval_trim_ascii_whitespace(bytes);
    if bytes.eq_ignore_ascii_case(b"now") {
        return match base_timestamp {
            Some(timestamp) => Ok(timestamp),
            None => eval_current_unix_timestamp(),
        };
    }
    let Some((year, month, day, hour, minute, second)) = eval_parse_iso_datetime(bytes) else {
        return Ok(-1);
    };
    eval_context_mktime_timestamp((hour, minute, second, month, day, year), context)
}

/// Trims ASCII whitespace from both ends of one byte slice.
pub(in crate::interpreter) fn eval_trim_ascii_whitespace(bytes: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = bytes.len();
    while start < end && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    &bytes[start..end]
}

/// Parses fixed-width ISO date and datetime forms supported by eval `strtotime()`.
pub(in crate::interpreter) fn eval_parse_iso_datetime(
    bytes: &[u8],
) -> Option<(
    libc::c_int,
    libc::c_int,
    libc::c_int,
    libc::c_int,
    libc::c_int,
    libc::c_int,
)> {
    if bytes.len() != 10 && bytes.len() != 16 && bytes.len() != 19 {
        return None;
    }
    if bytes.get(4) != Some(&b'-') || bytes.get(7) != Some(&b'-') {
        return None;
    }
    let year = eval_parse_fixed_digits(bytes, 0, 4)?;
    let month = eval_parse_fixed_digits(bytes, 5, 2)?;
    let day = eval_parse_fixed_digits(bytes, 8, 2)?;
    let (hour, minute, second) = if bytes.len() == 10 {
        (0, 0, 0)
    } else {
        if !matches!(bytes.get(10), Some(b' ') | Some(b'T') | Some(b't')) {
            return None;
        }
        if bytes.get(13) != Some(&b':') {
            return None;
        }
        let hour = eval_parse_fixed_digits(bytes, 11, 2)?;
        let minute = eval_parse_fixed_digits(bytes, 14, 2)?;
        let second = if bytes.len() == 19 {
            if bytes.get(16) != Some(&b':') {
                return None;
            }
            eval_parse_fixed_digits(bytes, 17, 2)?
        } else {
            0
        };
        (hour, minute, second)
    };
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=59).contains(&second)
    {
        return None;
    }
    Some((year, month, day, hour, minute, second))
}

/// Parses a fixed-width decimal field as a libc-compatible integer.
pub(in crate::interpreter) fn eval_parse_fixed_digits(
    bytes: &[u8],
    start: usize,
    len: usize,
) -> Option<libc::c_int> {
    let end = start.checked_add(len)?;
    let field = bytes.get(start..end)?;
    let mut value: libc::c_int = 0;
    for byte in field {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?;
        value = value.checked_add(libc::c_int::from(byte - b'0'))?;
    }
    Some(value)
}
