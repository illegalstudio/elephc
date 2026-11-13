//! Purpose:
//! Applies shared numeric operand and integer-power contracts before eval runtime coercion.
//!
//! Called from:
//! - Shared walker/VM binary evaluation through checked_numeric.
//!
//! Key details:
//! - Native class operators run before the scalar contract is selected.
//! - Concrete scalar payload reads avoid allocating or invoking implicit casts.

use super::*;
use elephc_semantics::{conversions::ValueKind, numeric_domains::{self as policy, NumericOperator, IntegerBitOp}};

/// Maps existing eval operators into the same language signatures used by the compiler.
fn operator(op: EvalBinOp) -> Option<NumericOperator> {
    Some(match op {
        EvalBinOp::Add | EvalBinOp::Sub | EvalBinOp::Mul => NumericOperator::Arithmetic,
        EvalBinOp::Div => NumericOperator::Division, EvalBinOp::Mod => NumericOperator::Modulo,
        EvalBinOp::Pow => NumericOperator::Power,
        EvalBinOp::BitAnd | EvalBinOp::BitOr | EvalBinOp::BitXor => NumericOperator::Bitwise,
        EvalBinOp::ShiftLeft | EvalBinOp::ShiftRight => NumericOperator::Shift,
        _ => return None,
    })
}

/// Maps canonical runtime tags without interpreting strings or confusing booleans with integers.
fn value_kind(tag: u64) -> ValueKind {
    match tag {
        EVAL_TAG_INT => ValueKind::Integer, EVAL_TAG_FLOAT => ValueKind::Float,
        EVAL_TAG_BOOL => ValueKind::Boolean, EVAL_TAG_STRING => ValueKind::String,
        EVAL_TAG_NULL => ValueKind::Null, _ => ValueKind::NonScalar,
    }
}

/// Guards a unary or update value after its explicit native-class adapter was excluded.
pub(super) fn require_unary(operator: NumericOperator, value: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps) -> Result<(), EvalStatus> {
    let Some(rule) = policy::operand_rule(context.semantic_profile(), operator) else { return Ok(()); };
    if rule.domain.allows(value_kind(values.type_tag(value)?)) { return Ok(()); }
    throwables::eval_throw_type_error(&rule.message(), context, values)
}

/// Rejects kind and integer-power violations without changing either borrowed operand.
pub(super) fn require_binary(
    op: EvalBinOp, left: RuntimeCellHandle, right: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(operator) = operator(op) else { return Ok(()); };
    let Some(rule) = policy::operand_rule(context.semantic_profile(), operator) else { return Ok(()); };
    let (left_kind, right_kind) = (value_kind(values.type_tag(left)?), value_kind(values.type_tag(right)?));
    if !rule.domain.allows(left_kind) || !rule.domain.allows(right_kind) {
        return throwables::eval_throw_type_error(&rule.message(), context, values);
    }
    if op == EvalBinOp::Pow && left_kind == ValueKind::Integer && right_kind == ValueKind::Integer
        && policy::rejects_negative_integer_power(context.semantic_profile(), left_kind, right_kind, values.raw_value_word(right)? as i64) {
        return throwables::eval_throw_builtin_exception("ArithmeticError", policy::NEGATIVE_INTEGER_POWER_MESSAGE, context, values);
    }
    Ok(())
}

/// Evaluates native integer bit operations after any explicit native-class overload.
pub(super) fn bitwise(
    op: EvalBinOp, left: RuntimeCellHandle, right: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let operation = match op {
        EvalBinOp::BitAnd => IntegerBitOp::And, EvalBinOp::BitOr => IntegerBitOp::Or,
        EvalBinOp::BitXor => IntegerBitOp::Xor, EvalBinOp::ShiftLeft => IntegerBitOp::ShiftLeft,
        EvalBinOp::ShiftRight => IntegerBitOp::ShiftRight, _ => return Ok(None),
    };
    let profile = context.semantic_profile();
    if policy::operand_rule(profile, operator(op).unwrap()).is_some() {
        if let Some(value) = runtime_ops::object_operators::shift(values, op, left, right)? { return Ok(Some(value)); }
        if let Some(value) = runtime_ops::object_operators::bitwise(values, op, left, right)? { return Ok(Some(value)); }
        require_binary(op, left, right, context, values)?;
    } else if values.type_tag(left)? != EVAL_TAG_INT || values.type_tag(right)? != EVAL_TAG_INT {
        return Ok(None);
    }
    let (left, right) = (values.raw_value_word(left)? as i64, values.raw_value_word(right)? as i64);
    match policy::integer_bitwise(operation, left, right) {
        Ok(value) => values.int(value).map(Some),
        Err(message) => throwables::eval_throw_builtin_exception("ArithmeticError", message, context, values),
    }
}

/// Preserves exact result kinds and catchable value failures after noncoercive scalar guards.
pub(super) fn non_overflow_operation(
    op: EvalBinOp, left: RuntimeCellHandle, right: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if op == EvalBinOp::Pow && values.type_tag(left)? == EVAL_TAG_INT && values.type_tag(right)? == EVAL_TAG_INT {
        use elephc_semantics::integer_power::{php_integer_power, PowerResult};
        return match php_integer_power(values.raw_value_word(left)? as i64, values.raw_value_word(right)? as i64) {
            PowerResult::Int(value) => values.int(value).map(Some),
            PowerResult::Float(value) => values.float(value).map(Some),
        };
    }
    if !matches!(op, EvalBinOp::Div | EvalBinOp::Mod) { return Ok(None); }
    let tag = values.type_tag(right)?;
    let word = values.raw_value_word(right)?;
    let zero = if tag == EVAL_TAG_FLOAT { f64::from_bits(word) == 0.0 } else { word == 0 };
    if zero {
        return throwables::eval_throw_builtin_division_by_zero_error(
            if op == EvalBinOp::Div { "Division by zero" } else { "Modulo by zero" }, context, values);
    }
    if op == EvalBinOp::Mod && word as i64 == -1 { return values.int(0).map(Some); }
    Ok(None)
}
