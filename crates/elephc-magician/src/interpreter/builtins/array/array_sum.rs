//! Purpose:
//! Declarative eval registry entry for `array_sum`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the array-aggregate hook.

use super::super::super::*;

eval_builtin! {
    contract: "array_sum",
    area: Array,
    direct: ArrayAggregate,
    values: ArrayAggregate,
}
/// Dispatches direct eval calls for the `array_sum` array builtin.
pub(in crate::interpreter) fn eval_array_sum_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_aggregate("array_sum", args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_sum` array builtin.
pub(in crate::interpreter) fn eval_array_sum_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    eval_array_aggregate_result("array_sum", *array, context, values)
}

/// Evaluates PHP array aggregate builtins over one eval array expression.
pub(in crate::interpreter) fn eval_builtin_array_aggregate(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    with_eval_operands(&[array], context, scope, values, |args, context, _, values| {
        eval_array_aggregate_result(name, args[0], context, values)
    })
}

/// Computes `array_sum()` or `array_product()` through eval's numeric value hooks.
pub(in crate::interpreter) fn eval_array_aggregate_result(
    name: &str,
    array: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if elephc_semantics::arithmetic_policy::ArithmeticPolicy::from_profile(context.semantic_profile()).has_numeric_domain() {
        return super::numeric_domain_aggregate::evaluate(name, array, context, values);
    }
    if context.semantic_profile().contains(elephc_semantics::StrictFeature::Overflow) {
        use elephc_semantics::numeric::CheckedNumericOp;
        let op = match name {
            "array_sum" => CheckedNumericOp::ArraySum,
            "array_product" => CheckedNumericOp::ArrayProduct,
            _ => return Err(EvalStatus::UnsupportedConstruct),
        };
        return crate::interpreter::checked_numeric::apply(op, array, array, context, values);
    }
    let len = values.array_len(array)?;
    let mut result = match name {
        "array_sum" => values.int(0)?,
        "array_product" => values.int(1)?,
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        // php-src's numeric aggregate uses `_IS_NUMBER` on internal value objects: GMP
        // contributes `int` when it fits and `float` otherwise, not a GMP arithmetic result.
        let value = values.number_value(value)?;
        result = match name {
            "array_sum" => values.add(result, value)?,
            "array_product" => values.mul(result, value)?,
            _ => return Err(EvalStatus::UnsupportedConstruct),
        };
    }
    Ok(result)
}
