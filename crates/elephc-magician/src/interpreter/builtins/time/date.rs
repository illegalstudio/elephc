//! Purpose:
//! Eval registry entry and implementation for `date` plus shared date-format helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` direct and by-value dispatch.
//!
//! Key details:
//! - `gmdate` calls this file for shared formatting and UTC/local timestamp conversion.
//! - Timezone environment writes use Rust's environment lock shared with process spawning.

use std::ffi::OsStr;
use std::sync::Mutex;

use super::super::*;
use super::*;

eval_builtin! {
    contract: "date",
    area: Time,
    direct: Time,
    values: Time,
}

static EVAL_TZ_MUTEX: Mutex<()> = Mutex::new(());

/// PHP-visible broken-down time without libc's narrow year representation.
pub(in crate::interpreter) struct EvalDateTime {
    pub tm_sec: i32,
    pub tm_min: i32,
    pub tm_hour: i32,
    pub tm_mday: i32,
    pub tm_mon: i32,
    pub tm_year: i64,
    pub tm_wday: i32,
    pub tm_yday: i32,
    pub tm_isdst: i32,
    pub utc_offset: i64,
}

#[cfg(unix)]
unsafe extern "C" {
    /// Re-reads libc's process-global timezone environment.
    fn tzset();
}

#[cfg(windows)]
unsafe extern "C" {
    /// Converts a 64-bit Unix timestamp into local broken-down time through the Windows CRT.
    #[link_name = "_localtime64_s"]
    fn windows_localtime64_s(output: *mut libc::tm, timestamp: *const i64) -> libc::c_int;

    /// Re-reads the Windows CRT process-global timezone environment.
    #[link_name = "_tzset"]
    fn windows_tzset();
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
) -> Result<EvalDateTime, EvalStatus> {
    #[cfg(windows)]
    if let Some((offset, is_dst, _abbreviation)) =
        elephc_tz::zone_offset_at(context.default_timezone(), timestamp)
    {
        let mut local = eval_civil_time(elephc_tz::civil_time_at_offset(timestamp, offset), offset);
        local.tm_isdst = i32::from(is_dst);
        return Ok(local);
    }
    eval_with_timezone(context.default_timezone(), || {
        let tm = eval_localtime(timestamp)?;
        eval_date_time_from_libc(tm, timestamp)
    })
}

/// Formats one RFC 2822 mail Date using the eval context's active timezone.
pub(in crate::interpreter) fn eval_mail_date_at(
    timestamp: i64,
    context: &ElephcEvalContext,
) -> Result<Vec<u8>, EvalStatus> {
    let local = eval_context_localtime(timestamp, context)?;
    eval_format_date_bytes(b"r", &local, timestamp)
}

/// Converts one Unix timestamp to process-local broken-down time through libc.
pub(in crate::interpreter) fn eval_localtime(timestamp: i64) -> Result<libc::tm, EvalStatus> {
    #[cfg(unix)]
    {
    let raw: libc::time_t = timestamp.try_into().map_err(|_| EvalStatus::RuntimeFatal)?;
    let mut tm = MaybeUninit::<libc::tm>::uninit();
    let result = unsafe { libc::localtime_r(&raw, tm.as_mut_ptr()) };
    if result.is_null() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(unsafe { tm.assume_init() })
    }
    #[cfg(windows)]
    {
        let mut tm = MaybeUninit::<libc::tm>::uninit();
        let status = unsafe { windows_localtime64_s(tm.as_mut_ptr(), &timestamp) };
        if status != 0 {
            return Err(EvalStatus::RuntimeFatal);
        }
        Ok(unsafe { tm.assume_init() })
    }
}

/// Converts one Unix timestamp to UTC broken-down time through libc.
pub(in crate::interpreter) fn eval_gmtime(timestamp: i64) -> Result<EvalDateTime, EvalStatus> {
    Ok(eval_civil_time(elephc_tz::utc_civil_time(timestamp), 0))
}

/// Adapts wide pure-Rust civil components to the PHP-facing date representation.
fn eval_civil_time(civil: elephc_tz::UtcCivilTime, utc_offset: i32) -> EvalDateTime {
    EvalDateTime {
        tm_sec: civil.second as i32,
        tm_min: civil.minute as i32,
        tm_hour: civil.hour as i32,
        tm_mday: civil.day as i32,
        tm_mon: civil.month as i32 - 1,
        tm_year: civil.year - 1900,
        tm_wday: civil.weekday as i32,
        tm_yday: civil.yearday as i32,
        tm_isdst: 0,
        utc_offset: i64::from(utc_offset),
    }
}

/// Adapts libc local time while retaining its exact offset from the source instant.
fn eval_date_time_from_libc(tm: libc::tm, timestamp: i64) -> Result<EvalDateTime, EvalStatus> {
    #[cfg(unix)]
    let local_as_utc = {
        let mut offset_fields = tm;
        (unsafe { libc::timegm(&mut offset_fields) }) as i128
    };
    #[cfg(windows)]
    let local_as_utc = elephc_tz::utc_timestamp_from_civil(
        i64::from(tm.tm_year) + 1900,
        u32::try_from(tm.tm_mon + 1).map_err(|_| EvalStatus::RuntimeFatal)?,
        u32::try_from(tm.tm_mday).map_err(|_| EvalStatus::RuntimeFatal)?,
        u32::try_from(tm.tm_hour).map_err(|_| EvalStatus::RuntimeFatal)?,
        u32::try_from(tm.tm_min).map_err(|_| EvalStatus::RuntimeFatal)?,
        u32::try_from(tm.tm_sec).map_err(|_| EvalStatus::RuntimeFatal)?,
    )
    .ok_or(EvalStatus::RuntimeFatal)? as i128;
    let offset = local_as_utc - i128::from(timestamp);
    Ok(EvalDateTime {
        tm_sec: tm.tm_sec,
        tm_min: tm.tm_min,
        tm_hour: tm.tm_hour,
        tm_mday: tm.tm_mday,
        tm_mon: tm.tm_mon,
        tm_year: i64::from(tm.tm_year),
        tm_wday: tm.tm_wday,
        tm_yday: tm.tm_yday,
        tm_isdst: tm.tm_isdst,
        utc_offset: i64::try_from(offset).map_err(|_| EvalStatus::RuntimeFatal)?,
    })
}

