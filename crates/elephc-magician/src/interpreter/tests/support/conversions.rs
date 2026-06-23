//! Purpose:
//! Conversion, comparison, and stringification helpers for fake interpreter values.
//! RuntimeValueOps methods delegate here to keep scalar PHP-like coercion rules
//! out of the trait implementation file.
//!
//! Called from:
//! - `crate::interpreter::tests::support::runtime_ops`.
//!
//! Key details:
//! - Helpers intentionally cover only semantics asserted by eval interpreter tests.

use super::*;

impl FakeOps {
    /// Compares fake scalar values with the same loose rules covered by eval tests.
    pub(super) fn loose_eq(&self, left: RuntimeCellHandle, right: RuntimeCellHandle) -> bool {
        match (self.get(left), self.get(right)) {
            (FakeValue::Bool(left), right) => left == self.fake_truthy(&right),
            (left, FakeValue::Bool(right)) => self.fake_truthy(&left) == right,
            (FakeValue::Null, FakeValue::Null) => true,
            (FakeValue::Null, FakeValue::String(value))
            | (FakeValue::String(value), FakeValue::Null) => value.is_empty(),
            (FakeValue::Null, FakeValue::Bytes(value))
            | (FakeValue::Bytes(value), FakeValue::Null) => value.is_empty(),
            (FakeValue::String(left), FakeValue::String(right)) => {
                match (left.parse::<f64>(), right.parse::<f64>()) {
                    (Ok(left), Ok(right)) => left == right,
                    _ => left == right,
                }
            }
            (FakeValue::Bytes(left), FakeValue::Bytes(right)) => left == right,
            (FakeValue::String(left), FakeValue::Bytes(right))
            | (FakeValue::Bytes(right), FakeValue::String(left)) => left.as_bytes() == right,
            (FakeValue::String(left), right) => left
                .parse::<f64>()
                .is_ok_and(|left| left == self.fake_numeric(&right)),
            (FakeValue::Bytes(left), right) => std::str::from_utf8(&left)
                .ok()
                .and_then(|left| left.parse::<f64>().ok())
                .is_some_and(|left| left == self.fake_numeric(&right)),
            (left, FakeValue::String(right)) => right
                .parse::<f64>()
                .is_ok_and(|right| self.fake_numeric(&left) == right),
            (left, FakeValue::Bytes(right)) => std::str::from_utf8(&right)
                .ok()
                .and_then(|right| right.parse::<f64>().ok())
                .is_some_and(|right| self.fake_numeric(&left) == right),
            (left, right) => self.fake_numeric(&left) == self.fake_numeric(&right),
        }
    }

    /// Compares fake scalar values by PHP strict tag and payload equality.
    pub(super) fn strict_eq(&self, left: RuntimeCellHandle, right: RuntimeCellHandle) -> bool {
        if left == right
            && matches!(
                self.get(left),
                FakeValue::Object(_) | FakeValue::Iterator { .. }
            )
        {
            return true;
        }
        match (self.get(left), self.get(right)) {
            (FakeValue::Null, FakeValue::Null) => true,
            (FakeValue::Bool(left), FakeValue::Bool(right)) => left == right,
            (FakeValue::Int(left), FakeValue::Int(right)) => left == right,
            (FakeValue::Float(left), FakeValue::Float(right)) => left == right,
            (FakeValue::String(left), FakeValue::String(right)) => left == right,
            (FakeValue::Bytes(left), FakeValue::Bytes(right)) => left == right,
            (FakeValue::String(left), FakeValue::Bytes(right))
            | (FakeValue::Bytes(right), FakeValue::String(left)) => left.as_bytes() == right,
            (FakeValue::Resource(left), FakeValue::Resource(right)) => left == right,
            _ => false,
        }
    }

    /// Converts one fake scalar cell to a numeric value for comparison tests.
    pub(super) fn numeric(&self, handle: RuntimeCellHandle) -> Result<f64, EvalStatus> {
        Ok(self.fake_numeric(&self.get(handle)))
    }

