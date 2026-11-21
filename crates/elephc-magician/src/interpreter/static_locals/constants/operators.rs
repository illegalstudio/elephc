//! Purpose:
//! Proves constant operator domains before calling shared scalar or runtime helpers.
//!
//! Called from:
//! - The bounded static initializer reducer used by Reflection.
//!
//! Key details:
//! - No arbitrary objects, PHP conversions, warnings or exception probes execute here.
//! - Arithmetic and kind rules reuse the same neutral contracts as normal execution.

use super::*;
use elephc_semantics::{StrictFeature, numeric::IntegerOp, numeric_unary::{NumericValue, NumericUnaryOp},
    numeric_domains::{self as domains, NumericOperator, IntegerBitOp}, scalar_compare::{php_numeric_string, PhpNumeric}};

impl<V: RuntimeValueOps> Reducer<'_, V> {
    /// Reads a scalar number only when PHP arithmetic would not warn about a numeric prefix.
    fn number(&mut self, value: RuntimeCellHandle) -> Result<Option<NumericValue>, EvalStatus> {
        Ok(Some(match self.values.type_tag(value)? {
            EVAL_TAG_INT => NumericValue::Int(self.values.raw_value_word(value)? as i64),
            EVAL_TAG_FLOAT => NumericValue::Float(f64::from_bits(self.values.raw_value_word(value)?)),
            EVAL_TAG_BOOL => NumericValue::Int(i64::from(self.values.raw_value_word(value)? != 0)),
            EVAL_TAG_NULL => NumericValue::Int(0),
            EVAL_TAG_STRING => {
                let bytes = self.values.string_bytes(value)?;
                let Ok(text) = std::str::from_utf8(&bytes) else { return Ok(None); };
                match php_numeric_string(text) {
                    Some(PhpNumeric::Int(value)) => NumericValue::Int(value),
                    Some(PhpNumeric::Float { value, .. }) => NumericValue::Float(value),
                    None => return Ok(None),
                }
            }
            _ => return Ok(None),
        }))
    }

    /// Keeps integer-only implicit conversions silent by requiring exact finite float values.
    fn integer(&mut self, value: RuntimeCellHandle) -> Result<Option<i64>, EvalStatus> {
        Ok(match self.number(value)? {
            Some(NumericValue::Int(value)) => Some(value),
            Some(NumericValue::Float(value)) => elephc_semantics::numeric::checked_float_to_int(value).ok()
                .filter(|integer| *integer as f64 == value),
            None => None,
        })
    }

    /// Checks semantic kind rules without throwing the diagnostic reserved for the real call.
    fn domain(&mut self, operator: NumericOperator, value: RuntimeCellHandle) -> Result<bool, EvalStatus> {
        let Some(rule) = domains::operand_rule(self.context.semantic_profile(), operator) else { return Ok(true); };
        Ok(rule.domain.allows(numeric_domains::value_kind(self.values.type_tag(value)?)))
    }

    /// Rejects NaN boolean conversion before it can notify an installed PHP error handler.
    fn silent_truthy(&mut self, value: RuntimeCellHandle) -> Result<Option<bool>, EvalStatus> {
        if self.values.type_tag(value)? == EVAL_TAG_FLOAT && f64::from_bits(self.values.raw_value_word(value)?).is_nan() { return Ok(None); }
        self.values.truthy(value).map(Some)
    }

    /// Applies strict condition membership independently of explicit bool casts.
    pub(super) fn condition(&mut self, value: RuntimeCellHandle) -> Result<Option<bool>, EvalStatus> {
        if self.context.semantic_profile().contains(StrictFeature::Bools) && self.values.type_tag(value)? != EVAL_TAG_BOOL { return Ok(None); }
        if !matches!(self.values.type_tag(value)?, EVAL_TAG_NULL | EVAL_TAG_BOOL | EVAL_TAG_INT | EVAL_TAG_FLOAT | EVAL_TAG_STRING | EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) { return Ok(None); }
        self.silent_truthy(value)
    }

    /// Boxes a neutral numeric result without running runtime coercion hooks.
    fn numeric_result(&mut self, value: NumericValue) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        self.make(|_, values| match value { NumericValue::Int(value) => values.int(value), NumericValue::Float(value) => values.float(value) })
    }

    /// Applies pure unary contracts after proving the operand cannot run a PHP hook.
    pub(super) fn unary(&mut self, op: EvalUnaryOp, value: RuntimeCellHandle) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        if op == EvalUnaryOp::LogicalNot {
            let Some(truthy) = self.condition(value)? else { return Ok(None); };
            return self.make(|_, values| values.bool_value(!truthy));
        }
        if op == EvalUnaryOp::BitNot {
            if !self.domain(NumericOperator::BitNot, value)? { return Ok(None); }
            match self.values.type_tag(value)? {
                EVAL_TAG_STRING | EVAL_TAG_INT => return self.make(|_, values| values.bit_not(value)),
                EVAL_TAG_FLOAT if self.integer(value)?.is_some() => return self.make(|_, values| values.bit_not(value)),
                _ => return Ok(None),
            }
        }
        if !self.domain(NumericOperator::Unary, value)? { return Ok(None); }
        let Some(value) = self.number(value)? else { return Ok(None); };
        let op = if op == EvalUnaryOp::Plus { NumericUnaryOp::Plus } else { NumericUnaryOp::Negate };
        match elephc_semantics::numeric_unary::apply(op, value, self.context.semantic_profile().contains(StrictFeature::Overflow)) {
            Ok(value) => self.numeric_result(value), Err(_) => Ok(None),
        }
    }

    /// Uses existing primitive cast implementations only after excluding diagnostics and objects.
    pub(super) fn cast(&mut self, target: EvalCastType, value: RuntimeCellHandle) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        let tag = self.values.type_tag(value)?;
        if !matches!(tag, EVAL_TAG_NULL | EVAL_TAG_BOOL | EVAL_TAG_INT | EVAL_TAG_FLOAT | EVAL_TAG_STRING | EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) { return Ok(None); }
        if tag == EVAL_TAG_STRING && matches!(target, EvalCastType::String | EvalCastType::StringFormat) { return Ok(Some(value)); }
        if target == EvalCastType::Int && tag == EVAL_TAG_FLOAT {
            let value = f64::from_bits(self.values.raw_value_word(value)?);
            if elephc_semantics::numeric::checked_float_to_int(value).is_err() { return Ok(None); }
        }
        if target == EvalCastType::Bool && self.silent_truthy(value)?.is_none() { return Ok(None); }
        if matches!(target, EvalCastType::String | EvalCastType::StringFormat) && matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) { return Ok(None); }
        if self.context.semantic_profile().contains(StrictFeature::Casts) && matches!(target, EvalCastType::Int | EvalCastType::Float) {
            use elephc_semantics::numeric_cast::{NumericCastInput as Input, NumericCastTarget as Target, checked_numeric_cast};
            let bytes = if tag == EVAL_TAG_STRING { self.values.string_bytes(value)? } else { Vec::new() };
            let input = match tag {
                EVAL_TAG_NULL => Input::Null, EVAL_TAG_BOOL => Input::Bool(self.values.raw_value_word(value)? != 0),
                EVAL_TAG_INT => Input::Int(self.values.raw_value_word(value)? as i64),
                EVAL_TAG_FLOAT => Input::Float(f64::from_bits(self.values.raw_value_word(value)?)),
                EVAL_TAG_STRING => { let Ok(text) = std::str::from_utf8(&bytes) else { return Ok(None); }; Input::String(text) }
                _ => Input::Unsupported,
            };
            let target = if target == EvalCastType::Int { Target::Int } else { Target::Float };
            return match checked_numeric_cast(target, input) { Ok(value) => self.numeric_result(value), Err(_) => Ok(None) };
        }
        self.make(|_, values| match target {
            EvalCastType::Int => values.cast_int(value), EvalCastType::Float => values.cast_float(value),
            EvalCastType::Bool => values.cast_bool(value), EvalCastType::Array => values.cast_array(value),
            EvalCastType::String | EvalCastType::StringFormat => values.cast_string(value),
        })
    }

    /// Dispatches only proven primitive operations; fallible arithmetic remains deferred.
    pub(super) fn binary(&mut self, op: EvalBinOp, left: RuntimeCellHandle, right: RuntimeCellHandle)
        -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        let (left_tag, right_tag) = (self.values.type_tag(left)?, self.values.type_tag(right)?);
        let scalar = |tag| matches!(tag, EVAL_TAG_NULL | EVAL_TAG_BOOL | EVAL_TAG_INT | EVAL_TAG_FLOAT | EVAL_TAG_STRING);
        if !scalar(left_tag) || !scalar(right_tag) {
            if self.plain_data(left, 0)? && self.plain_data(right, 0)? {
                if op == EvalBinOp::Add && matches!(left_tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) && matches!(right_tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC)
                    && domains::operand_rule(self.context.semantic_profile(), NumericOperator::Arithmetic).is_none() {
                    return self.array_union(left, right);
                }
                if matches!(op, EvalBinOp::StrictEq | EvalBinOp::StrictNotEq) || !self.context.semantic_profile().contains(StrictFeature::Comparisons)
                    && matches!(op, EvalBinOp::LooseEq | EvalBinOp::LooseNotEq) {
                    let strict = matches!(op, EvalBinOp::StrictEq | EvalBinOp::StrictNotEq);
                    let Some(equal) = self.array_equal(left, right, strict, 0)? else { return Ok(None); };
                    let equal = equal ^ matches!(op, EvalBinOp::LooseNotEq | EvalBinOp::StrictNotEq);
                    return self.make(|_, values| values.bool_value(equal));
                }
            }
            return Ok(None);
        }
        if op == EvalBinOp::Concat {
            if self.context.semantic_profile().contains(StrictFeature::Strings) && (left_tag != EVAL_TAG_STRING || right_tag != EVAL_TAG_STRING) { return Ok(None); }
            let Some(left) = self.cast(EvalCastType::String, left)? else { return Ok(None); };
            let Some(right) = self.cast(EvalCastType::String, right)? else { return Ok(None); };
            return self.make(|_, values| values.concat(left, right));
        }
        if op == EvalBinOp::LogicalXor {
            let (Some(left), Some(right)) = (self.condition(left)?, self.condition(right)?) else { return Ok(None); };
            return self.make(|_, values| values.bool_value(left ^ right));
        }
        if matches!(op, EvalBinOp::LooseEq | EvalBinOp::LooseNotEq | EvalBinOp::StrictEq | EvalBinOp::StrictNotEq | EvalBinOp::Lt | EvalBinOp::LtEq | EvalBinOp::Gt | EvalBinOp::GtEq | EvalBinOp::Spaceship) {
            return self.compare(op, left, right, left_tag, right_tag);
        }
        let operator = match op {
            EvalBinOp::Div => NumericOperator::Division, EvalBinOp::Mod => NumericOperator::Modulo,
            EvalBinOp::Pow => NumericOperator::Power,
            EvalBinOp::BitAnd | EvalBinOp::BitOr | EvalBinOp::BitXor => NumericOperator::Bitwise,
            EvalBinOp::ShiftLeft | EvalBinOp::ShiftRight => NumericOperator::Shift,
            _ => NumericOperator::Arithmetic,
        };
        if !self.domain(operator, left)? || !self.domain(operator, right)? { return Ok(None); }
        if matches!(operator, NumericOperator::Bitwise | NumericOperator::Shift) {
            if operator == NumericOperator::Bitwise && left_tag == EVAL_TAG_STRING && right_tag == EVAL_TAG_STRING {
                return self.make(|_, values| values.bitwise_dispatch(op, left, right));
            }
            let (Some(left), Some(right)) = (self.integer(left)?, self.integer(right)?) else { return Ok(None); };
            let op = match op { EvalBinOp::BitAnd => IntegerBitOp::And, EvalBinOp::BitOr => IntegerBitOp::Or,
                EvalBinOp::BitXor => IntegerBitOp::Xor, EvalBinOp::ShiftLeft => IntegerBitOp::ShiftLeft, _ => IntegerBitOp::ShiftRight };
            return match domains::integer_bitwise(op, left, right) { Ok(value) => self.numeric_result(NumericValue::Int(value)), Err(_) => Ok(None) };
        }
        let (left, right) = if op == EvalBinOp::Mod {
            let (Some(left), Some(right)) = (self.integer(left)?, self.integer(right)?) else { return Ok(None); };
            (NumericValue::Int(left), NumericValue::Int(right))
        } else {
            let (Some(left), Some(right)) = (self.number(left)?, self.number(right)?) else { return Ok(None); };
            (left, right)
        };
        let op = match op { EvalBinOp::Add => IntegerOp::Add, EvalBinOp::Sub => IntegerOp::Sub,
            EvalBinOp::Mul => IntegerOp::Mul, EvalBinOp::Div => IntegerOp::Div, EvalBinOp::Mod => IntegerOp::Mod,
            EvalBinOp::Pow => IntegerOp::Pow, _ => return Ok(None) };
        match elephc_semantics::constant_numeric::evaluate(self.context.semantic_profile(), op, left, right) {
            Some(value) => self.numeric_result(value), None => Ok(None),
        }
    }

    /// Reuses the byte-aware runtime scalar comparator after proving strict and warning domains.
    fn compare(&mut self, op: EvalBinOp, left: RuntimeCellHandle, right: RuntimeCellHandle,
        left_tag: u64, right_tag: u64,
    ) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        if !matches!(op, EvalBinOp::StrictEq | EvalBinOp::StrictNotEq) {
            if self.context.semantic_profile().contains(StrictFeature::Comparisons) {
                use elephc_semantics::comparisons::{ordering, OrderingPlan};
                if matches!(op, EvalBinOp::LooseEq | EvalBinOp::LooseNotEq) { return Ok(None); }
                if ordering(numeric_domains::value_kind(left_tag), numeric_domains::value_kind(right_tag)) != OrderingPlan::Exact { return Ok(None); }
                if left_tag == EVAL_TAG_STRING {
                    return self.make(|context, values| checked_comparisons::binary(op, left, right, context, values)?.ok_or(EvalStatus::RuntimeFatal));
                }
            }
            if matches!(left_tag, EVAL_TAG_NULL | EVAL_TAG_BOOL) && self.silent_truthy(right)?.is_none()
                || matches!(right_tag, EVAL_TAG_NULL | EVAL_TAG_BOOL) && self.silent_truthy(left)?.is_none() { return Ok(None); }
        }
        self.make(|_, values| if op == EvalBinOp::Spaceship { values.spaceship(left, right) } else { values.compare(op, left, right) })
    }
}
