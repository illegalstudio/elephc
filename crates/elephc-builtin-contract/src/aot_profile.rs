//! Purpose:
//! Defines the small set of compiler signature compatibility profiles that
//! intentionally differ from the canonical shared builtin contract.
//!
//! Called from:
//! - Documentation and parity consumers that describe the effective AOT surface.
//!
//! Key details:
//! - Entries here are neutral contract data, not compiler lowering hooks.
//! - Absence means AOT consumes the canonical signature exactly.

use crate::{BuiltinContract, BuiltinId, BuiltinSignature, ParamSpec, TypeSpec};

/// Why the compiler intentionally exposes a signature narrower than the
/// canonical shared contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AotSignatureOverrideReason {
    /// An injected prelude currently implements only a strict signature subset.
    PreludeSignatureSubset,
}

/// Selected AOT signature plus an explicit compatibility reason.
#[derive(Clone, Copy, Debug)]
pub struct AotSignatureProfile {
    /// Complete signature accepted by compiled code.
    pub signature: BuiltinSignature,
    /// Reason for divergence, or `None` when the canonical contract is used.
    pub override_reason: Option<AotSignatureOverrideReason>,
}

/// Returns the signature compiled code accepts for one shared contract.
pub fn aot_signature(contract: &BuiltinContract) -> BuiltinSignature {
    aot_signature_profile(contract).signature
}

/// Returns the compiler signature and any deliberate divergence reason.
pub fn aot_signature_profile(contract: &BuiltinContract) -> AotSignatureProfile {
    aot_signature_override(contract.id).unwrap_or_else(|| AotSignatureProfile {
        signature: contract.signature(),
        override_reason: None,
    })
}

/// Finds a deliberate compiler compatibility signature by stable builtin ID.
fn aot_signature_override(id: BuiltinId) -> Option<AotSignatureProfile> {
    if id != BuiltinId::from_canonical_name("hash_init") {
        return None;
    }
    Some(AotSignatureProfile {
        signature: BuiltinSignature {
            params: HASH_INIT_PARAMS,
            variadic: None,
            required_param_count: None,
        },
        override_reason: Some(AotSignatureOverrideReason::PreludeSignatureSubset),
    })
}

const HASH_INIT_PARAMS: &[ParamSpec] = &[ParamSpec {
    name: "algo",
    ty: TypeSpec::Str,
    default: None,
    by_ref: false,
writes: None,
}];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{contracts, lookup};

    /// Verifies ordinary registry builtins keep the canonical compiler signature.
    #[test]
    fn canonical_signature_is_the_default_aot_profile() {
        let contract = lookup("strlen").expect("strlen contract must exist");
        let signature = aot_signature(contract);
        assert_eq!(signature.params.len(), contract.params.len());
        assert_eq!(signature.variadic, contract.variadic);
    }

    /// Verifies the hash prelude's narrower compiler signature is explicit.
    #[test]
    fn hash_init_retains_the_prelude_signature_subset() {
        let contract = lookup("hash_init").expect("hash_init contract must exist");
        let profile = aot_signature_profile(contract);
        assert_eq!(profile.signature.params.len(), 1);
        assert_eq!(profile.signature.params[0].name, "algo");
        assert_eq!(
            profile.override_reason,
            Some(AotSignatureOverrideReason::PreludeSignatureSubset)
        );
    }

    /// Verifies every AOT signature divergence is enumerated and reasoned.
    #[test]
    fn every_aot_signature_override_has_a_reason() {
        let overridden = contracts()
            .iter()
            .filter_map(|contract| {
                aot_signature_profile(contract)
                    .override_reason
                    .map(|reason| (contract.name, reason))
            })
            .collect::<Vec<_>>();
        assert_eq!(overridden.len(), 1);
        assert_eq!(overridden[0].0, "hash_init");
    }
}
