//! Purpose:
//! Implements eval support for PHP `sscanf()`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - The scan itself is `super::scanf_engine`, the same rule set the compiled backend runs as
//!   an injected elephc-PHP prelude, so `eval('sscanf(...)')` and a compiled `sscanf()` cannot
//!   answer differently. This file used to carry a `%d`/`%f`/`%s`/`%%` subset that pushed every
//!   match back as the matched STRING — `sscanf('77 xx', '%d %d')` gave `['77', '']` where php
//!   gives `[77, NULL]` — and knew no widths, suppression, character classes or EOF result.
//! - php's null result (end of input before any assignment) is returned as `null`, not as an
//!   empty array: `sscanf('', '%d')` is `NULL` while `sscanf('abc', '%d')` is `[NULL]`.
//! - A format php refuses raises the catchable `ValueError` it words itself, which is why the
//!   dispatchers thread `context` through — a pending throw needs somewhere to land.
//! - The by-ref `$vars` output form is REFUSED, matching the compiled builtin. php assigns
//!   each field through the reference and returns the field COUNT; this interpreter used to
//!   evaluate the extra arguments for side effects and IGNORE them, so the call silently
//!   returned the matched-fields array and assigned nothing. Refusing keeps `eval()` from
//!   being a silent-wrong workaround for the compiled path's refusal.
//! - This file owns registry metadata, direct dispatch, by-value dispatch, and the bridge from
//!   the engine's values to interpreter cells.

use super::super::super::*;
use super::scanf_engine::{scan, ScanfFormatError, ScanfValue};

eval_builtin! {
    contract: "sscanf",
    area: Formatting,
    direct: Sscanf,
    values: Sscanf,
}


/// Evaluates direct positional `sscanf()` calls in source order.
pub(in crate::interpreter) fn eval_builtin_sscanf(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() != 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let input = eval_expr(&args[0], context, scope, values)?;
    let format = eval_expr(&args[1], context, scope, values)?;
    eval_sscanf_result(input, format, context, values)
}


/// Dispatches by-value `sscanf()` calls after argument binding.
pub(in crate::interpreter) fn eval_sscanf_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    // An exact two-element binding, not `[input, format, ..]`: a bound `$vars` tail is the
    // unsupported by-ref output form, and swallowing it here returned the array in silence.
    let [input, format] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_sscanf_result(*input, *format, context, values)
}

/// Scans one string through php's scanf rules and returns php's own result shape.
pub(in crate::interpreter) fn eval_sscanf_result(
    input: RuntimeCellHandle,
    format: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let input = values.string_bytes(input)?;
    let format = values.string_bytes(format)?;
    let scanned = match scan(&input, &format) {
        Ok(scanned) => scanned,
        Err(error) => return eval_scanf_format_error(&error, context, values),
    };
    let Some(scanned) = scanned else {
        return values.null();
    };
    let mut result = values.array_new(scanned.len())?;
    for (index, scanned) in scanned.iter().enumerate() {
        let key = values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        let value = match scanned {
            ScanfValue::Int(value) => values.int(*value)?,
            ScanfValue::Float(value) => values.float(*value)?,
            ScanfValue::Bytes(value) => values.string_bytes_value(value)?,
            ScanfValue::Null => values.null()?,
        };
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Raises php's catchable `ValueError` for a format its scanner refuses.
fn eval_scanf_format_error<T>(
    error: &ScanfFormatError,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    let exception = values.new_object("ValueError")?;
    let message = values.string(&error.message)?;
    let code = values.int(0)?;
    values.construct_object(exception, vec![message, code])?;
    context.set_pending_throw(exception);
    Err(EvalStatus::UncaughtThrowable)
}
