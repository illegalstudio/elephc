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
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "date" => match evaluated_args {
            [format] => eval_date_result(*format, None, values)?,
            [format, timestamp] => eval_date_result(*format, Some(*timestamp), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "microtime" => match evaluated_args {
            [] | [_] => eval_microtime_result(values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "mktime" => {
            let [hour, minute, second, month, day, year] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_mktime_result(*hour, *minute, *second, *month, *day, *year, values)?
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
            eval_strtotime_result(*datetime, values)?
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
