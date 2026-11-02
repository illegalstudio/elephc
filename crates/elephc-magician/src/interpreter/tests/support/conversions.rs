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

    /// Returns PHP scalar ordering and whether a numeric comparison was IEEE unordered.
    pub(super) fn php_ordering(
        &self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> (std::cmp::Ordering, bool) {
        let left = self.get(left);
        let right = self.get(right);
        if matches!(left, FakeValue::Bool(_)) || matches!(right, FakeValue::Bool(_)) {
            return (self.fake_truthy(&left).cmp(&self.fake_truthy(&right)), false);
        }
        if let (FakeValue::Int(left), FakeValue::Int(right)) = (&left, &right) {
            return (left.cmp(right), false);
        }
        let Some(ordering) = self.fake_numeric(&left).partial_cmp(&self.fake_numeric(&right)) else {
            return (std::cmp::Ordering::Greater, true);
        };
        (ordering, false)
    }

    /// Converts a fake value to the numeric scalar used by comparison tests.
    pub(super) fn fake_numeric(&self, value: &FakeValue) -> f64 {
        match value {
            FakeValue::Null => 0.0,
            FakeValue::Bool(false) => 0.0,
            FakeValue::Bool(true) => 1.0,
            FakeValue::Int(value) => *value as f64,
            FakeValue::Float(value) => *value,
            FakeValue::String(value) => fake_leading_numeric_value(value.as_bytes()),
            FakeValue::Bytes(value) => fake_leading_numeric_value(value),
            FakeValue::Array(value) => value.len() as f64,
            FakeValue::Assoc(value) => value.len() as f64,
            FakeValue::Object(_) | FakeValue::Iterator { .. } => 1.0,
            FakeValue::Resource(value) => self.fake_resource_id(*value) as f64,
            FakeValue::InvokerRefCell(_) => 0.0,
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
            FakeValue::InvokerRefCell(_) => true,
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
            FakeValue::Resource(value) => format!("Resource id #{}", self.fake_resource_id(value)),
            FakeValue::InvokerRefCell(_) => "[invoker-ref]".to_string(),
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
            FakeValue::Resource(value) => format!("Resource id #{}", self.fake_resource_id(*value)),
            FakeValue::InvokerRefCell(_) => "[invoker-ref]".to_string(),
        }
    }
}

/// Returns the value of a PHP string's LEADING NUMERIC PREFIX, which is what PHP converts.
///
/// FAKE-RUNTIME DEFECT this replaces: the fixture parsed the WHOLE string and fell back to zero,
/// so an int cast of "34x" answered 0 where `php -n` 8.5.6 answers 34. The real runtime already
/// gets this right -- `tests/ir_backend_smoke_test.rs` pins `intval("42xyz")` at `42` through
/// the compiled backend -- so the divergence was the fixture's alone, and it reached every
/// numeric use of a string rather than only casts.
///
/// PHP's rule, each part measured: leading whitespace is skipped and trailing whitespace
/// tolerated (`"  12  "` is 12); an optional sign, digits, an optional fractional part and an
/// optional exponent are taken (`"1e3"` is 1000, `"12.9abc"` is 12 as an int); anything not
/// starting with a number is 0 (`"x34"`, and `"0x1A"` because hex is NOT recognised); the empty
/// string is 0.
fn fake_leading_numeric_value(bytes: &[u8]) -> f64 {
    let text = String::from_utf8_lossy(bytes);
    let trimmed = text.trim_start();
    let raw = trimmed.as_bytes();
    let mut end = 0;
    if end < raw.len() && matches!(raw[end], b'+' | b'-') {
        end += 1;
    }
    let digits_start = end;
    while end < raw.len() && raw[end].is_ascii_digit() {
        end += 1;
    }
    let integer_digits = end - digits_start;
    let mut fraction_digits = 0;
    if end < raw.len() && raw[end] == b'.' {
        let dot = end;
        end += 1;
        let fraction_start = end;
        while end < raw.len() && raw[end].is_ascii_digit() {
            end += 1;
        }
        fraction_digits = end - fraction_start;
        if integer_digits == 0 && fraction_digits == 0 {
            end = dot;
        }
    }
    if integer_digits == 0 && fraction_digits == 0 {
        return 0.0;
    }
    if end < raw.len() && matches!(raw[end], b'e' | b'E') {
        let exponent_marker = end;
        end += 1;
        if end < raw.len() && matches!(raw[end], b'+' | b'-') {
            end += 1;
        }
        let exponent_start = end;
        while end < raw.len() && raw[end].is_ascii_digit() {
            end += 1;
        }
        if end == exponent_start {
            end = exponent_marker;
        }
    }
    trimmed[..end].parse::<f64>().unwrap_or(0.0)
}
