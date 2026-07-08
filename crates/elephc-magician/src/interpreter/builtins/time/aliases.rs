//! Purpose:
//! Executes procedural DateTime, DateInterval, DateTimeZone, and calendar aliases
//! that static elephc normally rewrites before type checking.
//!
//! Called from:
//! - `crate::interpreter::eval_call()`
//! - `crate::interpreter::builtins::registry::dispatch`
//!
//! Key details:
//! - The eval parser cannot run the static name-resolver rewrite, so this module
//!   mirrors the alias dispatch at runtime and delegates to native AOT bridges.
//! - This file is a deliberate >500 LoC single-scope runtime bridge for
//!   procedural date/time aliases; splitting by alias would obscure the shared
//!   fallback and timezone-table rules.

use super::*;

#[path = "../../../../../../src/list_id_prelude/table.rs"]
mod timezone_identifier_table;

const EVAL_TZ_VERSION: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../elephc-tz/data/version.data"));
const EVAL_DATETIMEZONE_ALL: i64 = 2047;
const EVAL_DATETIMEZONE_PER_COUNTRY: i64 = 4096;
const EVAL_DATETIMEZONE_PER_COUNTRY_ERROR: &str = concat!(
    "DateTimeZone::listIdentifiers(): Argument #2 ($countryCode) must be a two-letter ",
    "ISO 3166-1 compatible country code when argument #1 ($timezoneGroup) is ",
    "DateTimeZone::PER_COUNTRY",
);

/// Attempts to execute one direct procedural date/time alias call.
pub(in crate::interpreter) fn eval_date_procedural_alias_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if eval_date_alias_key(name).is_none() {
        return Ok(None);
    }
    let evaluated_args = eval_call_arg_values(args, context, scope, values)?;
    eval_date_procedural_alias_with_evaluated_args(name, evaluated_args, context, values)
}

/// Attempts to execute one procedural date/time alias from positional runtime values.
pub(in crate::interpreter) fn eval_date_procedural_alias_with_values(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let evaluated_args = evaluated_args
        .iter()
        .copied()
        .map(|value| EvaluatedCallArg {
            name: None,
            value,
            ref_target: None,
        })
        .collect();
    eval_date_procedural_alias_with_evaluated_args(name, evaluated_args, context, values)
}

/// Attempts to execute one procedural date/time alias from evaluated call args.
pub(in crate::interpreter) fn eval_date_procedural_alias_with_evaluated_args(
    name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(name) = eval_date_alias_key(name) else {
        return Ok(None);
    };
    if eval_date_alias_should_fall_back_to_builtin(&name, &evaluated_args) {
        return Ok(None);
    }
    let args = positional_evaluated_arg_values(evaluated_args)?;
    let result = eval_date_alias_result(&name, args, context, values)?;
    Ok(Some(result))
}

