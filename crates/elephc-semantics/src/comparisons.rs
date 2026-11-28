//! Purpose:
//! Defines noncoercive scalar ordering independently of PHP value storage.
//!
//! Called from:
//! - Compiler source/checker adapters and native/eval comparison boundaries.
//!
//! Key details:
//! - Identity equality remains valid across different kinds.
//! - Ordering requires the same scalar kind and string order is bytewise.

use crate::conversions::ValueKind;

/// Describes whether a comparison can proceed without changing either scalar value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderingPlan { Exact, Guard, Reject }

/// Preserves uncertainty without accepting a known conversion or structural PHP ordering.
pub fn ordering(left: ValueKind, right: ValueKind) -> OrderingPlan {
    if left == ValueKind::Dynamic || right == ValueKind::Dynamic { return OrderingPlan::Guard; }
    if left == right && matches!(left, ValueKind::Integer | ValueKind::Float
        | ValueKind::Boolean | ValueKind::String | ValueKind::Null)
    { OrderingPlan::Exact } else { OrderingPlan::Reject }
}

/// Supplies the same failure text at frontend and execution boundaries.
pub const ORDERING_MESSAGE: &str = "--strict-comparisons requires operands of the same scalar kind";

/// Names the explicit equality syntax rather than silently changing loose operators.
pub const EQUALITY_MESSAGE: &str = "--strict-comparisons requires === or !== instead of loose equality";

/// Rejects implicit callback-result casts at source-selected comparison boundaries.
pub const COMPARATOR_MESSAGE: &str = "--strict-comparisons requires an integer comparator result";

#[cfg(test)]
mod tests {
    use super::*;

    /// Numeric widths, null and formatting conversions never become ordering identities.
    #[test]
    fn strict_comparison_ordering_requires_original_matching_scalar_kinds() {
        let kinds = [ValueKind::Integer, ValueKind::Float, ValueKind::Boolean,
            ValueKind::String, ValueKind::Null, ValueKind::NonScalar];
        for left in kinds {
            for right in kinds {
                assert_eq!(ordering(left, right) == OrderingPlan::Exact,
                    left == right && left != ValueKind::NonScalar, "{left:?} {right:?}");
            }
            assert_eq!(ordering(left, ValueKind::Dynamic), OrderingPlan::Guard);
            assert_eq!(ordering(ValueKind::Dynamic, left), OrderingPlan::Guard);
        }
    }
}
