//! Purpose:
//! Eval registry entry and implementation for `vsprintf`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - `vsprintf()` owns array-to-argument expansion for printf-family vector
//!   calls. `vprintf()` reuses that expansion from this file.

use crate::interpreter::builtins::collection_builder::{stage_collection_entry, with_collection_call};
use super::super::super::*;

eval_builtin! {
    contract: "vsprintf",
    area: Formatting,
    direct: Vsprintf,
    values: Vsprintf,
}

/// Evaluates direct positional `vsprintf()` calls in source order.
pub(in crate::interpreter) fn eval_builtin_vsprintf(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_vsprintf_result(&evaluated_args, context, values)
}

/// Formats `vsprintf()` array arguments and returns the resulting PHP string.
pub(in crate::interpreter) fn eval_vsprintf_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [format, array] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let format = values.string_bytes(*format)?;
    with_sprintf_array_args(*array, context, values, |args, context, values| {
        let output = super::sprintf::eval_sprintf_bytes(&format, args, context, values)?;
        values.string_bytes_value(&output)
    })
}

/// Lends the array's values in iteration order and retires them after formatting.
pub(in crate::interpreter) fn with_sprintf_array_args<V: RuntimeValueOps>(
    array: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut V,
    consume: impl FnOnce(&[RuntimeCellHandle], &mut ElephcEvalContext, &mut V)
        -> Result<RuntimeCellHandle, EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !values.is_array_like(array)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let len = values.array_len(array)?;
    with_collection_call(context, values, |context, values, owners, result| {
        let mut args = Vec::with_capacity(len);
        for position in 0..len {
            let (_, value) = stage_collection_entry(array, position, values, owners)?;
            args.push(value);
        }
        result.replace(Some(consume(&args, context, values)?));
        Ok(())
    })
}
