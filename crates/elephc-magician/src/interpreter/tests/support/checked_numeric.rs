//! Purpose:
//! Models explicit checked scalar callbacks for focused interpreter tests.
//!
//! Called from:
//! - RuntimeValueOps for FakeOps, separately from permissive arithmetic hooks.
//!
//! Key details:
//! - Exact numeric strings use the shared host grammar and integer evaluator.
//! - Failures transfer a fake pending Throwable through the same hook contract as native code.

use super::*;
use elephc_semantics::numeric::{checked_integer, CheckedNumericOp, IntegerOp, INTEGER_OVERFLOW_MESSAGE};

/// Test-only normalized scalar numeric payload, captured before diagnostic callbacks.
#[derive(Clone, Copy)]
enum Number { Int(i64), Float(f64) }

impl Number {
    /// Exposes a floating operand only when the PHP operation actually selects floating arithmetic.
    fn float(self) -> f64 { match self { Self::Int(value) => value as f64, Self::Float(value) => value } }
}

impl FakeOps {
    /// Classifies a borrowed scalar with exact integers and numeric-prefix warning evidence.
    fn checked_number(&self, value: RuntimeCellHandle) -> Option<(Number, bool)> {
        match self.get(value) {
            FakeValue::Int(value) => Some((Number::Int(value), false)),
            FakeValue::Float(value) => Some((Number::Float(value), false)),
            FakeValue::Bool(value) => Some((Number::Int(i64::from(value)), false)),
            FakeValue::Null => Some((Number::Int(0), false)),
            FakeValue::String(value) => checked_string(&value),
            FakeValue::Bytes(value) => checked_string(std::str::from_utf8(&value).ok()?),
            _ => None,
        }
    }

    /// Constructs one owned pending exception after retiring its constructor arguments.
    fn checked_error(&mut self, class: &str, message: &str) -> Result<RuntimeCellHandle, EvalStatus> {
        let error = self.new_object(class)?;
        let message = self.string(message)?;
        let code = self.int(0)?;
        self.construct_object(error, vec![message, code])?;
        self.release(message)?;
        self.release(code)?;
        self.pending_runtime_throwable = Some(error);
        Err(EvalStatus::UncaughtThrowable)
    }

    /// Checks scalar numeric results without relying on the fake's deliberately limited PHP hooks.
    pub(super) fn runtime_checked_numeric(
        &mut self, op: CheckedNumericOp, left: RuntimeCellHandle, right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        if matches!(op, CheckedNumericOp::ArraySum | CheckedNumericOp::ArrayProduct) {
            return self.runtime_checked_aggregate(op, left);
        }
        let Some((left, left_warning)) = self.checked_number(left) else {
            return self.checked_error("TypeError", "Unsupported operand types");
        };
        if op == CheckedNumericOp::Abs {
            if left_warning { return self.checked_error("TypeError", "abs(): argument must be numeric"); }
            return match left {
                Number::Int(value) => match value.checked_abs() {
                    Some(value) => self.int(value),
                    None => self.checked_error("ArithmeticError", INTEGER_OVERFLOW_MESSAGE),
                },
                Number::Float(value) => self.float(value.abs()),
            };
        }
        let Some((right, right_warning)) = self.checked_number(right) else {
            return self.checked_error("TypeError", "Unsupported operand types");
        };
        for warn in [left_warning, right_warning] {
            if warn { self.warning("Warning: A non-numeric value encountered\n")?; }
        }
        let integer_op = match op {
            CheckedNumericOp::Add => IntegerOp::Add, CheckedNumericOp::Sub => IntegerOp::Sub,
            CheckedNumericOp::Mul => IntegerOp::Mul, CheckedNumericOp::Pow => IntegerOp::Pow,
            CheckedNumericOp::Abs | CheckedNumericOp::ArraySum | CheckedNumericOp::ArrayProduct =>
                unreachable!("unary and reduction operations were handled above"),
        };
        if let (Number::Int(left), Number::Int(right)) = (left, right) {
            if op != CheckedNumericOp::Pow || right >= 0 {
                return match checked_integer(integer_op, left, right) {
                    Ok(value) => self.int(value),
                    Err(_) => self.checked_error("ArithmeticError", INTEGER_OVERFLOW_MESSAGE),
                };
            }
        }
        self.float(match op {
            CheckedNumericOp::Add => left.float() + right.float(),
            CheckedNumericOp::Sub => left.float() - right.float(),
            CheckedNumericOp::Mul => left.float() * right.float(),
            CheckedNumericOp::Pow => left.float().powf(right.float()),
            CheckedNumericOp::Abs | CheckedNumericOp::ArraySum | CheckedNumericOp::ArrayProduct =>
                unreachable!("unary and reduction operations were handled above"),
        })
    }

    /// Models checked numeric reductions without allocating an unrooted intermediate carry.
    fn runtime_checked_aggregate(
        &mut self, op: CheckedNumericOp, array: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let values = match self.get(array) {
            FakeValue::Array(values) => values,
            FakeValue::Assoc(values) => values.into_iter().map(|(_, value)| value).collect(),
            _ => return self.checked_error("TypeError", "numeric reduction requires an array"),
        };
        let product = op == CheckedNumericOp::ArrayProduct;
        let operation = if product { IntegerOp::Mul } else { IntegerOp::Add };
        let mut result = Number::Int(i64::from(product));
        for value in values {
            let original = value;
            let converted = self.number_value(original.borrowed())?;
            let classified = self.checked_number(converted);
            if converted.as_ptr() != original.as_ptr() && !converted.is_borrowed() { self.release(converted)?; }
            let (value, warning) = match classified {
                Some(number) => number,
                None => {
                    self.warning("Warning: Unsupported value in numeric reduction\n")?;
                    if !matches!(self.get(original), FakeValue::String(_) | FakeValue::Bytes(_)) { continue; }
                    (Number::Int(0), false)
                }
            };
            if warning { self.warning("Warning: A non-numeric value encountered\n")?; }
            result = match (result, value) {
                (Number::Int(left), Number::Int(right)) => match checked_integer(operation, left, right) {
                    Ok(value) => Number::Int(value),
                    Err(_) => return self.checked_error("ArithmeticError", INTEGER_OVERFLOW_MESSAGE),
                },
                (left, right) if product => Number::Float(left.float() * right.float()),
                (left, right) => Number::Float(left.float() + right.float()),
            };
        }
        match result { Number::Int(value) => self.int(value), Number::Float(value) => self.float(value) }
    }

}

/// Parses a test scalar through the same grammar used by compiler source evidence.
fn checked_string(value: &str) -> Option<(Number, bool)> {
    let scan = elephc_semantics::numeric_string::scan_numeric_prefix(value)?;
    let warning = !scan.is_fully_numeric();
    if !scan.is_float {
        if let Ok(value) = scan.text.parse() { return Some((Number::Int(value), warning)); }
    }
    scan.text.parse().ok().map(|value| (Number::Float(value), warning))
}
