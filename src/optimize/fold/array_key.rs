//! Purpose:
//! Classifies compile-time scalar literals under the selected array key policy.
//! Gives array-literal access folding the same typed key identity as the runtime hash table.
//!
//! Called from:
//! - `crate::optimize::fold::ops`
//! - `crate::types::deferred_defaults` (float key check before a literal array decides a branch)
//!
//! Key details:
//! - The neutral string classifier owns PHP normalization and exact byte-preserving identity.
//! - Invalid exact-key domains decline folding so their original source remains observable.
//! - Float keys truncate toward zero, but PHP 8.1+ *deprecates* the lossy ones. Folding one
//!   would swallow the diagnostic, so only exactly-representable floats normalize.

use elephc_semantics::{array_keys::{classify_string_key, StringKey}, StrictFeature};

use super::scalar::ScalarValue;

/// A stored array key with its integer or string kind intact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ArrayKey {
    Int(i64),
    Str(String),
}

/// Classifies a scalar literal into the key the selected runtime would store.
///
/// `null` becomes `""`, booleans become `0`/`1`, integer-valued strings become integers, and
/// integral in-range floats truncate. Returns `None` for a float that PHP would report as a
/// lossy implicit conversion (fractional or out of `i64` range) so the fold declines instead
/// of swallowing the deprecation notice.
/// Exact-key mode accepts only original integers and strings, retaining numeric-looking strings.
pub(super) fn array_key(value: &ScalarValue) -> Option<ArrayKey> {
    let profile = super::super::semantic_context::active_profile();
    if profile.contains(StrictFeature::ArrayKeys)
        && !matches!(value, ScalarValue::Int(_) | ScalarValue::String(_)) { return None; }
    match value {
        ScalarValue::Null => Some(ArrayKey::Str(String::new())),
        ScalarValue::Bool(value) => Some(ArrayKey::Int(i64::from(*value))),
        ScalarValue::Int(value) => Some(ArrayKey::Int(*value)),
        ScalarValue::Float(value) => float_array_key(*value),
        ScalarValue::String(value) => Some(match classify_string_key(profile, value.as_bytes()) {
            StringKey::Integer(value) => ArrayKey::Int(value),
            StringKey::String(_) => ArrayKey::Str(value.clone()),
        }),
    }
}

/// Whether PHP converts float array key `value` to an int without any diagnostic.
pub(crate) fn float_key_is_silent(value: f64) -> bool {
    float_array_key(value).is_some()
}

/// Normalizes a float array key, declining every value PHP 8.1+ deprecates.
///
/// Only a finite float that is already integral and inside the `i64` range converts silently;
/// `1.7` and `1e20` both emit "Implicit conversion from float ... loses precision", which a
/// folded access would hide.
fn float_array_key(value: f64) -> Option<ArrayKey> {
    if !value.is_finite() || value.trunc() != value {
        return None;
    }
    // `i64::MAX as f64` rounds up, so the upper bound is exclusive.
    if value >= -(i64::MIN as f64) || value < i64::MIN as f64 {
        return None;
    }
    Some(ArrayKey::Int(value as i64))
}

#[cfg(test)]
mod tests;
