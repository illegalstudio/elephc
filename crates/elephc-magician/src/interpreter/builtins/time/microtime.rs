//! Purpose:
//! Eval registry entry and implementation for `microtime`.
//!
//! Called from:
//! - `crate::interpreter::builtins::time` direct and by-value dispatch.
//!
//! Key details:
//! - The optional argument is accepted for PHP arity parity but does not alter the result.

use super::super::super::*;

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "microtime",
    area: Time,
    params: [as_float = EvalBuiltinDefaultValue::Bool(false)],
    direct: Time,
    values: Time,
}

/// Evaluates PHP `microtime()` with an optional ignored argument.
pub(in crate::interpreter) fn eval_builtin_microtime(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_microtime_string_result(values),
        [as_float] => {
            let flag = eval_expr(as_float, context, scope, values)?;
            if values.truthy(flag)? {
                eval_microtime_result(values)
            } else {
                eval_microtime_string_result(values)
            }
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

/// Returns `microtime()`'s default string form: the sub-second fraction, a space,
/// then the whole seconds.
///
/// php formats it as `"%.8F %ld"` over `tv_usec / 1e6` and `tv_sec`
/// (ext/standard/microtime.c `_php_math_microtime`), producing values such as
/// `"0.59125600 1784994272"`. Only a truthy argument selects the float form.
pub(in crate::interpreter) fn eval_microtime_string_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| EvalStatus::RuntimeFatal)?;
    let fraction = f64::from(timestamp.subsec_micros()) / 1_000_000.0;
    values.string(&format!("{:.8} {}", fraction, timestamp.as_secs()))
}
