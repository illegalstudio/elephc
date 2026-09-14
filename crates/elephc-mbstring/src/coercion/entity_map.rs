//! Purpose:
//! Plans numeric-entity map casts with PHP's integer, warning, and overflow rules.
//!
//! Called from:
//! - Shared entity adapters before each protected diagnostic callback.
//!
//! Key details:
//! - Map elements use arithmetic integer conversion independently of caller strictness.
//! - Floats wrap on overflow; numeric strings saturate and allow a warned numeric prefix.
//! - All diagnostic messages own their bytes before host callbacks can mutate references.

use super::{Diagnostic, Input, messages, numeric::{Numeric, numeric_prefix, whitespace}};

/// Converts one map element and owns its ordered diagnostics without invoking host callbacks.
pub(crate) fn integer(input: Input<'_>) -> (Option<i64>, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let value = match input {
        Input::Null | Input::Bool(false) => Some(0),
        Input::Bool(true) => Some(1),
        Input::Int(value) => Some(value),
        Input::Float(bits) => {
            let value = f64::from_bits(bits);
            let fits = !(value >= -(i64::MIN as f64) || value < i64::MIN as f64);
            let integer = if !value.is_finite() { 0 } else if fits { value as i64 }
                else { ((value % 18446744073709551616.0).rem_euclid(18446744073709551616.0) as u64) as i64 };
            if !value.is_finite() || !fits {
                diagnostics.push(Diagnostic { level: 2, message: format!(
                    "The float {} is not representable as an int, cast occurred", messages::shortest_float(value)).into_bytes() });
            }
            if fits && integer as f64 != value { diagnostics.push(messages::lossy_float(bits)); }
            Some(integer)
        },
        Input::String(bytes) => {
            let Some((number, consumed)) = numeric_prefix(bytes) else { return (None, diagnostics); };
            if !bytes[consumed..].iter().copied().all(whitespace) {
                diagnostics.push(Diagnostic { level: 2, message: b"A non-numeric value encountered".to_vec() });
            }
            Some(match number {
                Numeric::Integer(value) => value,
                Numeric::Float(value) => {
                    let integer = if value.is_finite() { value as i64 } else { 0 };
                    if integer as f64 != value { diagnostics.push(messages::lossy_string(bytes)); }
                    integer
                },
            })
        },
        Input::Array | Input::Object { .. } | Input::Resource { .. } => None,
    };
    (value, diagnostics)
}
