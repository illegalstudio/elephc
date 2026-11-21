//! Purpose:
//! Plans PHP weak scalar binding independently of checked explicit numeric casts.
//!
//! Called from:
//! - Declaration adapters that have already tested exact nominal and structural membership.
//!
//! Key details:
//! - Exact scalar alternatives win before int, float, string and bool coercion preference.
//! - Numeric strings preserve PHP's integer/float classification and implicit integer bounds.
//! - Null and non-scalar inputs require an explicitly accepting declaration or object adapter.

use crate::conversions::{AllowedKinds, ValueKind};
use crate::numeric::checked_float_to_int;
use crate::numeric_string::scan_numeric_prefix;
use crate::numeric_unary::NumericValue;

/// Original scalar values, including byte strings that are not necessarily UTF-8.
#[derive(Clone, Copy, Debug)]
pub enum ScalarBindingInput<'a> {
    Integer(i64), Float(f64), Boolean(bool), Bytes(&'a [u8]), Null,
}

/// A declaration's scalar adaptation; integer precision diagnostics precede publication.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WeakScalarBinding {
    Identity,
    Integer { value: i64, loses_precision: bool },
    Float(f64),
    Boolean(bool),
    String,
}

/// Checks a scalar source kind's conversion domain while leaving contents and ranges to plan().
pub fn supports_source_kind(allowed: AllowedKinds, source: ValueKind) -> bool {
    if matches!(source, ValueKind::Dynamic | ValueKind::NonScalar) { return false; }
    if allowed.allows(source) { return true; }
    if source == ValueKind::Null { return false; }
    [ValueKind::Integer, ValueKind::Float, ValueKind::Boolean, ValueKind::String]
        .iter().any(|kind| allowed.allows(*kind))
}

/// Selects exact membership or PHP's weak scalar preference, returning None for a type failure.
pub fn plan(allowed: AllowedKinds, input: ScalarBindingInput<'_>) -> Option<WeakScalarBinding> {
    use ScalarBindingInput as Input;
    use WeakScalarBinding as Plan;
    let kind = match input {
        Input::Integer(_) => ValueKind::Integer, Input::Float(_) => ValueKind::Float,
        Input::Boolean(_) => ValueKind::Boolean, Input::Bytes(_) => ValueKind::String,
        Input::Null => ValueKind::Null,
    };
    if allowed.allows(kind) { return Some(Plan::Identity); }
    if !supports_source_kind(allowed, kind) { return None; }
    let number = match input {
        Input::Integer(value) => Some(NumericValue::Int(value)),
        Input::Float(value) => Some(NumericValue::Float(value)),
        Input::Boolean(value) => Some(NumericValue::Int(i64::from(value))),
        Input::Bytes(bytes) => numeric_string(bytes), Input::Null => None,
    };
    if let Some(number) = number {
        // A string declaring both numeric alternatives follows its PHP numeric kind.
        let prefer_float = matches!(input, Input::Bytes(_)) && matches!(number, NumericValue::Float(_))
            && allowed.allows(ValueKind::Float);
        if allowed.allows(ValueKind::Integer) && !prefer_float {
            match number {
                NumericValue::Int(value) => return Some(Plan::Integer { value, loses_precision: false }),
                NumericValue::Float(value) => {
                    if let Ok(integer) = checked_float_to_int(value) {
                        return Some(Plan::Integer { value: integer, loses_precision: value != integer as f64 });
                    }
                }
            }
        }
        if allowed.allows(ValueKind::Float) {
            return Some(Plan::Float(match number {
                NumericValue::Int(value) => value as f64, NumericValue::Float(value) => value,
            }));
        }
    }
    if allowed.allows(ValueKind::String) { return Some(Plan::String); }
    if allowed.allows(ValueKind::Boolean) {
        return Some(Plan::Boolean(match input {
            Input::Integer(value) => value != 0, Input::Float(value) => value != 0.0,
            Input::Boolean(value) => value, Input::Bytes(value) => !value.is_empty() && value != b"0",
            Input::Null => false,
        }));
    }
    None
}

