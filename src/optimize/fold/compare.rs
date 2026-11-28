//! Purpose:
//! Adapts shared scalar comparisons to the active compiler semantic profile.
//!
//! Called from:
//! - Constant folding, scalar casts and switch selection.
//!
//! Key details:
//! - PHP ordering and numeric-string classification have one dependency-neutral owner.

use std::cmp::Ordering;
use super::scalar::ScalarValue;
pub(in crate::optimize) use elephc_semantics::scalar_compare::{PhpNumeric, php_numeric_prefix};

/// Preserves the compiler's source-selected comparison policy over shared scalar values.
pub(in crate::optimize) fn compare_scalars(left: &ScalarValue, right: &ScalarValue) -> Option<Ordering> {
    elephc_semantics::scalar_compare::compare_scalars(left, right, super::super::semantic_context::active_profile())
}

/// Applies PHP equality using the same shared comparison and source policy as ordering.
pub(in crate::optimize) fn loose_eq_values(left: &ScalarValue, right: &ScalarValue) -> Option<bool> {
    elephc_semantics::scalar_compare::loose_eq_values(left, right, super::super::semantic_context::active_profile())
}
