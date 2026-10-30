//! Purpose:
//! Applies shared internal-class operator contracts to evaluated runtime cells.
//!
//! Called from:
//! - RuntimeValueOps arithmetic, comparison, boolean, cast, and clone entry points.
//! - EvalIR unary arithmetic for native internal-class operands.
//!
//! Key details:
//! - Native method bridges borrow operands and transfer an owned result cell.
//! - Strict identity bypasses dispatch; class-name and comparison temporaries are released.

use super::*;
use elephc_builtin_contract::{ClassContract, ObjectArithmetic, ObjectArithmeticResult};

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
    if let Some(class) = class.filter(|class| class.operators.is_some()) { return Ok(Some(class)); }
    for class in elephc_builtin_contract::classes().iter().filter(|class| class.operators.is_some()) {
        if values.object_is_a(value, class.name, false)? { return Ok(Some(class)); }
    }
    Ok(None)
}

/// Preserves numeric-result float zero signs without changing other native classes' unary adapter.
pub(in crate::interpreter) fn negate(
    values: &mut (impl RuntimeValueOps + ?Sized), value: RuntimeCellHandle,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(class) = operator_class(values, value)? else { return Ok(None); };
    let Some(operators) = class.operators else { return Ok(None); };
    let number = operators.arithmetic_result == ObjectArithmeticResult::Number;
    let operation = if number { ObjectArithmetic::Mul } else { ObjectArithmetic::Sub };
    let Some(method) = operators.arithmetic_method(operation) else { return Ok(None); };
    let factor = values.int(if number { -1 } else { 0 })?;
    let result = values.operator_static_call(class.name, method, vec![factor, value]);
    let released = values.release(factor);
    match (result, released) {
        (Err(status), _) => Err(status),
        (Ok(result), Err(status)) => { let _ = values.release(result); Err(status) },
        (Ok(result), Ok(())) => Ok(Some(result)),
    }
}

/// Runs a native clone handler and reports whether it replaces userland clone dispatch.
pub(in crate::interpreter) fn clone_native_storage(
    values: &mut (impl RuntimeValueOps + ?Sized), value: RuntimeCellHandle,
) -> Result<bool, EvalStatus> {
    for class in elephc_builtin_contract::classes() {
        let Some(method) = class.operators.and_then(|ops| ops.native_clone) else { continue; };
        match values.object_is_a(value, class.name, false) {
            Ok(true) => {
                let result = values.operator_method_call(value, method, vec![])?;
                return values.release(result).map(|()| true);
            }
            Ok(false) => (),
            // Embedders without native class capabilities retain ordinary user-object cloning.
            Err(EvalStatus::UnsupportedConstruct) => return Ok(false),
            Err(status) => return Err(status),
        }
    }
    Ok(false)
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

/// Calls a typed bitwise adapter with operands in their original source order.
pub(super) fn bitwise(
    values: &mut (impl RuntimeValueOps + ?Sized), op: EvalBinOp,
    left: RuntimeCellHandle, right: RuntimeCellHandle,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let operation = match op {
        EvalBinOp::BitAnd => elephc_builtin_contract::ObjectBitwise::And,
        EvalBinOp::BitOr => elephc_builtin_contract::ObjectBitwise::Or,
        EvalBinOp::BitXor => elephc_builtin_contract::ObjectBitwise::Xor,
        _ => return Ok(None),
    };
    let class = operator_class(values, left)?.or(operator_class(values, right)?);
    let Some(class) = class else { return Ok(None) };
    let Some(method) = class.operators.and_then(|ops| ops.bitwise_method(operation)) else { return Ok(None) };
    values.operator_static_call(class.name, method, vec![left, right]).map(Some)
}

/// Calls a typed shift adapter with operands in their original source order.
pub(super) fn shift(
    values: &mut (impl RuntimeValueOps + ?Sized), op: EvalBinOp,
    left: RuntimeCellHandle, right: RuntimeCellHandle,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let operation = match op {
        EvalBinOp::ShiftLeft => elephc_builtin_contract::ObjectShift::Left,
        EvalBinOp::ShiftRight => elephc_builtin_contract::ObjectShift::Right,
        _ => return Ok(None),
    };
    let class = operator_class(values, left)?.or(operator_class(values, right)?);
    let Some(class) = class else { return Ok(None) };
    let Some(method) = class.operators.and_then(|ops| ops.shift_method(operation)) else { return Ok(None) };
    values.operator_static_call(class.name, method, vec![left, right]).map(Some)
}

/// Applies PHP's internal-class `~$value` identity as `0 - value - 1`.
pub(super) fn bit_not(
    values: &mut (impl RuntimeValueOps + ?Sized), value: RuntimeCellHandle,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(class) = operator_class(values, value)? else { return Ok(None) };
    let Some(method) = class
        .operators
        .and_then(|ops| ops.arithmetic_method(ObjectArithmetic::Sub))
    else {
        return Ok(None);
    };
    let zero = values.int(0)?;
    let negative = values.operator_static_call(class.name, method, vec![zero, value]);
    values.release(zero)?;
    let negative = negative?;
    let one = values.int(1)?;
    let result = values.operator_static_call(class.name, method, vec![negative, one]);
    values.release(negative)?;
    values.release(one)?;
    result.map(Some)
}

/// Calls an internal value class's `__elephc_to_number` adapter when one exists.
pub(super) fn number_cast(
    values: &mut (impl RuntimeValueOps + ?Sized), value: RuntimeCellHandle,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(class) = operator_class(values, value)? else { return Ok(None) };
    let Some(method) = class
        .operators
        .and_then(|ops| ops.casts)
        .and_then(|casts| casts.to_number)
    else {
        return Ok(None);
    };
    values.operator_method_call(value, method, vec![]).map(Some)
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

/// Applies an internal numeric conversion through the same instance adapter as AOT.
pub(in crate::interpreter) fn numeric_cast(
    values: &mut (impl RuntimeValueOps + ?Sized), value: RuntimeCellHandle,
    cast: elephc_builtin_contract::ObjectCast,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(class) = operator_class(values, value)? else { return Ok(None); };
    let Some(method) = class.operators.and_then(|ops| ops.cast_method(cast)) else { return Ok(None); };
    values.operator_method_call(value, method, vec![]).map(Some)
}

/// Selects an internal JSON adapter without invoking it before the encoder's recursion guard.
pub(in crate::interpreter) fn json_method(
    values: &mut (impl RuntimeValueOps + ?Sized), value: RuntimeCellHandle,
) -> Result<Option<&'static str>, EvalStatus> {
    let class = match operator_class(values, value) {
        Ok(class) => class,
        // Embedders without native class capabilities keep ordinary property encoding.
        Err(EvalStatus::UnsupportedConstruct) => return Ok(None),
        Err(status) => return Err(status),
    };
    Ok(class.and_then(|class| class.operators).and_then(|operators| operators.json))
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
    if left_tag == EVAL_TAG_NULL || right_tag == EVAL_TAG_NULL {
        // PHP's standard comparison ranks every object above null; the internal class
        // handler is only consulted once both operands are comparable values.
        let ordering = match (left_tag, right_tag) {
            (EVAL_TAG_NULL, EVAL_TAG_NULL) => 0,
            (EVAL_TAG_NULL, _) => -1,
            (_, EVAL_TAG_NULL) => 1,
            _ => unreachable!("null branch requires one null operand"),
        };
        return materialize_comparison(values, op, ordering).map(Some);
    }
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
