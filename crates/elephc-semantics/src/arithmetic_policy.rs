//! Purpose:
//! Resolves the independent arithmetic guarantees into compact operation metadata.
//!
//! Called from:
//! - Compiler checking/lowering and eval operator adapters.
//!
//! Key details:
//! - This is derived policy, not a second mutable semantic profile or a native ABI.
//! - PHP implementation operations can explicitly retain PERMISSIVE behavior.

use crate::{SemanticProfile, StrictFeature};

/// Checked operation policy small enough to travel in EIR immediates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ArithmeticPolicy(u8);

impl ArithmeticPolicy {
    /// Keeps the existing PHP numeric operation behavior.
    pub const PERMISSIVE: Self = Self(0);

    /// Resolves only numeric guarantees, without enabling any independent feature.
    pub const fn from_profile(profile: SemanticProfile) -> Self {
        Self((profile.contains(StrictFeature::Overflow) as u8)
            | ((profile.contains(StrictFeature::Arithmetic) as u8) << 1)
            | ((profile.contains(StrictFeature::Coercions) as u8) << 2))
    }

    /// Requires integer arithmetic to fail instead of promoting an overflowing result.
    pub const fn checked_overflow(self) -> bool { self.0 & 1 != 0 }

    /// Requires the reviewed numeric operator domains and result signatures.
    pub const fn strict_arithmetic(self) -> bool { self.0 & 2 != 0 }

    /// Refuses implicit PHP value-kind conversions inside arithmetic.
    pub const fn strict_coercions(self) -> bool { self.0 & 4 != 0 }

    /// Returns the semantic guarantees required by this operation's compact policy.
    pub const fn requirements(self) -> SemanticProfile {
        SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, self.checked_overflow())
            .with(StrictFeature::Arithmetic, self.strict_arithmetic())
            .with(StrictFeature::Coercions, self.strict_coercions())
    }

    /// Identifies ordinary PHP operations for legacy optimizer specializations.
    pub const fn is_permissive(self) -> bool { self.0 == 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Selecting one numeric guarantee leaves the other guarantees independently disabled.
    #[test]
    fn strict_arithmetic_policy_resolves_independent_guarantees() {
        for (feature, expected) in [(StrictFeature::Overflow, (true, false, false)),
            (StrictFeature::Arithmetic, (false, true, false)), (StrictFeature::Coercions, (false, false, true))] {
            let policy = ArithmeticPolicy::from_profile(SemanticProfile::PERMISSIVE.with(feature, true));
            assert_eq!((policy.checked_overflow(), policy.strict_arithmetic(), policy.strict_coercions()), expected);
        }
        assert!(ArithmeticPolicy::from_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Bools, true)).is_permissive());
        assert_eq!(std::mem::size_of::<ArithmeticPolicy>(), 1);
    }
}
