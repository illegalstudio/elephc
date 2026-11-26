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
use crate::runtime_hooks::owner_frames::{
    RuntimeOwnerSlot, StagedRuntimeOwners, with_runtime_owners,
};
use elephc_builtin_contract::{ClassContract, ClassOperators, ObjectArithmetic, ObjectArithmeticResult, ObjectOperandDispatch};

/// Resolves the actual runtime class without retaining a transient class-name cell.
fn operator_class(
    values: &mut (impl RuntimeValueOps + ?Sized), value: RuntimeCellHandle,
) -> Result<Option<&'static ClassContract>, EvalStatus> {
    if values.type_tag(value)? != EVAL_TAG_OBJECT { return Ok(None); }
    let name = RuntimeOwnerSlot::new(values.object_class_name(value)?);
    let bytes = with_runtime_owners(&name, || {
        let bytes = values.string_bytes(name.borrowed());
        let released = values.release(name.take());
        bytes.and_then(|bytes| released.map(|()| bytes))
    })?;
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
    let owners = StagedRuntimeOwners::default();
    let result = RuntimeOwnerSlot::empty();
    with_runtime_owners(&(&owners, &result), || {
        let operation = (|| {
            let factor = owners.stage(values.int(if number { -1 } else { 0 })?);
            result.replace(Some(values.operator_static_call(
                class.name,
                method,
                vec![factor.borrowed(), value],
            )?));
            Ok(())
        })();
        owners
            .finish(operation, &result, |cell| values.release(cell))
            .map(Some)
    })
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
    let Some((class, method)) = binary_method(values, left, right,
        |ops| ops.arithmetic_method(operation))? else { return Ok(None); };
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
    let Some((class, method)) = binary_method(values, left, right,
        |ops| ops.bitwise_method(operation))? else { return Ok(None); };
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
    let Some((class, method)) = binary_method(values, left, right,
        |ops| ops.shift_method(operation))? else { return Ok(None); };
    values.operator_static_call(class.name, method, vec![left, right]).map(Some)
}

/// Falls through missing operations while trying both handlers before numeric conversion.
fn binary_method(
    values: &mut (impl RuntimeValueOps + ?Sized), left: RuntimeCellHandle, right: RuntimeCellHandle,
    method: impl Fn(&ClassOperators) -> Option<&'static str>,
) -> Result<Option<(&'static ClassContract, &'static str)>, EvalStatus> {
    let candidates = [operator_class(values, left)?, operator_class(values, right)?];
    for dispatch in [ObjectOperandDispatch::Handler, ObjectOperandDispatch::NumericConversion] {
        for class in candidates.into_iter().flatten() {
            if let Some(operators) = class.operators.filter(|ops| ops.operand_dispatch == dispatch) {
                if let Some(method) = method(operators) { return Ok(Some((class, method))); }
            }
        }
    }
    Ok(None)
}

/// Runs native bitwise handlers before numeric casts and retires independent cast cells.
pub(super) fn bitwise_result(
    values: &mut (impl RuntimeValueOps + ?Sized), op: EvalBinOp,
    left: RuntimeCellHandle, right: RuntimeCellHandle,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(result) = shift(values, op, left, right)? { return Ok(result); }
    if let Some(result) = bitwise(values, op, left, right)? { return Ok(result); }
    let owners = StagedRuntimeOwners::default();
    let result = RuntimeOwnerSlot::empty();
    with_runtime_owners(&(&owners, &result), || {
        let operation = (|| {
            let left = match numeric_cast(
                values,
                left,
                elephc_builtin_contract::ObjectCast::Int,
            )? {
                Some(value) => owners.stage(value).borrowed(),
                None => left,
            };
            let right = match numeric_cast(
                values,
                right,
                elephc_builtin_contract::ObjectCast::Int,
            )? {
                Some(value) => owners.stage(value).borrowed(),
                None => right,
            };
            result.replace(Some(values.bitwise(op, left, right)?));
            Ok(())
        })();
        owners.finish(operation, &result, |cell| values.release(cell))
    })
}

/// Applies PHP's internal-class `~$value` identity as `0 - value - 1`.
pub(super) fn bit_not(
    values: &mut (impl RuntimeValueOps + ?Sized), value: RuntimeCellHandle,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(class) = operator_class(values, value)? else { return Ok(None) };
    if let Some(operators) = class.operators {
        if let Some(method) = operators.bit_not {
            return values.operator_method_call(value, method, vec![]).map(Some);
        }
        if operators.operand_dispatch == ObjectOperandDispatch::NumericConversion {
            return Ok(None);
        }
    }
    let Some(method) = class
        .operators
        .and_then(|ops| ops.arithmetic_method(ObjectArithmetic::Sub))
    else {
        return Ok(None);
    };
    let owners = StagedRuntimeOwners::default();
    let result = RuntimeOwnerSlot::empty();
    with_runtime_owners(&(&owners, &result), || {
        let operation = (|| {
            let zero = owners.stage(values.int(0)?);
            let negative = owners.stage(values.operator_static_call(
                class.name,
                method,
                vec![zero.borrowed(), value],
            )?);
            let one = owners.stage(values.int(1)?);
            result.replace(Some(values.operator_static_call(
                class.name,
                method,
                vec![negative.borrowed(), one.borrowed()],
            )?));
            Ok(())
        })();
        owners
            .finish(operation, &result, |cell| values.release(cell))
            .map(Some)
    })
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
    let owners = StagedRuntimeOwners::default();
    with_runtime_owners(&owners, || {
        let result = owners.stage(values.operator_method_call(value, method, vec![])?);
        let truthy = values.truthy_raw(result.borrowed()).map(Some);
        owners.complete(truthy, |cell| values.release(cell))
    })
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
    let owners = StagedRuntimeOwners::default();
    let comparison = with_runtime_owners(&owners, || {
        let result = owners.stage(values.operator_static_call(
            class.name,
            method,
            vec![left, right],
        )?);
        let comparison = values.raw_value_word(result.borrowed()).map(|value| value as i64);
        owners.complete(comparison, |cell| values.release(cell))
    })?;
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
