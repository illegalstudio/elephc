//! Purpose:
//! Eval registry entry and implementation for `set_time_limit`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env`.
//!
//! Key details:
//! - php returns `true` for every argument probed on php 8.5.10 CLI (`0`, `30`, `-1`,
//!   `PHP_INT_MAX`, `true`, `"5"`, `null`); no input was found that returns `false`, so the
//!   constant answer is exact.
//! - `$seconds` is still COERCED, because the coercion is observable: `set_time_limit("x")` is a
//!   catchable `TypeError` naming the parameter, and `set_time_limit([])` likewise. Skipping the
//!   coercion would turn both into a silent success.
//! - `$seconds <= 0` means "no limit" in php and an elephc program has no limit either, so those
//!   calls are FULLY faithful -- including Symfony's `set_time_limit(0)`. A POSITIVE `$seconds`
//!   is a named divergence: php CLI really arms the timer and aborts with
//!   `Fatal error: Maximum execution time of N second(s) exceeded`, and elephc has no execution
//!   -time interrupt to arm. See `src/builtins/system/set_time_limit.rs` for the measurement.
//! - The other named divergence is that php makes the new value visible through
//!   `ini_get('max_execution_time')`; elephc's `ini_get` reads a compile-time table that has no
//!   `max_execution_time` entry and no runtime store behind it.

use super::*;

eval_builtin! {
    contract: "set_time_limit",
    area: NetworkEnv,
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates PHP `set_time_limit(...)` from unevaluated call-site expressions.
pub(in crate::interpreter) fn eval_builtin_set_time_limit(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() != 1 {
        return eval_throw_argument_count_error(
            &format!(
                "set_time_limit() expects exactly 1 argument, {} given",
                args.len()
            ),
            context,
            values,
        );
    }
    let seconds = eval_expr(&args[0], context, scope, values)?;
    eval_set_time_limit_result(seconds, context, values)
}

/// Evaluates PHP `set_time_limit(...)` from already-evaluated argument cells (the named/spread
/// and dynamic-call route).
pub(in crate::interpreter) fn eval_set_time_limit_values(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [seconds] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_set_time_limit_result(*seconds, context, values)
}

/// Coerces `$seconds` at php's weak-typing boundary and answers the `true` php always answers.
pub(in crate::interpreter) fn eval_set_time_limit_result(
    seconds: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    // The coerced value is deliberately discarded: there is no timer to arm. The coercion is
    // kept because its REFUSAL is observable as a catchable TypeError.
    let _seconds = eval_require_int_arg(seconds, "set_time_limit", 1, "seconds", context, values)?;
    values.bool_value(true)
}
