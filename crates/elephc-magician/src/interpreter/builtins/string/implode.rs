//! Purpose:
//! Declarative eval registry entry for `implode`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Direct and evaluated-argument dispatch stay in this leaf.
//! - Both overloads share weak separator binding and the existing operand owner boundary.

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
    let operands = args.iter().collect::<Vec<_>>();
    with_eval_operands(&operands, context, scope, values, |args, context, _, values|
        eval_implode_declared_values_result(args, context, values))
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
    let string_type = EvalParameterType::new(vec![EvalParameterTypeVariant::String], false);
    let separator = eval_method_parameter_value(&string_type, separator.borrowed(), context, values)?;
    let mut separator_bytes = Vec::new();
    with_eval_value_lease(separator, context, values, |separator, _, values| {
        separator_bytes = values.string_bytes(separator)?;
        Ok(())
    })?;
    let separator = separator_bytes;
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
    match evaluated_args {
        [array] => {
            let separator = values.string("")?;
            let result = eval_implode_result(separator, *array, context, values);
            finish_eval_argument_values(result, [separator], context, values)
        }
        [separator, array] => eval_implode_result(*separator, *array, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