/// Classifies complete PHP numeric strings without rounding exact integer spellings through float.
fn numeric_string(bytes: &[u8]) -> Option<NumericValue> {
    let value = std::str::from_utf8(bytes).ok()?;
    let scan = scan_numeric_prefix(value).filter(|scan| scan.is_fully_numeric())?;
    if !scan.is_float {
        if let Ok(value) = scan.text.parse::<i64>() { return Some(NumericValue::Int(value)); }
    }
    scan.text.parse().ok().map(NumericValue::Float)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Source domains keep unknown/null distinct from scalar values whose contents can still fail.
    #[test]
    fn weak_scalar_source_domains_preserve_unknown_null_and_value_checks() {
        let int = AllowedKinds::one(ValueKind::Integer);
        for kind in [ValueKind::Integer, ValueKind::Float, ValueKind::Boolean, ValueKind::String] {
            assert!(supports_source_kind(int, kind));
        }
        for kind in [ValueKind::Dynamic, ValueKind::NonScalar, ValueKind::Null] {
            assert!(!supports_source_kind(int, kind));
        }
        assert!(supports_source_kind(int.union(AllowedKinds::one(ValueKind::Null)), ValueKind::Null));
        assert!(!supports_source_kind(AllowedKinds::one(ValueKind::NonScalar), ValueKind::Integer));
        assert_eq!(plan(int, ScalarBindingInput::Bytes(b"not numeric")), None);
        assert_eq!(plan(int, ScalarBindingInput::Float(f64::NAN)), None);
    }

    /// Union preference follows numeric string kind while exact string/bool members win first.
    #[test]
    fn weak_scalar_binding_preserves_exact_members_and_numeric_string_kind() {
        let numeric = AllowedKinds::one(ValueKind::Integer).union(AllowedKinds::one(ValueKind::Float));
        assert_eq!(plan(numeric, ScalarBindingInput::Bytes(b"1")),
            Some(WeakScalarBinding::Integer { value: 1, loses_precision: false }));
        assert_eq!(plan(numeric, ScalarBindingInput::Bytes(b"1.0")), Some(WeakScalarBinding::Float(1.0)));
        assert_eq!(plan(numeric.union(AllowedKinds::one(ValueKind::String)), ScalarBindingInput::Bytes(b"1.0")),
            Some(WeakScalarBinding::Identity));
        assert_eq!(plan(numeric, ScalarBindingInput::Boolean(true)),
            Some(WeakScalarBinding::Integer { value: 1, loses_precision: false }));
    }

    /// Implicit integer declarations reject rounded overflow while explicit decimal casts remain exact.
    #[test]
    fn weak_scalar_binding_uses_implicit_integer_bounds_and_fallbacks() {
        let integer = AllowedKinds::one(ValueKind::Integer);
        assert_eq!(plan(integer, ScalarBindingInput::Bytes(b"9223372036854775807")),
            Some(WeakScalarBinding::Integer { value: i64::MAX, loses_precision: false }));
        assert_eq!(plan(integer, ScalarBindingInput::Bytes(b"9223372036854775807.9")), None);
        assert_eq!(crate::numeric::checked_string_to_int("9223372036854775807.9"), Ok(i64::MAX));
        assert_eq!(plan(integer, ScalarBindingInput::Float(f64::NAN)), None);
        assert_eq!(plan(integer.union(AllowedKinds::one(ValueKind::String)), ScalarBindingInput::Float(f64::NAN)),
            Some(WeakScalarBinding::String));
        assert_eq!(plan(integer, ScalarBindingInput::Bytes(b"1.5")),
            Some(WeakScalarBinding::Integer { value: 1, loses_precision: true }));
        assert_eq!(crate::numeric::checked_float_to_int(f64::INFINITY), Err(crate::numeric::NumericFailure::Overflow));
    }

    /// Invalid byte strings can bind bool but nullable acceptance remains explicit.
    #[test]
    fn weak_scalar_binding_retains_byte_truthiness_and_null_domains() {
        let boolean = AllowedKinds::one(ValueKind::Boolean);
        for (bytes, expected) in [(b"".as_slice(), false), (b"0", false), (b"00", true), (b"\xff", true)] {
            assert_eq!(plan(boolean, ScalarBindingInput::Bytes(bytes)), Some(WeakScalarBinding::Boolean(expected)));
        }
        assert_eq!(plan(boolean, ScalarBindingInput::Null), None);
        assert_eq!(plan(boolean.union(AllowedKinds::one(ValueKind::Null)), ScalarBindingInput::Null),
            Some(WeakScalarBinding::Identity));
    }

    /// Weak float binding follows numeric string kind, retaining signed zero only for float syntax.
    #[test]
    fn weak_scalar_binding_uses_numeric_kind_for_signed_zero() {
        for (bytes, expected) in [(b"-0".as_slice(), 0.0f64), (b"  -000 ", 0.0), (b"-0e999", -0.0)] {
            let Some(WeakScalarBinding::Float(value)) = plan(AllowedKinds::one(ValueKind::Float),
                ScalarBindingInput::Bytes(bytes)) else { panic!("expected float for {bytes:?}"); };
            assert_eq!(value.to_bits(), expected.to_bits());
        }
    }

}
