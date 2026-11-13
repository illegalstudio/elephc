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

/// Selects a checked scalar entry only for user-visible operations owned by Overflow.
pub(super) fn binary(
    op: EvalBinOp, left: RuntimeCellHandle, right: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !context.semantic_profile().contains(StrictFeature::Overflow) { return Ok(None); }
    let (checked, object) = match op {
        EvalBinOp::Add => (CheckedNumericOp::Add, elephc_builtin_contract::ObjectArithmetic::Add),
        EvalBinOp::Sub => (CheckedNumericOp::Sub, elephc_builtin_contract::ObjectArithmetic::Sub),
        EvalBinOp::Mul => (CheckedNumericOp::Mul, elephc_builtin_contract::ObjectArithmetic::Mul),
        EvalBinOp::Pow => (CheckedNumericOp::Pow, elephc_builtin_contract::ObjectArithmetic::Pow),
        _ => return Ok(None),
    };
    if let Some(result) = runtime_ops::object_operators::arithmetic(values, object, left, right)? {
        return Ok(Some(result));
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
