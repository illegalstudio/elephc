//! Purpose:
//! Time, date, sleep, PHP version, and uname builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins` re-exports used by core call dispatch.
//!
//! Key details:
//! - Helpers stay scoped to the eval interpreter and preserve PHP-visible runtime
//!   behavior through `RuntimeValueOps`.

use super::super::*;
use super::*;

/// Evaluates PHP `time()` with no arguments.
pub(in crate::interpreter) fn eval_builtin_time(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_time_result(values)
}

/// Returns the current Unix timestamp as a boxed PHP integer.
pub(in crate::interpreter) fn eval_time_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.int(eval_current_unix_timestamp()?)
}

/// Returns the current Unix timestamp as an integer payload.
pub(in crate::interpreter) fn eval_current_unix_timestamp() -> Result<i64, EvalStatus> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| EvalStatus::RuntimeFatal)?
        .as_secs();
    i64::try_from(timestamp).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Evaluates PHP `date($format, $timestamp = time())` for the eval subset.
pub(in crate::interpreter) fn eval_builtin_date(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [format] => {
            let format = eval_expr(format, context, scope, values)?;
            eval_date_result(format, None, values)
        }
        [format, timestamp] => {
            let format = eval_expr(format, context, scope, values)?;
            let timestamp = eval_expr(timestamp, context, scope, values)?;
            eval_date_result(format, Some(timestamp), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Formats one Unix timestamp through PHP `date()` token rules supported by elephc.
pub(in crate::interpreter) fn eval_date_result(
    format: RuntimeCellHandle,
    timestamp: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let format = values.string_bytes(format)?;
    let timestamp = match timestamp {
        Some(timestamp) => eval_int_value(timestamp, values)?,
        None => eval_current_unix_timestamp()?,
    };
    let tm = eval_localtime(timestamp)?;
    let output = eval_format_date_bytes(&format, &tm, timestamp)?;
    values.string_bytes_value(&output)
}

/// Converts one Unix timestamp to local broken-down time through libc.
pub(in crate::interpreter) fn eval_localtime(timestamp: i64) -> Result<libc::tm, EvalStatus> {
    let raw: libc::time_t = timestamp.try_into().map_err(|_| EvalStatus::RuntimeFatal)?;
    let mut tm = MaybeUninit::<libc::tm>::uninit();
    let result = unsafe { libc::localtime_r(&raw, tm.as_mut_ptr()) };
    if result.is_null() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(unsafe { tm.assume_init() })
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

/// Evaluates PHP `mktime(hour, minute, second, month, day, year)`.
pub(in crate::interpreter) fn eval_builtin_mktime(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [hour, minute, second, month, day, year] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let hour = eval_expr(hour, context, scope, values)?;
    let minute = eval_expr(minute, context, scope, values)?;
    let second = eval_expr(second, context, scope, values)?;
    let month = eval_expr(month, context, scope, values)?;
    let day = eval_expr(day, context, scope, values)?;
    let year = eval_expr(year, context, scope, values)?;
    eval_mktime_result(hour, minute, second, month, day, year, values)
}

/// Converts PHP date components to a local Unix timestamp through libc `mktime`.
pub(in crate::interpreter) fn eval_mktime_result(
    hour: RuntimeCellHandle,
    minute: RuntimeCellHandle,
    second: RuntimeCellHandle,
    month: RuntimeCellHandle,
    day: RuntimeCellHandle,
    year: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let timestamp = eval_mktime_timestamp(
        eval_int_cell_as_c_int(hour, values)?,
        eval_int_cell_as_c_int(minute, values)?,
        eval_int_cell_as_c_int(second, values)?,
        eval_int_cell_as_c_int(month, values)?,
        eval_int_cell_as_c_int(day, values)?,
        eval_int_cell_as_c_int(year, values)?,
    )?;
    values.int(timestamp)
}

/// Converts local date components into a Unix timestamp through libc `mktime`.
pub(in crate::interpreter) fn eval_mktime_timestamp(
    hour: libc::c_int,
    minute: libc::c_int,
    second: libc::c_int,
    month: libc::c_int,
    day: libc::c_int,
    year: libc::c_int,
) -> Result<i64, EvalStatus> {
    let mut tm = unsafe { MaybeUninit::<libc::tm>::zeroed().assume_init() };
    tm.tm_hour = hour;
    tm.tm_min = minute;
    tm.tm_sec = second;
    tm.tm_mon = month - 1;
    tm.tm_mday = day;
    tm.tm_year = year - 1900;
    tm.tm_isdst = -1;
    let timestamp = unsafe { libc::mktime(&mut tm) };
    i64::try_from(timestamp).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Casts one eval cell to a PHP int and checks it fits a libc `c_int`.
pub(in crate::interpreter) fn eval_int_cell_as_c_int(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<libc::c_int, EvalStatus> {
    let value = eval_int_value(value, values)?;
    libc::c_int::try_from(value).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Evaluates PHP `strtotime(datetime)` for eval's supported date-string subset.
pub(in crate::interpreter) fn eval_builtin_strtotime(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [datetime] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let datetime = eval_expr(datetime, context, scope, values)?;
    eval_strtotime_result(datetime, values)
}

/// Parses one eval `strtotime()` input and boxes the resulting timestamp.
pub(in crate::interpreter) fn eval_strtotime_result(
    datetime: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(datetime)?;
    let timestamp = eval_strtotime_bytes(&bytes)?;
    values.int(timestamp)
}

/// Parses eval's supported `strtotime()` strings into local Unix timestamps.
pub(in crate::interpreter) fn eval_strtotime_bytes(bytes: &[u8]) -> Result<i64, EvalStatus> {
    let bytes = eval_trim_ascii_whitespace(bytes);
    if bytes.eq_ignore_ascii_case(b"now") {
        return eval_current_unix_timestamp();
    }
    let Some((year, month, day, hour, minute, second)) = eval_parse_iso_datetime(bytes) else {
        return Ok(-1);
    };
    eval_mktime_timestamp(hour, minute, second, month, day, year)
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

/// Evaluates PHP `microtime()` with an optional ignored argument.
pub(in crate::interpreter) fn eval_builtin_microtime(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_microtime_result(values),
        [as_float] => {
            let _ = eval_expr(as_float, context, scope, values)?;
            eval_microtime_result(values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Returns the current Unix timestamp with microsecond precision as a boxed float.
pub(in crate::interpreter) fn eval_microtime_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| EvalStatus::RuntimeFatal)?;
    let seconds = timestamp.as_secs() as f64;
    let micros = f64::from(timestamp.subsec_micros()) / 1_000_000.0;
    values.float(seconds + micros)
}

/// Evaluates PHP `sleep($seconds)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_sleep(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [seconds] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let seconds = eval_expr(seconds, context, scope, values)?;
    eval_sleep_result(seconds, values)
}

/// Sleeps for a non-negative number of seconds and returns PHP's remaining-seconds value.
pub(in crate::interpreter) fn eval_sleep_result(
    seconds: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let seconds = eval_int_value(seconds, values)?;
    let seconds = u64::try_from(seconds).map_err(|_| EvalStatus::RuntimeFatal)?;
    std::thread::sleep(std::time::Duration::from_secs(seconds));
    values.int(0)
}

/// Evaluates PHP `usleep($microseconds)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_usleep(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [microseconds] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let microseconds = eval_expr(microseconds, context, scope, values)?;
    eval_usleep_result(microseconds, values)
}

/// Sleeps for a non-negative number of microseconds and returns PHP null.
pub(in crate::interpreter) fn eval_usleep_result(
    microseconds: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let microseconds = eval_int_value(microseconds, values)?;
    let microseconds = u64::try_from(microseconds).map_err(|_| EvalStatus::RuntimeFatal)?;
    std::thread::sleep(std::time::Duration::from_micros(microseconds));
    values.null()
}

/// Evaluates PHP `phpversion()` with no arguments.
pub(in crate::interpreter) fn eval_builtin_phpversion(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_phpversion_result(values)
}

/// Returns the root elephc package version as a boxed PHP string.
pub(in crate::interpreter) fn eval_phpversion_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.string(eval_compiler_php_version())
}

/// Reads the root package version from the workspace manifest used by native `phpversion()`.
pub(in crate::interpreter) fn eval_compiler_php_version() -> &'static str {
    let mut in_package = false;
    for line in EVAL_ROOT_CARGO_TOML.lines() {
        let line = line.trim();
        if line == "[package]" {
            in_package = true;
            continue;
        }
        if in_package && line.starts_with('[') {
            break;
        }
        if in_package {
            if let Some(value) = line.strip_prefix("version = ") {
                return value.trim_matches('"');
            }
        }
    }
    env!("CARGO_PKG_VERSION")
}

/// Evaluates PHP `php_uname($mode = "a")` over zero or one eval expression.
pub(in crate::interpreter) fn eval_builtin_php_uname(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_php_uname_result(None, values),
        [mode] => {
            let mode = eval_expr(mode, context, scope, values)?;
            eval_php_uname_result(Some(mode), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Reads the local uname fields and formats the PHP `php_uname()` mode result.
pub(in crate::interpreter) fn eval_php_uname_result(
    mode: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mode = match mode {
        Some(mode) => {
            let bytes = values.string_bytes(mode)?;
            let [mode] = bytes.as_slice() else {
                return Err(EvalStatus::RuntimeFatal);
            };
            *mode
        }
        None => b'a',
    };

    let mut utsname = std::mem::MaybeUninit::<libc::utsname>::zeroed();
    let status = unsafe {
        // libc writes all uname fields into the stack-owned utsname buffer.
        libc::uname(utsname.as_mut_ptr())
    };
    if status != 0 {
        return values.string("");
    }
    let utsname = unsafe {
        // `uname` succeeded, so libc initialized the full `utsname` structure.
        utsname.assume_init()
    };
    let sysname = eval_uname_field_bytes(&utsname.sysname);
    let nodename = eval_uname_field_bytes(&utsname.nodename);
    let release = eval_uname_field_bytes(&utsname.release);
    let version = eval_uname_field_bytes(&utsname.version);
    let machine = eval_uname_field_bytes(&utsname.machine);

    match mode {
        b'a' => {
            let mut output = Vec::new();
            for field in [&sysname, &nodename, &release, &version, &machine] {
                if !output.is_empty() {
                    output.push(b' ');
                }
                output.extend_from_slice(field);
            }
            values.string_bytes_value(&output)
        }
        b's' => values.string_bytes_value(&sysname),
        b'n' => values.string_bytes_value(&nodename),
        b'r' => values.string_bytes_value(&release),
        b'v' => values.string_bytes_value(&version),
        b'm' => values.string_bytes_value(&machine),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Copies one NUL-terminated `utsname` field into raw PHP string bytes.
pub(in crate::interpreter) fn eval_uname_field_bytes(field: &[libc::c_char]) -> Vec<u8> {
    let length = field
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(field.len());
    field[..length].iter().map(|byte| *byte as u8).collect()
}
