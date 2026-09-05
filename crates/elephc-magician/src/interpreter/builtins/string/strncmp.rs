//! Purpose:
//! Eval registry entry and implementation for PHP `strncmp`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks` for direct and callable eval dispatch.
//!
//! Key details:
//! - Comparison is byte-oriented, stops at `$length`, and returns php-src's raw
//!   first-byte difference.
//! - A negative length schedules PHP's catchable `ValueError`.

use super::super::super::*;

eval_builtin! {
    contract: "strncmp",
    area: String,
    direct: StringCompare,
    values: StringCompare,
}

/// Evaluates PHP `strncmp(...)` arguments in source order.
pub(in crate::interpreter) fn eval_builtin_strncmp(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right, length] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    let length = eval_expr(length, context, scope, values)?;
    eval_strncmp_result(left, right, length, context, values)
}

/// Applies PHP `strncmp(...)` to already evaluated values.
pub(in crate::interpreter) fn eval_strncmp_result(
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    length: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_string_ncompare_named_result("strncmp", left, right, length, context, values)
}

/// Compares up to the requested number of bytes, optionally folding ASCII case.
pub(in crate::interpreter) fn eval_string_ncompare_named_result(
    name: &str,
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    length: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let length = eval_int_value(length, values)?;
    if length < 0 {
        let message =
            format!("{name}(): Argument #3 ($length) must be greater than or equal to 0");
        return eval_throw_builtin_value_error(&message, context, values);
    }
    let length = usize::try_from(length).map_err(|_| EvalStatus::RuntimeFatal)?;
    let mut left = values.string_bytes(left)?;
    let mut right = values.string_bytes(right)?;
    if name == "strncasecmp" {
        left.make_ascii_lowercase();
        right.make_ascii_lowercase();
    } else if name != "strncmp" {
        return Err(EvalStatus::UnsupportedConstruct);
    }
    let left_len = left.len().min(length);
    let right_len = right.len().min(length);
    values.int(super::strcmp::compare_byte_slices(
        &left[..left_len],
        &right[..right_len],
    ))
}