/// Dispatches a normalized alias name to the equivalent runtime operation.
fn eval_date_alias_result(
    name: &str,
    args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "idate" => eval_idate_alias(args, context, values),
        "mktime" | "gmmktime" => eval_mktime_alias(name, args, context, values),
        "date_create" => eval_new_datetime_alias("DateTime", args, context, values),
        "date_create_immutable" => {
            eval_new_datetime_alias("DateTimeImmutable", args, context, values)
        }
        "date_create_from_format" => {
            eval_static_alias("DateTime", "createFromFormat", args, context, values)
        }
        "date_create_immutable_from_format" => {
            eval_static_alias("DateTimeImmutable", "createFromFormat", args, context, values)
        }
        "date_parse_from_format" => {
            eval_static_alias("DateTime", "__elephc_date_parse_from_format", args, context, values)
        }
        "date_parse" => eval_static_alias("DateTime", "__elephc_date_parse", args, context, values),
        "date_sun_info" => {
            eval_static_alias("DateTime", "__elephc_date_sun_info", args, context, values)
        }
        "date_sunrise" => eval_date_sunfunc_alias(false, args, context, values),
        "date_sunset" => eval_date_sunfunc_alias(true, args, context, values),
        "strptime" => eval_static_alias("DateTime", "__elephc_strptime", args, context, values),
        "timezone_name_from_abbr" => eval_static_alias(
            "DateTime",
            "__elephc_timezone_name_from_abbr",
            args,
            context,
            values,
        ),
        "cal_to_jd" => eval_static_alias("DateTime", "__elephc_cal_to_jd", args, context, values),
        "cal_from_jd" => {
            eval_static_alias("DateTime", "__elephc_cal_from_jd", args, context, values)
        }
        "cal_days_in_month" => eval_static_alias(
            "DateTime",
            "__elephc_cal_days_in_month",
            args,
            context,
            values,
        ),
        "cal_info" => eval_static_alias("DateTime", "__elephc_cal_info", args, context, values),
        "gregoriantojd" => {
            eval_static_alias("DateTime", "__elephc_gregoriantojd", args, context, values)
        }
        "jdtogregorian" => {
            eval_static_alias("DateTime", "__elephc_jdtogregorian", args, context, values)
        }
        "juliantojd" => {
            eval_static_alias("DateTime", "__elephc_juliantojd", args, context, values)
        }
        "jdtojulian" => eval_static_alias("DateTime", "__elephc_jdtojulian", args, context, values),
        "frenchtojd" => {
            eval_static_alias("DateTime", "__elephc_frenchtojd", args, context, values)
        }
        "jdtofrench" => eval_static_alias("DateTime", "__elephc_jdtofrench", args, context, values),
        "jewishtojd" => {
            eval_static_alias("DateTime", "__elephc_jewishtojd", args, context, values)
        }
        "jdtojewish" => eval_static_alias("DateTime", "__elephc_jdtojewish", args, context, values),
        "jddayofweek" => {
            eval_static_alias("DateTime", "__elephc_jddayofweek", args, context, values)
        }
        "jdmonthname" => {
            eval_static_alias("DateTime", "__elephc_jdmonthname", args, context, values)
        }
        "jdtounix" => eval_static_alias("DateTime", "__elephc_jdtounix", args, context, values),
        "unixtojd" => eval_unixtojd_alias(args, context, values),
        "easter_days" => eval_easter_alias("__elephc_easter_days", args, context, values),
        "easter_date" => eval_easter_alias("__elephc_easter_date", args, context, values),
        "gettimeofday" => {
            eval_static_alias("DateTime", "__elephc_gettimeofday", args, context, values)
        }
        "date_get_last_errors" => {
            eval_static_alias("DateTime", "getLastErrors", args, context, values)
        }
        "strftime" => eval_strftime_alias(false, args, context, values),
        "gmstrftime" => eval_strftime_alias(true, args, context, values),
        "timezone_open" => eval_new_datetime_alias("DateTimeZone", args, context, values),
        "timezone_identifiers_list" => eval_timezone_identifiers_alias(args, context, values),
        "timezone_location_get" => eval_method_alias(args, 0, "getLocation", &[], context, values),
        "timezone_transitions_get" => {
            eval_method_alias_tail(args, 0, "getTransitions", context, values)
        }
        "timezone_abbreviations_list" => {
            eval_static_alias("DateTimeZone", "listAbbreviations", args, context, values)
        }
        "timezone_version_get" => eval_timezone_version_alias(args, values),
        "date_interval_create_from_date_string" => {
            eval_static_alias("DateInterval", "createFromDateString", args, context, values)
        }
        "date_diff" => eval_method_alias_tail(args, 0, "diff", context, values),
        "date_format" => eval_method_alias(args, 0, "format", &[1], context, values),
        "date_add" => eval_method_alias(args, 0, "add", &[1], context, values),
        "date_sub" => eval_method_alias(args, 0, "sub", &[1], context, values),
        "date_modify" => eval_method_alias(args, 0, "modify", &[1], context, values),
        "date_timestamp_get" => eval_method_alias(args, 0, "getTimestamp", &[], context, values),
        "date_timestamp_set" => {
            eval_method_alias(args, 0, "setTimestamp", &[1], context, values)
        }
        "date_timezone_get" => eval_method_alias(args, 0, "getTimezone", &[], context, values),
        "date_timezone_set" => {
            eval_method_alias(args, 0, "setTimezone", &[1], context, values)
        }
        "date_offset_get" => eval_method_alias(args, 0, "getOffset", &[], context, values),
        "date_date_set" => eval_method_alias(args, 0, "setDate", &[1, 2, 3], context, values),
        "date_isodate_set" => eval_method_alias_tail(args, 0, "setISODate", context, values),
        "date_time_set" => eval_method_alias_tail(args, 0, "setTime", context, values),
        "date_interval_format" => eval_method_alias(args, 0, "format", &[1], context, values),
        "timezone_name_get" => eval_method_alias(args, 0, "getName", &[], context, values),
        "timezone_offset_get" => eval_method_alias(args, 0, "getOffset", &[1], context, values),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Implements `idate()` as `intval(date(...))`.
fn eval_idate_alias(
    args: Vec<RuntimeCellHandle>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let result = match args.as_slice() {
        [format] => eval_date_result("date", *format, None, context, values),
        [format, timestamp] => eval_date_result("date", *format, Some(*timestamp), context, values),
        _ => return Err(EvalStatus::RuntimeFatal),
    }?;
    let cast = values.cast_int(result);
    values.release(result)?;
    cast
}

/// Implements `mktime()` and `gmmktime()` optional argument filling.
fn eval_mktime_alias(
    name: &str,
    args: Vec<RuntimeCellHandle>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() > 6 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut full = Vec::with_capacity(6);
    let mut temps = Vec::new();
    let date_name = if name == "gmmktime" { "gmdate" } else { "date" };
    for (index, spec) in ["G", "i", "s", "n", "j", "Y"].into_iter().enumerate() {
        if let Some(arg) = args.get(index) {
            full.push(*arg);
        } else {
            let default = eval_current_date_part_int(date_name, spec, context, values)?;
            temps.push(default);
            full.push(default);
        }
    }
    let result = eval_mktime_result(
        name, full[0], full[1], full[2], full[3], full[4], full[5], context, values,
    );
    for temp in temps {
        values.release(temp)?;
    }
    result
}

/// Constructs one native DateTime-family object and runs its constructor.
fn eval_new_datetime_alias(
    class_name: &str,
    args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let object = values.new_object(class_name)?;
    if let Err(status) =
        eval_native_constructor_with_evaluated_args(class_name, object, positional_args(args), context, values)
    {
        let _ = values.release(object);
        return Err(status);
    }
    Ok(object)
}

/// Calls one native/static method alias with positional arguments.
fn eval_static_alias(
    class_name: &str,
    method_name: &str,
    args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_static_method_call_result(class_name, method_name, positional_args(args), context, values)
}

/// Calls the injected list-identifiers prelude function, falling back to the
/// synthetic method if the native prelude function is unavailable.
fn eval_timezone_identifiers_alias(
    args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(function) = context.native_function("__elephc_list_identifiers") {
        let bound_args =
            bind_evaluated_native_function_args(&function, positional_args(args), context, values)?;
        return eval_native_function_with_values(function, bound_args, context, values);
    }
    eval_timezone_identifiers_filtered_alias(args, context, values)
}

/// Implements `timezone_identifiers_list()` filtering for eval-only programs.
fn eval_timezone_identifiers_filtered_alias(
    args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() > 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let group = eval_timezone_identifier_group(args.first().copied(), values)?;
    let country = eval_timezone_identifier_country(args.get(1).copied(), values)?;
    if group & EVAL_DATETIMEZONE_PER_COUNTRY != 0 && country.is_empty() {
        return eval_timezone_identifiers_country_error(context, values);
    }
    let rows = eval_timezone_identifier_rows(group, &country);
    let mut result = values.array_new(rows.len())?;
    for (index, name) in rows.into_iter().enumerate() {
        let key = values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        let value = values.string(name)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Returns the requested DateTimeZone group mask, defaulting to `DateTimeZone::ALL`.
fn eval_timezone_identifier_group(
    value: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    value
        .map(|value| eval_int_value(value, values))
        .unwrap_or(Ok(EVAL_DATETIMEZONE_ALL))
}

/// Returns the requested PER_COUNTRY country code, defaulting to the empty marker.
fn eval_timezone_identifier_country(
    value: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let Some(value) = value else {
        return Ok(String::new());
    };
    String::from_utf8(values.string_bytes(value)?).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Returns the identifiers matching one DateTimeZone group/country selector.
fn eval_timezone_identifier_rows(group: i64, country: &str) -> Vec<&'static str> {
    timezone_identifier_table::TIMEZONE_GROUPS_TABLE
        .split(';')
        .filter_map(|row| eval_timezone_identifier_row(row, group, country))
        .collect()
}

/// Returns one table row's identifier when it matches the selector.
fn eval_timezone_identifier_row(
    row: &'static str,
    group: i64,
    country: &str,
) -> Option<&'static str> {
    let mut fields = row.split(',');
    let name = fields.next()?;
    let mask = fields.next()?.parse::<i64>().ok()?;
    let row_country = fields.next()?;
    if group & EVAL_DATETIMEZONE_PER_COUNTRY != 0 {
        (row_country == country).then_some(name)
    } else {
        (mask & group != 0).then_some(name)
    }
}

/// Throws PHP's `ValueError` for PER_COUNTRY calls without a country code.
fn eval_timezone_identifiers_country_error<T>(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    let exception = values.new_object("ValueError")?;
    let message = values.string(EVAL_DATETIMEZONE_PER_COUNTRY_ERROR)?;
    let code = values.int(0)?;
    values.construct_object(exception, vec![message, code])?;
    context.set_pending_throw(exception);
    Err(EvalStatus::UncaughtThrowable)
}

/// Calls one object-method alias with selected argument indices.
fn eval_method_alias(
    args: Vec<RuntimeCellHandle>,
    object_index: usize,
    method_name: &str,
    arg_indices: &[usize],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(object) = args.get(object_index).copied() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let mut method_args = Vec::with_capacity(arg_indices.len());
    for index in arg_indices {
        let Some(arg) = args.get(*index).copied() else {
            return Err(EvalStatus::RuntimeFatal);
        };
        method_args.push(arg);
    }
    eval_method_call_result(object, method_name, method_args, context, values)
}

/// Calls one object-method alias with every argument after the receiver.
fn eval_method_alias_tail(
    args: Vec<RuntimeCellHandle>,
    object_index: usize,
    method_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(object) = args.get(object_index).copied() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let method_args = args
        .iter()
        .enumerate()
        .filter_map(|(index, arg)| (index != object_index).then_some(*arg))
        .collect();
    eval_method_call_result(object, method_name, method_args, context, values)
}

/// Implements `unixtojd($timestamp = time())`.
fn eval_unixtojd_alias(
    args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (args, temp) = match args.as_slice() {
        [] => {
            let now = eval_time_result(values)?;
            (vec![now], Some(now))
        }
        [_] => (args, None),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let result = eval_static_alias("DateTime", "__elephc_unixtojd", args, context, values);
    if let Some(temp) = temp {
        values.release(temp)?;
    }
    result
}

/// Implements `easter_days()` and `easter_date()` default-year filling.
fn eval_easter_alias(
    method_name: &str,
    args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() > 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let (args, temp) = if args.is_empty() {
        let year = eval_current_date_part_int("date", "Y", context, values)?;
        (vec![year], Some(year))
    } else {
        (args, None)
    };
    let result = eval_static_alias("DateTime", method_name, args, context, values);
    if let Some(temp) = temp {
        values.release(temp)?;
    }
    result
}

/// Implements `date_sunrise()` and `date_sunset()` by prepending the synthetic flag.
fn eval_date_sunfunc_alias(
    sunset: bool,
    args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(1..=6).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let which = values.int(if sunset { 1 } else { 0 })?;
    let mut call_args = Vec::with_capacity(args.len() + 1);
    call_args.push(which);
    call_args.extend(args);
    let result = eval_static_alias("DateTime", "__elephc_date_sunfunc", call_args, context, values);
    values.release(which)?;
    result
}

/// Implements `strftime()` and `gmstrftime()` by adding timestamp and UTC flag.
fn eval_strftime_alias(
    utc: bool,
    args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() != 1 && args.len() != 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut call_args = Vec::with_capacity(3);
    call_args.push(args[0]);
    let mut temps = Vec::new();
    if let Some(timestamp) = args.get(1) {
        call_args.push(*timestamp);
    } else {
        let timestamp = eval_time_result(values)?;
        temps.push(timestamp);
        call_args.push(timestamp);
    }
    let utc = values.bool_value(utc)?;
    temps.push(utc);
    call_args.push(utc);
    let result = eval_static_alias("DateTime", "__elephc_strftime", call_args, context, values);
    for temp in temps {
        values.release(temp)?;
    }
    result
}

/// Returns the bundled timezone database version.
fn eval_timezone_version_alias(
    args: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    values.string(EVAL_TZ_VERSION.trim())
}

/// Evaluates one current date part as an integer runtime cell.
fn eval_current_date_part_int(
    date_name: &str,
    spec: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let format = values.string(spec)?;
    let date = match eval_date_result(date_name, format, None, context, values) {
        Ok(date) => date,
        Err(status) => {
            values.release(format)?;
            return Err(status);
        }
    };
    values.release(format)?;
    let value = values.cast_int(date);
    values.release(date)?;
    value
}

/// Normalizes a possible alias name to its lowercase bare name.
fn eval_date_alias_key(name: &str) -> Option<String> {
    let bare = name
        .rsplit('\\')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    eval_date_alias_is_supported(&bare).then_some(bare)
}

/// Returns true when a real builtin should keep handling this named call shape.
fn eval_date_alias_should_fall_back_to_builtin(
    name: &str,
    args: &[EvaluatedCallArg],
) -> bool {
    matches!(name, "mktime" | "gmmktime") && args.iter().any(|arg| arg.name.is_some())
}

/// Returns whether eval has runtime dispatch for one procedural date/time alias.
fn eval_date_alias_is_supported(name: &str) -> bool {
    matches!(
        name,
        "idate"
            | "mktime"
            | "gmmktime"
            | "date_create"
            | "date_create_immutable"
            | "date_create_from_format"
            | "date_create_immutable_from_format"
            | "date_parse_from_format"
            | "date_parse"
            | "date_sun_info"
            | "date_sunrise"
            | "date_sunset"
            | "strptime"
            | "timezone_name_from_abbr"
            | "cal_to_jd"
            | "cal_from_jd"
            | "cal_days_in_month"
            | "cal_info"
            | "gregoriantojd"
            | "jdtogregorian"
            | "juliantojd"
            | "jdtojulian"
            | "frenchtojd"
            | "jdtofrench"
            | "jewishtojd"
            | "jdtojewish"
            | "jddayofweek"
            | "jdmonthname"
            | "jdtounix"
            | "unixtojd"
            | "easter_days"
            | "easter_date"
            | "gettimeofday"
            | "date_get_last_errors"
            | "strftime"
            | "gmstrftime"
            | "timezone_open"
            | "timezone_identifiers_list"
            | "timezone_location_get"
            | "timezone_transitions_get"
            | "timezone_abbreviations_list"
            | "timezone_version_get"
            | "date_interval_create_from_date_string"
            | "date_diff"
            | "date_format"
            | "date_add"
            | "date_sub"
            | "date_modify"
            | "date_timestamp_get"
            | "date_timestamp_set"
            | "date_timezone_get"
            | "date_timezone_set"
            | "date_offset_get"
            | "date_date_set"
            | "date_isodate_set"
            | "date_time_set"
            | "date_interval_format"
            | "timezone_name_get"
            | "timezone_offset_get"
    )
}
