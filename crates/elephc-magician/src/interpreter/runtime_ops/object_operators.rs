//! Purpose:
//! Applies shared internal-class operator contracts to evaluated runtime cells.
//!
//! Called from:
//! - RuntimeValueOps arithmetic, comparison, boolean, and cast entry points.
//!
//! Key details:
//! - Native method bridges borrow operands and transfer an owned result cell.
//! - Strict identity bypasses dispatch; class-name and comparison temporaries are released.

use super::*;
use elephc_builtin_contract::{ClassContract, ObjectArithmetic};

/// Resolves the actual runtime class without retaining a transient class-name cell.
fn operator_class(
    values: &mut (impl RuntimeValueOps + ?Sized), value: RuntimeCellHandle,
) -> Result<Option<&'static ClassContract>, EvalStatus> {
    if values.type_tag(value)? != EVAL_TAG_OBJECT { return Ok(None); }
    let name = values.object_class_name(value)?;
    let bytes = values.string_bytes(name);
    let released = values.release(name);
    let bytes = bytes.and_then(|bytes| released.map(|()| bytes))?;
    let class = std::str::from_utf8(&bytes).ok().and_then(elephc_builtin_contract::lookup_class);
    Ok(class.filter(|class| class.operators.is_some()))
}

/// Calls a typed arithmetic adapter with operands in their original source order.
pub(super) fn arithmetic(
    values: &mut (impl RuntimeValueOps + ?Sized), operation: ObjectArithmetic,
    left: RuntimeCellHandle, right: RuntimeCellHandle,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let class = operator_class(values, left)?.or(operator_class(values, right)?);
    let Some(class) = class else { return Ok(None) };
    let Some(method) = class.operators.and_then(|ops| ops.arithmetic_method(operation)) else { return Ok(None) };
    values.operator_static_call(class.name, method, vec![left, right]).map(Some)
}

/// Computes an internal value class's boolean value through its reviewed instance adapter.
pub(super) fn truthy(
    values: &mut (impl RuntimeValueOps + ?Sized), value: RuntimeCellHandle,
) -> Result<Option<bool>, EvalStatus> {
    let Some(class) = operator_class(values, value)? else { return Ok(None) };
    let Some(method) = class.operators.and_then(|ops| ops.boolean) else { return Ok(None) };
    let result = values.operator_method_call(value, method, vec![])?;
    let truthy = values.truthy_raw(result);
    let released = values.release(result);
    truthy.and_then(|truthy| released.map(|()| Some(truthy)))
}

/// Compares values and materializes either a signed integer or a PHP boolean result.
pub(super) fn comparison(
    values: &mut (impl RuntimeValueOps + ?Sized), op: EvalBinOp,
    left: RuntimeCellHandle, right: RuntimeCellHandle,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let class = operator_class(values, left)?.or(operator_class(values, right)?);
    let Some(class) = class else { return Ok(None) };
    let left_tag = values.type_tag(left)?;
    let right_tag = values.type_tag(right)?;
    if left_tag == EVAL_TAG_BOOL || right_tag == EVAL_TAG_BOOL {
        let lhs = i64::from(values.truthy(left)?);
        let rhs = i64::from(values.truthy(right)?);
        return materialize_comparison(values, op, lhs - rhs).map(Some);
    }
    let Some(method) = class.operators.and_then(|ops| ops.compare) else { return Ok(None) };
    let result = values.operator_static_call(class.name, method, vec![left, right])?;
    let comparison = values.raw_value_word(result).map(|value| value as i64);
    let released = values.release(result);
    let comparison = comparison.and_then(|value| released.map(|()| value))?;
    materialize_comparison(values, op, comparison).map(Some)
}

/// Converts a signed comparison to the requested PHP operator's scalar result.
fn materialize_comparison(
    values: &mut (impl RuntimeValueOps + ?Sized), op: EvalBinOp, comparison: i64,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if matches!(op, EvalBinOp::Spaceship) {
        let unordered = elephc_builtin_contract::ObjectOrdering::Unordered as i64;
        return values.int(if comparison == unordered { 1 } else { comparison });
    }
    let result = match op {
        EvalBinOp::LooseEq => comparison == 0,
        EvalBinOp::LooseNotEq => comparison != 0,
        EvalBinOp::Lt => comparison == -1,
        EvalBinOp::LtEq => comparison == -1 || comparison == 0,
        EvalBinOp::Gt => comparison == 1,
        EvalBinOp::GtEq => comparison == 1 || comparison == 0,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(result)
}
