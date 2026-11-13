//! Purpose:
//! Applies checked user arithmetic independently of PHP implementation arithmetic.
//!
//! Called from:
//! - Shared walker/VM binary and unary evaluation and public numeric builtin dispatch.
//!
//! Key details:
//! - Native classes retain their reviewed object operator contracts.
//! - Hooks borrow operands; their result and pending Throwable each transfer one owner.

use super::*;
use elephc_semantics::{numeric::CheckedNumericOp, StrictFeature};

/// Preserves native overloads, validates numeric domains and selects explicit checked kernels.
pub(super) fn binary(
    op: EvalBinOp, left: RuntimeCellHandle, right: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if elephc_semantics::arithmetic_policy::ArithmeticPolicy::from_profile(context.semantic_profile()).is_permissive() { return Ok(None); }
    if let Some(value) = super::numeric_domains::bitwise(op, left, right, context, values)? { return Ok(Some(value)); }
    let (checked, object) = match op {
        EvalBinOp::Add => (CheckedNumericOp::Add, elephc_builtin_contract::ObjectArithmetic::Add),
        EvalBinOp::Sub => (CheckedNumericOp::Sub, elephc_builtin_contract::ObjectArithmetic::Sub),
        EvalBinOp::Mul => (CheckedNumericOp::Mul, elephc_builtin_contract::ObjectArithmetic::Mul),
        EvalBinOp::Mod => (CheckedNumericOp::Mod, elephc_builtin_contract::ObjectArithmetic::Mod),
        EvalBinOp::Div => (CheckedNumericOp::Div, elephc_builtin_contract::ObjectArithmetic::Div),
        EvalBinOp::Pow => (CheckedNumericOp::Pow, elephc_builtin_contract::ObjectArithmetic::Pow),
        _ => return Ok(None),
    };
    if let Some(result) = runtime_ops::object_operators::arithmetic(values, object, left, right)? {
        return Ok(Some(result));
    }
    super::numeric_domains::require_binary(op, left, right, context, values)?;
    if !context.semantic_profile().contains(StrictFeature::Overflow) {
        return super::numeric_domains::non_overflow_operation(op, left, right, context, values);
    }
    apply(checked, left, right, context, values).map(Some)
}

/// Transfers native callback failures into the same catch state used by interpreted exceptions.
pub(super) fn apply(
    op: CheckedNumericOp, left: RuntimeCellHandle, right: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match values.checked_numeric(op, left, right) {
        Err(EvalStatus::UncaughtThrowable) => {
            if let Some(thrown) = values.take_pending_runtime_throwable()? {
                context.set_pending_throw(thrown);
            }
            Err(EvalStatus::UncaughtThrowable)
        }
        result => result,
    }
}


/// Checks a scalar update before writeback and preserves internal-class arithmetic adapters.
pub(super) fn update(
    current: RuntimeCellHandle, increment: bool,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(current)? == EVAL_TAG_OBJECT {
        let one = values.int(1)?;
        let result = binary(if increment { EvalBinOp::Add } else { EvalBinOp::Sub }, current, one, context, values)
            .and_then(|value| value.ok_or(EvalStatus::UnsupportedConstruct));
        return finish_synthesized_method_arguments(result, [one], context, values);
    }
    let op = if increment { CheckedNumericOp::Increment } else { CheckedNumericOp::Decrement };
    apply(op, current, current, context, values)
}
