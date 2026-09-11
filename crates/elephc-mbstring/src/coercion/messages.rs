//! Purpose:
//! Formats binary-safe PHP parameter errors and ordered scalar-coercion diagnostics.
//!
//! Called from:
//! - The shared mbstring parameter-coercion planner.
//!
//! Key details:
//! - PHP parameter names and type spellings come from the neutral contract.
//! - Precision-loss notices use shortest double formatting independently of PHP display precision.

use super::{BuiltinContract, Diagnostic, Input};

/// Names the concrete input in PHP TypeError form without losing binary object class names.
fn input_name(input: Input<'_>) -> &[u8] {
    match input {
        Input::Null => b"null", Input::Bool(false) => b"false", Input::Bool(true) => b"true", Input::Int(_) => b"int",
        Input::Float(_) => b"float", Input::String(_) => b"string", Input::Array => b"array",
        Input::Object { class, .. } => class,
        Input::Resource { .. } => b"resource",
    }
}

/// Builds a complete binary-safe internal-function TypeError from shared parameter metadata.
pub(super) fn type_error(contract: &BuiltinContract, index: usize, parsed_type: super::TypeSpec, input: Input<'_>) -> Vec<u8> {
    let parameter = &contract.params[index];
    let mut bytes = format!("{}(): Argument #{} (${}) must be of type {}, ",
        contract.name, index + 1, parameter.name, parsed_type).into_bytes();
    bytes.extend_from_slice(input_name(input));
    bytes.extend_from_slice(b" given");
    bytes
}

/// Describes PHP's deprecated null-to-nonnullable-scalar conversion before applying its zero value.
pub(super) fn null_argument(contract: &BuiltinContract, index: usize) -> Diagnostic {
    let parameter = &contract.params[index];
    Diagnostic { level: 8192, message: format!("{}(): Passing null to parameter #{} (${}) of type {} is deprecated",
        contract.name, index + 1, parameter.name, parameter.ty).into_bytes() }
}

/// Reports the original numeric string when its fractional part is discarded by an integer parameter.
pub(super) fn lossy_string(bytes: &[u8]) -> Diagnostic {
    let mut message = b"Implicit conversion from float-string \"".to_vec();
    message.extend_from_slice(bytes);
    message.extend_from_slice(b"\" to int loses precision");
    Diagnostic { level: 8192, message }
}

/// Reports a lossy double using PHP's shortest decimal/exponential diagnostic spelling.
pub(super) fn lossy_float(bits: u64) -> Diagnostic {
    Diagnostic { level: 8192, message: format!("Implicit conversion from float {} to int loses precision",
        shortest_float(f64::from_bits(bits))).into_bytes() }
}

/// Reports PHP 8.5's NaN warning before a boolean or string conversion.
pub(super) fn nan(target: &str) -> Diagnostic {
    Diagnostic { level: 2, message: format!("unexpected NAN value was coerced to {target}").into_bytes() }
}

/// Renders shortest digits with zend_gcvt's 17-digit and small-exponent style boundaries.
pub(super) fn shortest_float(value: f64) -> String {
    if value.is_nan() { return "NAN".into(); }
    if value.is_infinite() { return if value.is_sign_negative() { "-INF" } else { "INF" }.into(); }
    let scientific = format!("{value:e}");
    let (mantissa, exponent) = scientific.split_once('e').expect("finite scientific double");
    let exponent = exponent.parse::<i32>().expect("decimal double exponent");
    if exponent < -4 || exponent >= 17 {
        let fraction = if mantissa.contains('.') { "" } else { ".0" };
        format!("{mantissa}{fraction}E{exponent:+}")
    } else { value.to_string() }
}
