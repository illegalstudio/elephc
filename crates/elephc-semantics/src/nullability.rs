//! Purpose:
//! Selects non-null obligations independently of kind membership and coercion.
//!
//! Called from:
//! - Compiler receiver checks and interpreter value consumers.
//!
//! Key details:
//! - Unknown values require a runtime proof; explicit null probes need no proof.

use crate::{SemanticProfile, StrictFeature};

/// Reports a null value reaching an operation that requires a non-null receiver.
pub const NON_NULL_MESSAGE: &str = "--strict-nullability: a non-null value is required";

/// Logical null information supplied by a checker or a runtime cell adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NullState { Null, NonNull, Unknown }

/// The remaining obligation before a non-null operation may consume its value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NullPlan { ExistingBehavior, Proven, Reject, Guard }

/// Keeps nullability independent from all other strict guarantees and null probes.
pub fn plan_non_null(profile: SemanticProfile, state: NullState, probe: bool) -> NullPlan {
    if probe || !profile.contains(StrictFeature::Nullability) {
        return NullPlan::ExistingBehavior;
    }
    match state {
        NullState::Null => NullPlan::Reject,
        NullState::NonNull => NullPlan::Proven,
        NullState::Unknown => NullPlan::Guard,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A null proof is required only by its own flag and only at an ordinary consumer.
    #[test]
    fn nullability_plans_are_independent_and_probe_aware() {
        let strict = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
        for (state, expected) in [(NullState::Null, NullPlan::Reject),
            (NullState::NonNull, NullPlan::Proven), (NullState::Unknown, NullPlan::Guard)] {
            assert_eq!(plan_non_null(strict, state, false), expected);
            assert_eq!(plan_non_null(strict, state, true), NullPlan::ExistingBehavior);
            assert_eq!(plan_non_null(SemanticProfile::PERMISSIVE, state, false), NullPlan::ExistingBehavior);
            assert_eq!(plan_non_null(SemanticProfile::PERMISSIVE.with(StrictFeature::Mixed, true),
                state, false), NullPlan::ExistingBehavior);
        }
    }
}
