//! Purpose:
//! Eval registry entry and implementation for `checkdate`.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` direct and by-value dispatch.
//!
//! Key details:
//! - Gregorian bounds and leap-year validation are owned by this builtin file.

use super::super::*;
use super::*;

eval_builtin! {
    name: "checkdate",
    area: Time,
    params: [month, day, year],
    direct: Time,
    values: Time,
}

/// Evaluates PHP `checkdate(month, day, year)` over three eval expressions.
pub(in crate::interpreter) fn eval_builtin_checkdate(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [month, day, year] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let month = eval_expr(month, context, scope, values)?;
    let day = eval_expr(day, context, scope, values)?;
    let year = eval_expr(year, context, scope, values)?;
    eval_checkdate_result(month, day, year, values)
}

/// Returns whether the supplied month/day/year tuple is a valid Gregorian date.
pub(in crate::interpreter) fn eval_checkdate_result(
    month: RuntimeCellHandle,
    day: RuntimeCellHandle,
    year: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let month = eval_int_value(month, values)?;
    let day = eval_int_value(day, values)?;
    let year = eval_int_value(year, values)?;
    values.bool_value(eval_checkdate_parts(month, day, year))
}

/// Tests PHP `checkdate()` bounds and leap-year behavior for integer components.
fn eval_checkdate_parts(month: i64, day: i64, year: i64) -> bool {
    if !(1..=12).contains(&month) || !(1..=32767).contains(&year) {
        return false;
    }
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if eval_is_leap_year(year) => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}

/// Returns whether one Gregorian year is a leap year.
fn eval_is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}