    /// Converts a fake value to the numeric scalar used by comparison tests.
    pub(super) fn fake_numeric(&self, value: &FakeValue) -> f64 {
        match value {
            FakeValue::Null => 0.0,
            FakeValue::Bool(false) => 0.0,
            FakeValue::Bool(true) => 1.0,
            FakeValue::Int(value) => *value as f64,
            FakeValue::Float(value) => *value,
            FakeValue::String(value) => value.parse::<f64>().unwrap_or(0.0),
            FakeValue::Bytes(value) => std::str::from_utf8(value)
                .ok()
                .and_then(|value| value.parse::<f64>().ok())
                .unwrap_or(0.0),
            FakeValue::Array(value) => value.len() as f64,
            FakeValue::Assoc(value) => value.len() as f64,
            FakeValue::Object(_) | FakeValue::Iterator { .. } => 1.0,
            FakeValue::Resource(value) => (*value + 1) as f64,
        }
    }

    /// Converts a fake value to the integer scalar used by modulo tests.
    pub(super) fn fake_int(&self, value: &FakeValue) -> i64 {
        self.fake_numeric(value) as i64
    }

    /// Returns fake PHP truthiness for already-loaded test values.
    pub(super) fn fake_truthy(&self, value: &FakeValue) -> bool {
        match value {
            FakeValue::Null => false,
            FakeValue::Bool(value) => *value,
            FakeValue::Int(value) => *value != 0,
            FakeValue::Float(value) => *value != 0.0,
            FakeValue::String(value) => !value.is_empty() && value != "0",
            FakeValue::Bytes(value) => !value.is_empty() && value.as_slice() != b"0",
            FakeValue::Array(value) => !value.is_empty(),
            FakeValue::Assoc(value) => !value.is_empty(),
            FakeValue::Object(_) | FakeValue::Iterator { .. } => true,
            FakeValue::Resource(_) => true,
        }
    }

    /// Converts a fake runtime cell to a PHP-like string for test echo/concat.
    pub(super) fn stringify(&self, handle: RuntimeCellHandle) -> String {
        match self.get(handle) {
            FakeValue::Null => String::new(),
            FakeValue::Bool(false) => String::new(),
            FakeValue::Bool(true) => "1".to_string(),
            FakeValue::Int(value) => value.to_string(),
            FakeValue::Float(value) => value.to_string(),
            FakeValue::String(value) => value,
            FakeValue::Bytes(value) => String::from_utf8_lossy(&value).into_owned(),
            FakeValue::Array(_) => "Array".to_string(),
            FakeValue::Assoc(_) => "Array".to_string(),
            FakeValue::Object(_) | FakeValue::Iterator { .. } => "Object".to_string(),
            FakeValue::Resource(value) => format!("Resource id #{}", value + 1),
        }
    }

    /// Converts a fake PHP value to string bytes while preserving binary strings.
    pub(super) fn string_bytes_for_value(&self, value: &FakeValue) -> Vec<u8> {
        match value {
            FakeValue::String(value) => value.as_bytes().to_vec(),
            FakeValue::Bytes(value) => value.clone(),
            value => self.stringify_value(value).into_bytes(),
        }
    }

    /// Converts one loaded fake PHP value to display text for byte coercions.
    pub(super) fn stringify_value(&self, value: &FakeValue) -> String {
        match value {
            FakeValue::Null => String::new(),
            FakeValue::Bool(false) => String::new(),
            FakeValue::Bool(true) => "1".to_string(),
            FakeValue::Int(value) => value.to_string(),
            FakeValue::Float(value) => value.to_string(),
            FakeValue::String(value) => value.clone(),
            FakeValue::Bytes(value) => String::from_utf8_lossy(value).into_owned(),
            FakeValue::Array(_) | FakeValue::Assoc(_) => "Array".to_string(),
            FakeValue::Object(_) | FakeValue::Iterator { .. } => "Object".to_string(),
            FakeValue::Resource(value) => format!("Resource id #{}", value + 1),
        }
    }
}
