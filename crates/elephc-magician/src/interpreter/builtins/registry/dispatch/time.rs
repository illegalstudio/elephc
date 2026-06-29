//! Purpose:
//! Dispatches already evaluated date, time, and sleep builtins by dynamic callable name.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry::dispatch`.
//!
//! Key details:
//! - Returns `Ok(None)` for names outside this domain so the parent dispatcher can
//!   continue probing other builtin families.

use super::super::super::super::*;
use super::super::super::*;

/// Attempts to dispatch evaluated date, time, and sleep builtins.
pub(in crate::interpreter) fn eval_time_builtin_with_values(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "checkdate" => {
            let [month, day, year] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_checkdate_result(*month, *day, *year, values)?
        }
        "date" | "gmdate" => match evaluated_args {
            [format] => eval_date_result(name, *format, None, context, values)?,
            [format, timestamp] => eval_date_result(name, *format, Some(*timestamp), context, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "date_default_timezone_get" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_date_default_timezone_get_result(context, values)?
        }
        "date_default_timezone_set" => {
            let [timezone] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_date_default_timezone_set_result(*timezone, context, values)?
        }
        "getdate" => match evaluated_args {
            [] => eval_getdate_result(None, context, values)?,
            [timestamp] => eval_getdate_result(Some(*timestamp), context, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "hrtime" => match evaluated_args {
            [] => eval_hrtime_result(None, values)?,
            [as_number] => eval_hrtime_result(Some(*as_number), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "header" => match evaluated_args {
            [line] => eval_header_result(*line, None, None, context, values)?,
            [line, replace] => eval_header_result(*line, Some(*replace), None, context, values)?,
            [line, replace, response_code] => {
                eval_header_result(*line, Some(*replace), Some(*response_code), context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "http_response_code" => match evaluated_args {
            [] => eval_http_response_code_result(None, context, values)?,
            [response_code] => eval_http_response_code_result(Some(*response_code), context, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "localtime" => match evaluated_args {
            [] => eval_localtime_result(None, None, context, values)?,
            [timestamp] => eval_localtime_result(Some(*timestamp), None, context, values)?,
            [timestamp, associative] => {
                eval_localtime_result(Some(*timestamp), Some(*associative), context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "microtime" => match evaluated_args {
            [] | [_] => eval_microtime_result(values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "mktime" | "gmmktime" => {
            let [hour, minute, second, month, day, year] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_mktime_result(
                name, *hour, *minute, *second, *month, *day, *year, context, values,
            )?
        }
        "sleep" => {
            let [seconds] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_sleep_result(*seconds, values)?
        }
        "time" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_time_result(values)?
        }
        "strtotime" => {
            let [datetime] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_strtotime_result(*datetime, context, values)?
        }
        "usleep" => {
            let [microseconds] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_usleep_result(*microseconds, values)?
        }
        _ => return Ok(None),
    };
    Ok(Some(result))
}
