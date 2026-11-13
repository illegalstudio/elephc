//! Purpose:
//! Resolves CLI requests into an immutable, versioned semantic profile.
//!
//! Called from:
//! - Compiler option parsing and profile-aware runtime/cache boundaries.
//!
//! Key details:
//! - Explicit feature overrides win over the umbrella in either argument order.
//! - Repeating an explicit override uses the most recent request.

use crate::{StrictFeature, FEATURES};

/// Version of the stable semantic-profile encoding passed through native boundaries.
pub const SEMANTICS_ABI_VERSION: u32 = 1;

/// Resolved semantic guarantees for one compilation or eval context.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SemanticProfile(u64);

/// Refuses unknown feature bits instead of interpreting another profile layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileDecodeError {
    /// Bits not present in this build's feature catalog.
    pub unknown_bits: u64,
}

/// A native or persistent boundary advertised an incompatible semantic encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticAbiError {
    /// The advertised ABI cannot be interpreted by this build.
    VersionMismatch { expected: u32, actual: u64 },
    /// This ABI is understood but its profile refers to unknown guarantees.
    UnknownBits(ProfileDecodeError),
}

impl SemanticProfile {
    /// Preserves the compiler's default PHP-oriented behavior.
    pub const PERMISSIVE: Self = Self(0);

    /// Returns the profile containing every registered strict guarantee.
    pub fn all() -> Self {
        Self(FEATURES.iter().fold(0, |bits, spec| bits | spec.feature.bit()))
    }

    /// Checks whether this profile enables a guarantee.
    pub const fn contains(self, feature: StrictFeature) -> bool {
        self.0 & feature.bit() != 0
    }

    /// Returns an updated profile without mutating its original value.
    pub const fn with(self, feature: StrictFeature, enabled: bool) -> Self {
        Self(if enabled { self.0 | feature.bit() } else { self.0 & !feature.bit() })
    }

    /// Encodes the resolved profile for cache identities and native boundaries.
    pub const fn bits(self) -> u64 { self.0 }

    /// Decodes the version and feature word together at native and persistent boundaries.
    pub fn from_abi(version: u64, bits: u64) -> Result<Self, SemanticAbiError> {
        if version != u64::from(SEMANTICS_ABI_VERSION) {
            return Err(SemanticAbiError::VersionMismatch { expected: SEMANTICS_ABI_VERSION, actual: version });
        }
        Self::from_bits(bits).map_err(SemanticAbiError::UnknownBits)
    }

    /// Decodes only bits known to the current semantic ABI.
    pub fn from_bits(bits: u64) -> Result<Self, ProfileDecodeError> {
        let unknown_bits = bits & !Self::all().bits();
        if unknown_bits != 0 { return Err(ProfileDecodeError { unknown_bits }); }
        Ok(Self(bits))
    }
}

/// Unresolved umbrella and explicit options owned only by the CLI parser.
#[derive(Clone, Copy, Debug, Default)]
pub struct StrictRequests {
    all: bool,
    enabled: u64,
    disabled: u64,
}

impl StrictRequests {
    /// Consumes one exact strict option; returns false for other CLI arguments.
    pub fn apply_arg(&mut self, arg: &str) -> bool {
        if arg == "--strict" {
            self.all = true;
            return true;
        }
        let (name, enabled) = if let Some(name) = arg.strip_prefix("--no-") {
            (name, false)
        } else if let Some(name) = arg.strip_prefix("--") {
            (name, true)
        } else { return false; };
        let Some(feature) = StrictFeature::from_flag(name) else { return false; };
        self.set(feature, enabled);
        true
    }

    /// Records the last explicit request for one feature.
    pub fn set(&mut self, feature: StrictFeature, enabled: bool) {
        self.enabled &= !feature.bit();
        self.disabled &= !feature.bit();
        if enabled { self.enabled |= feature.bit(); } else { self.disabled |= feature.bit(); }
    }

    /// Resolves all requests independently of the umbrella's argument position.
    pub fn resolve(self) -> SemanticProfile {
        let base = if self.all { SemanticProfile::all().bits() } else { 0 };
        SemanticProfile((base | self.enabled) & !self.disabled)
    }
}

#[cfg(test)]
mod abi_tests {
    use super::*;

    /// Versioned profile decoding rejects incompatible versions and unknown bits together.
    #[test]
    fn strict_runtime_profile_abi_decoding_checks_version_and_bits() {
        let version = u64::from(SEMANTICS_ABI_VERSION);
        for profile in [SemanticProfile::PERMISSIVE, SemanticProfile::all()] {
            assert_eq!(SemanticProfile::from_abi(version, profile.bits()), Ok(profile));
        }
        assert!(matches!(SemanticProfile::from_abi(version + 1, 0), Err(SemanticAbiError::VersionMismatch { .. })));
        assert!(matches!(SemanticProfile::from_abi(u64::MAX, 0), Err(SemanticAbiError::VersionMismatch { .. })));
        assert!(matches!(SemanticProfile::from_abi(version, 1 << 63), Err(SemanticAbiError::UnknownBits(_))));
    }
}
