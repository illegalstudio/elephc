//! Purpose:
//! Eval registry entry and implementation for `microtime`.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` direct and by-value dispatch.
//!
//! Key details:
//! - The optional argument selects PHP's string or floating-point result mode.

use super::super::super::*;

eval_builtin! {
    contract: "microtime",
    area: Time,
    direct: Time,
    values: Time,
}

/// Evaluates PHP `microtime()` with PHP's optional floating-point result mode.
pub(in crate::interpreter) fn eval_builtin_microtime(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_microtime_result(None, values),
        [as_float] => {
            let as_float = eval_expr(as_float, context, scope, values)?;
            eval_microtime_result(Some(as_float), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Returns the current Unix timestamp in PHP's string or floating-point representation.
pub(in crate::interpreter) fn eval_microtime_result(
    as_float: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| EvalStatus::RuntimeFatal)?;
    let micros = f64::from(timestamp.subsec_micros()) / 1_000_000.0;
    let as_float = match as_float {
        Some(value) => values.truthy(value)?,
        None => false,
    };
    if as_float {
        return values.float(timestamp.as_secs() as f64 + micros);
    }
    values.string_bytes_value(
        format!("{micros:.8} {}", timestamp.as_secs()).as_bytes(),
    )
}
