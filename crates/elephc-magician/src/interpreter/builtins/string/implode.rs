//! Purpose:
//! Declarative eval registry entry for `implode`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Direct and evaluated-argument dispatch stay in this leaf.
//! - The current eval implementation supports the two-argument runtime form.

eval_builtin! {
    contract: "implode",
    area: String,
    direct: StringSplitJoin,
    values: StringSplitJoin,
}

use super::super::super::*;

/// Evaluates PHP `implode()` over separator and array expressions.
pub(in crate::interpreter) fn eval_builtin_implode(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [separator, array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let separator = eval_expr(separator, context, scope, values)?;
    let array = eval_expr(array, context, scope, values)?;
    eval_implode_result(separator, array, context, values)
}

/// Joins array values in eval iteration order using PHP string conversion.
pub(in crate::interpreter) fn eval_implode_result(
    separator: RuntimeCellHandle,
    array: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !values.is_array_like(array)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let separator = values.string_bytes(separator)?;
    let len = values.array_len(array)?;
    let mut output = Vec::new();
    for position in 0..len {
        if position > 0 {
            output.extend_from_slice(&separator);
        }
        let key = values.array_iter_key(array, position)?;
        with_eval_value_lease(key, context, values, |key, context, values| {
            let value = values.array_get(array, key)?;
            with_eval_value_lease(value, context, values, |value, _, values| {
                output.extend_from_slice(&values.string_bytes(value)?);
                Ok(())
            })
        })?;
    }
    values.string_bytes_value(&output)
}

/// Dispatches evaluated `implode()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_implode_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [separator, array] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_implode_result(*separator, *array, context, values)
}
