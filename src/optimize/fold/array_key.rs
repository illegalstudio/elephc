//! Purpose:
//! Normalizes compile-time scalar literals into PHP array keys.
//! Gives the array-literal access fold the same key identity the runtime hash table uses, so
//! `false`/`0`, `"1"`/`1` and `null`/`""` collapse to one slot instead of comparing raw variants.
//!
//! Called from:
//! - `crate::optimize::fold::ops`
//! - `crate::types::deferred_defaults` (float key check before a literal array decides a branch)
//!
//! Key details:
//! - The integer-string rule is shared with the type checker through
//!   `crate::types::is_php_integer_array_key`; there is exactly one definition of
//!   "this string is really an int key" in the compiler.
//! - Float keys truncate toward zero, but PHP 8.1+ *deprecates* the lossy ones. Folding one
//!   would swallow the diagnostic, so only exactly-representable floats normalize.

use crate::types::is_php_integer_array_key;

use super::scalar::ScalarValue;

/// A normalized PHP array key: the hash table only ever stores integers and strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PhpArrayKey {
    Int(i64),
    Str(String),
}

/// Normalizes a scalar literal into the array key PHP would actually store.
///
/// `null` becomes `""`, booleans become `0`/`1`, integer-valued strings become integers, and
/// integral in-range floats truncate. Returns `None` for a float that PHP would report as a
/// lossy implicit conversion (fractional or out of `i64` range) so the fold declines instead
/// of swallowing the deprecation notice.
pub(super) fn php_array_key(value: &ScalarValue) -> Option<PhpArrayKey> {
    match value {
        ScalarValue::Null => Some(PhpArrayKey::Str(String::new())),
        ScalarValue::Bool(value) => Some(PhpArrayKey::Int(i64::from(*value))),
        ScalarValue::Int(value) => Some(PhpArrayKey::Int(*value)),
        ScalarValue::Float(value) => float_array_key(*value),
        ScalarValue::String(value) => Some(if is_php_integer_array_key(value) {
            PhpArrayKey::Int(value.parse::<i64>().ok()?)
        } else {
            PhpArrayKey::Str(value.clone())
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
fn float_array_key(value: f64) -> Option<PhpArrayKey> {
    if !value.is_finite() || value.trunc() != value {
        return None;
    }
    // `i64::MAX as f64` rounds up, so the upper bound is exclusive.
    if value >= -(i64::MIN as f64) || value < i64::MIN as f64 {
        return None;
    }
    Some(PhpArrayKey::Int(value as i64))
}