/// Runs one libc timezone-sensitive operation under the eval context timezone.
pub(in crate::interpreter) fn eval_with_timezone<T>(
    timezone: &str,
    operation: impl FnOnce() -> Result<T, EvalStatus>,
) -> Result<T, EvalStatus> {
    let _guard = EVAL_TZ_MUTEX
        .lock()
        .map_err(|_| EvalStatus::RuntimeFatal)?;
    let previous = std::env::var_os("TZ");
    eval_apply_process_timezone(timezone)?;
    let result = operation();
    eval_restore_process_timezone(previous.as_deref())?;
    result
}

/// Applies one timezone identifier to libc's process-global timezone state.
fn eval_apply_process_timezone(timezone: &str) -> Result<(), EvalStatus> {
    if timezone.as_bytes().contains(&0) {
        return Err(EvalStatus::RuntimeFatal);
    }
    std::env::set_var("TZ", timezone);
    #[cfg(unix)]
    unsafe { tzset() };
    #[cfg(windows)]
    unsafe {
        windows_tzset();
    }
    Ok(())
}

/// Restores the process timezone that was active before an eval-local conversion.
fn eval_restore_process_timezone(previous: Option<&OsStr>) -> Result<(), EvalStatus> {
    if let Some(value) = previous {
        std::env::set_var("TZ", value);
    } else {
        std::env::remove_var("TZ");
    }
    #[cfg(unix)]
    unsafe { tzset() };
    #[cfg(windows)]
    unsafe {
        windows_tzset();
    }
    Ok(())
}

/// Applies PHP `date()` tokens to one local broken-down timestamp.
pub(in crate::interpreter) fn eval_format_date_bytes(
    format: &[u8],
    tm: &EvalDateTime,
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
    tm: &EvalDateTime,
    timestamp: i64,
) -> Result<(), EvalStatus> {
    match token {
        b'Y' => eval_push_year(output, tm.tm_year + 1900),
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
        b'O' => eval_push_timezone_offset(output, tm.utc_offset),
        b'r' => {
            let composite = eval_format_date_bytes(b"D, d M Y H:i:s O", tm, timestamp)?;
            output.extend_from_slice(&composite);
        }
        b'U' => output.extend_from_slice(timestamp.to_string().as_bytes()),
        _ => output.push(token),
    }
    Ok(())
}

/// Appends PHP's signed year with at least four digits excluding the sign.
fn eval_push_year(output: &mut Vec<u8>, year: i64) {
    if year < 0 {
        output.push(b'-');
        output.extend_from_slice(format!("{:04}", year.unsigned_abs()).as_bytes());
    } else {
        output.extend_from_slice(format!("{year:04}").as_bytes());
    }
}

/// Appends PHP's RFC 2822 numeric timezone offset (`+HHMM` or `-HHMM`).
fn eval_push_timezone_offset(
    output: &mut Vec<u8>,
    offset: i64,
) {
    let sign = if offset < 0 { b'-' } else { b'+' };
    let absolute = offset.unsigned_abs();
    let hours = absolute / 3600;
    let minutes = (absolute % 3600) / 60;
    output.push(sign);
    eval_push_padded_number(output, hours as i64, 2);
    eval_push_padded_number(output, minutes as i64, 2);
}

/// Returns a checked month index for PHP `date()` name tables.
pub(in crate::interpreter) fn eval_tm_month_index(tm: &EvalDateTime) -> Result<usize, EvalStatus> {
    let index = usize::try_from(tm.tm_mon).map_err(|_| EvalStatus::RuntimeFatal)?;
    if index >= EVAL_MONTH_NAMES.len() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(index)
}

/// Returns a checked weekday index for PHP `date()` name tables.
pub(in crate::interpreter) fn eval_tm_weekday_index(tm: &EvalDateTime) -> Result<usize, EvalStatus> {
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

#[cfg(test)]
mod environment_api_tests {
    /// Calendar helpers cannot bypass the environment lock used by inherited process spawns.
    #[test]
    fn timezone_mutation_uses_the_environment_api_shared_with_process_spawn() {
        let source = include_str!("date.rs");
        let libc_setenv = ["libc::", "setenv("].concat();
        let libc_unsetenv = ["libc::", "unsetenv("].concat();
        let rust_set_var = ["std::env::", "set_var("].concat();
        let rust_remove_var = ["std::env::", "remove_var("].concat();

        assert!(!source.contains(&libc_setenv));
        assert!(!source.contains(&libc_unsetenv));
        assert_eq!(source.matches(&rust_set_var).count(), 2);
        assert_eq!(source.matches(&rust_remove_var).count(), 1);
    }
}
