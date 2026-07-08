//! Purpose:
//! Eval registry entry and implementation for `date` plus shared date-format helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` direct and by-value dispatch.
//!
//! Key details:
//! - `gmdate` calls this file for shared formatting and UTC/local timestamp conversion.

use std::os::unix::ffi::OsStrExt;
use std::sync::Mutex;

use super::super::*;
use super::*;

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "date",
    area: Time,
    params: [format, timestamp = EvalBuiltinDefaultValue::Null],
    direct: Time,
    values: Time,
}

static EVAL_TZ_MUTEX: Mutex<()> = Mutex::new(());

unsafe extern "C" {
    /// Re-reads libc's process-global timezone environment.
    fn tzset();
}

/// Evaluates PHP `date($format, $timestamp = time())` for the eval subset.
pub(in crate::interpreter) fn eval_builtin_date(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_date_like("date", args, context, scope, values)
}

/// Evaluates PHP `date($format, $timestamp = time())` for the eval subset.
pub(in crate::interpreter) fn eval_builtin_date_like(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [format] => {
            let format = eval_expr(format, context, scope, values)?;
            eval_date_result(name, format, None, context, values)
        }
        [format, timestamp] => {
            let format = eval_expr(format, context, scope, values)?;
            let timestamp = eval_expr(timestamp, context, scope, values)?;
            eval_date_result(name, format, Some(timestamp), context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Formats one Unix timestamp through PHP `date()` token rules supported by elephc.
pub(in crate::interpreter) fn eval_date_result(
    name: &str,
    format: RuntimeCellHandle,
    timestamp: Option<RuntimeCellHandle>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let format = values.string_bytes(format)?;
    let timestamp = match timestamp {
        Some(timestamp) if !values.is_null(timestamp)? => eval_int_value(timestamp, values)?,
        None => eval_current_unix_timestamp()?,
        Some(_) => eval_current_unix_timestamp()?,
    };
    let tm = match name {
        "date" => eval_context_localtime(timestamp, context)?,
        "gmdate" => eval_gmtime(timestamp)?,
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    let output = eval_format_date_bytes(&format, &tm, timestamp)?;
    values.string_bytes_value(&output)
}

/// Converts one Unix timestamp to eval-timezone broken-down time through libc.
pub(in crate::interpreter) fn eval_context_localtime(
    timestamp: i64,
    context: &ElephcEvalContext,
) -> Result<libc::tm, EvalStatus> {
    eval_with_timezone(context.default_timezone(), || eval_localtime(timestamp))
}

/// Converts one Unix timestamp to process-local broken-down time through libc.
pub(in crate::interpreter) fn eval_localtime(timestamp: i64) -> Result<libc::tm, EvalStatus> {
    let raw: libc::time_t = timestamp.try_into().map_err(|_| EvalStatus::RuntimeFatal)?;
    let mut tm = MaybeUninit::<libc::tm>::uninit();
    let result = unsafe { libc::localtime_r(&raw, tm.as_mut_ptr()) };
    if result.is_null() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(unsafe { tm.assume_init() })
}

/// Converts one Unix timestamp to UTC broken-down time through libc.
pub(in crate::interpreter) fn eval_gmtime(timestamp: i64) -> Result<libc::tm, EvalStatus> {
    let raw: libc::time_t = timestamp.try_into().map_err(|_| EvalStatus::RuntimeFatal)?;
    let mut tm = MaybeUninit::<libc::tm>::uninit();
    let result = unsafe { libc::gmtime_r(&raw, tm.as_mut_ptr()) };
    if result.is_null() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(unsafe { tm.assume_init() })
}

/// Runs one libc timezone-sensitive operation under the eval context timezone.
pub(in crate::interpreter) fn eval_with_timezone<T>(
    timezone: &str,
    operation: impl FnOnce() -> Result<T, EvalStatus>,
) -> Result<T, EvalStatus> {
    let _guard = EVAL_TZ_MUTEX
        .lock()
        .map_err(|_| EvalStatus::RuntimeFatal)?;
    let previous = std::env::var_os("TZ")
        .map(|value| CString::new(value.as_bytes()).map_err(|_| EvalStatus::RuntimeFatal))
        .transpose()?;
    eval_apply_process_timezone(timezone)?;
    let result = operation();
    eval_restore_process_timezone(previous.as_ref())?;
    result
}

/// Applies one timezone identifier to libc's process-global timezone state.
fn eval_apply_process_timezone(timezone: &str) -> Result<(), EvalStatus> {
    let key = CString::new("TZ").map_err(|_| EvalStatus::RuntimeFatal)?;
    let value = CString::new(timezone).map_err(|_| EvalStatus::RuntimeFatal)?;
    let status = unsafe { libc::setenv(key.as_ptr(), value.as_ptr(), 1) };
    if status != 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    unsafe { tzset() };
    Ok(())
}

/// Restores the process timezone that was active before an eval-local conversion.
fn eval_restore_process_timezone(previous: Option<&CString>) -> Result<(), EvalStatus> {
    let key = CString::new("TZ").map_err(|_| EvalStatus::RuntimeFatal)?;
    let status = if let Some(value) = previous {
        unsafe { libc::setenv(key.as_ptr(), value.as_ptr(), 1) }
    } else {
        unsafe { libc::unsetenv(key.as_ptr()) }
    };
    if status != 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    unsafe { tzset() };
    Ok(())
}

/// Applies PHP `date()` tokens to one local broken-down timestamp.
pub(in crate::interpreter) fn eval_format_date_bytes(
    format: &[u8],
    tm: &libc::tm,
    timestamp: i64,
) -> Result<Vec<u8>, EvalStatus> {
    let mut output = Vec::new();
    let mut escaped = false;
    for byte in format {
        if escaped {
            output.push(*byte);
            escaped = false;
            continue;
        }
        if *byte == b'\\' {
            escaped = true;
            continue;
        }
        eval_push_date_token(&mut output, *byte, tm, timestamp)?;
    }
    if escaped {
        output.push(b'\\');
    }
    Ok(output)
}

/// Appends the expansion for one PHP `date()` token, or the token literal.
pub(in crate::interpreter) fn eval_push_date_token(
    output: &mut Vec<u8>,
    token: u8,
    tm: &libc::tm,
    timestamp: i64,
) -> Result<(), EvalStatus> {
    match token {
        b'Y' => eval_push_padded_number(output, i64::from(tm.tm_year) + 1900, 4),
        b'm' => eval_push_padded_number(output, i64::from(tm.tm_mon) + 1, 2),
        b'd' => eval_push_padded_number(output, i64::from(tm.tm_mday), 2),
        b'H' => eval_push_padded_number(output, i64::from(tm.tm_hour), 2),
        b'i' => eval_push_padded_number(output, i64::from(tm.tm_min), 2),
        b's' => eval_push_padded_number(output, i64::from(tm.tm_sec), 2),
        b'l' => output.extend_from_slice(EVAL_WEEKDAY_NAMES[eval_tm_weekday_index(tm)?].as_bytes()),
        b'F' => output.extend_from_slice(EVAL_MONTH_NAMES[eval_tm_month_index(tm)?].as_bytes()),
        b'D' => output
            .extend_from_slice(EVAL_WEEKDAY_SHORT_NAMES[eval_tm_weekday_index(tm)?].as_bytes()),
        b'M' => {
            output.extend_from_slice(EVAL_MONTH_SHORT_NAMES[eval_tm_month_index(tm)?].as_bytes())
        }
        b'N' => {
            let weekday = tm.tm_wday;
            let iso_weekday = if weekday == 0 { 7 } else { weekday };
            output.extend_from_slice(iso_weekday.to_string().as_bytes());
        }
        b'j' => output.extend_from_slice(tm.tm_mday.to_string().as_bytes()),
        b'n' => output.extend_from_slice((tm.tm_mon + 1).to_string().as_bytes()),
        b'G' => output.extend_from_slice(tm.tm_hour.to_string().as_bytes()),
        b'g' => {
            let hour = tm.tm_hour % 12;
            let hour = if hour == 0 { 12 } else { hour };
            output.extend_from_slice(hour.to_string().as_bytes());
        }
        b'A' => output.extend_from_slice(if tm.tm_hour < 12 { b"AM" } else { b"PM" }),
        b'a' => output.extend_from_slice(if tm.tm_hour < 12 { b"am" } else { b"pm" }),
        b'U' => output.extend_from_slice(timestamp.to_string().as_bytes()),
        _ => output.push(token),
    }
    Ok(())
}

/// Returns a checked month index for PHP `date()` name tables.
pub(in crate::interpreter) fn eval_tm_month_index(tm: &libc::tm) -> Result<usize, EvalStatus> {
    let index = usize::try_from(tm.tm_mon).map_err(|_| EvalStatus::RuntimeFatal)?;
    if index >= EVAL_MONTH_NAMES.len() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(index)
}

/// Returns a checked weekday index for PHP `date()` name tables.
pub(in crate::interpreter) fn eval_tm_weekday_index(tm: &libc::tm) -> Result<usize, EvalStatus> {
    let index = usize::try_from(tm.tm_wday).map_err(|_| EvalStatus::RuntimeFatal)?;
    if index >= EVAL_WEEKDAY_NAMES.len() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(index)
}

/// Appends one zero-padded decimal value with the requested minimum width.
pub(in crate::interpreter) fn eval_push_padded_number(
    output: &mut Vec<u8>,
    value: i64,
    width: usize,
) {
    output.extend_from_slice(format!("{value:0width$}").as_bytes());
}
