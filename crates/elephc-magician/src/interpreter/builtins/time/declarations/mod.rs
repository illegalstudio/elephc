//! Purpose:
//! Declarative eval registry entries and dispatch adapters for time and
//! response-header builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` module loading.
//! - `crate::interpreter::builtins::hooks` for migrated time dispatch.
//!
//! Key details:
//! - Time/date and response-state algorithms remain in sibling helper modules
//!   such as `date`, `calendar`, `clock`, `mktime`, `sleep`, `strtotime`, and
//!   `system`.
//! - This module owns registry metadata and small hook adapters only.

use super::super::super::*;
use super::*;

mod checkdate;
mod date;
mod date_default_timezone_get;
mod date_default_timezone_set;
mod getdate;
mod gmdate;
mod gmmktime;
mod header;
mod hrtime;
mod http_response_code;
mod localtime;
mod microtime;
mod mktime;
mod sleep;
mod strtotime;
mod time;
mod usleep;

/// Dispatches direct expression-level calls for declaratively migrated time builtins.
pub(in crate::interpreter) fn eval_builtin_time_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "checkdate" => eval_builtin_checkdate(args, context, scope, values),
        "date" | "gmdate" => eval_builtin_date_like(name, args, context, scope, values),
        "date_default_timezone_get" => eval_builtin_date_default_timezone_get(args, context, values),
        "date_default_timezone_set" => {
            eval_builtin_date_default_timezone_set(args, context, scope, values)
        }
        "getdate" => eval_builtin_getdate(args, context, scope, values),
        "gmmktime" | "mktime" => eval_builtin_mktime_like(name, args, context, scope, values),
        "header" => eval_builtin_header(args, context, scope, values),
        "hrtime" => eval_builtin_hrtime(args, context, scope, values),
        "http_response_code" => eval_builtin_http_response_code(args, context, scope, values),
        "localtime" => eval_builtin_localtime(args, context, scope, values),
        "microtime" => eval_builtin_microtime(args, context, scope, values),
        "sleep" => eval_builtin_sleep(args, context, scope, values),
        "strtotime" => eval_builtin_strtotime(args, context, scope, values),
        "time" => eval_builtin_time(args, values),
        "usleep" => eval_builtin_usleep(args, context, scope, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Dispatches evaluated-argument calls for declaratively migrated time builtins.
pub(in crate::interpreter) fn eval_time_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "checkdate" => {
            let [month, day, year] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_checkdate_result(*month, *day, *year, values)
        }
        "date" | "gmdate" => match evaluated_args {
            [format] => eval_date_result(name, *format, None, context, values),
            [format, timestamp] => eval_date_result(name, *format, Some(*timestamp), context, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "date_default_timezone_get" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_date_default_timezone_get_result(context, values)
        }
        "date_default_timezone_set" => {
            let [timezone] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_date_default_timezone_set_result(*timezone, context, values)
        }
        "getdate" => match evaluated_args {
            [] => eval_getdate_result(None, context, values),
            [timestamp] => eval_getdate_result(Some(*timestamp), context, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "gmmktime" | "mktime" => {
            let [hour, minute, second, month, day, year] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_mktime_result(
                name, *hour, *minute, *second, *month, *day, *year, context, values,
            )
        }
        "header" => match evaluated_args {
            [line] => eval_header_result(*line, None, None, context, values),
            [line, replace] => eval_header_result(*line, Some(*replace), None, context, values),
            [line, replace, response_code] => {
                eval_header_result(*line, Some(*replace), Some(*response_code), context, values)
            }
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "hrtime" => match evaluated_args {
            [] => eval_hrtime_result(None, values),
            [as_number] => eval_hrtime_result(Some(*as_number), values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "http_response_code" => match evaluated_args {
            [] => eval_http_response_code_result(None, context, values),
            [response_code] => eval_http_response_code_result(Some(*response_code), context, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "localtime" => match evaluated_args {
            [] => eval_localtime_result(None, None, context, values),
            [timestamp] => eval_localtime_result(Some(*timestamp), None, context, values),
            [timestamp, associative] => {
                eval_localtime_result(Some(*timestamp), Some(*associative), context, values)
            }
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "microtime" => match evaluated_args {
            [] | [_] => eval_microtime_result(values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "sleep" => {
            let [seconds] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_sleep_result(*seconds, values)
        }
        "strtotime" => match evaluated_args {
            [datetime] => eval_strtotime_result(*datetime, None, context, values),
            [datetime, base_timestamp] => {
                eval_strtotime_result(*datetime, Some(*base_timestamp), context, values)
            }
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "time" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_time_result(values)
        }
        "usleep" => {
            let [microseconds] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_usleep_result(*microseconds, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
