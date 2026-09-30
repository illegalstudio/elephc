//! Purpose:
//! Defines the small set of eval-runtime signature compatibility profiles that
//! intentionally differ from the canonical shared builtin contract.
//!
//! Called from:
//! - Magician registry assembly when joining eval hooks to shared contracts.
//!
//! Key details:
//! - Entries here are contract data, not dispatch hooks or runtime behavior.
//! - Absence means the eval runtime consumes the canonical signature exactly.

use crate::{BuiltinContract, BuiltinId, BuiltinSignature, DefaultSpec, ParamSpec, TypeSpec};

/// Why Magician intentionally exposes a signature shape that differs from the
/// canonical shared contract while preserving existing runtime behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvalSignatureOverrideReason {
    /// Eval retains a required parameter after an earlier optional slot.
    NonTrailingRequiredParameter,
    /// Eval materializes a different sentinel/default representation.
    RuntimeDefaultRepresentation,
    /// Eval supports additional optional PHP parameters.
    AdditionalOptionalParameters,
    /// Eval supports an additional optional by-reference output parameter.
    AdditionalByReferenceOutput,
}

/// Selected eval signature plus an explicit transitional-compatibility reason.
#[derive(Clone, Copy, Debug)]
pub struct EvalSignatureProfile {
    /// Complete signature consumed by Magician argument binding.
    pub signature: BuiltinSignature,
    /// Reason for divergence, or `None` when the canonical contract is used.
    pub override_reason: Option<EvalSignatureOverrideReason>,
}

/// Returns the signature Magician must expose for one shared contract.
pub fn eval_signature(contract: &BuiltinContract) -> BuiltinSignature {
    eval_signature_profile(contract).signature
}

/// Returns Magician's signature and the reason for any deliberate divergence.
pub fn eval_signature_profile(contract: &BuiltinContract) -> EvalSignatureProfile {
    eval_signature_override(contract.id).unwrap_or_else(|| EvalSignatureProfile {
        signature: contract.signature(),
        override_reason: None,
    })
}

/// Finds a deliberate eval-runtime compatibility signature by stable builtin ID.
fn eval_signature_override(id: BuiltinId) -> Option<EvalSignatureProfile> {
    if id == BuiltinId::from_canonical_name("implode") {
        return Some(EvalSignatureProfile {
            signature: BuiltinSignature {
                params: IMPLODE_PARAMS,
                variadic: None,
                required_param_count: Some(1),
            },
            override_reason: Some(
                EvalSignatureOverrideReason::NonTrailingRequiredParameter,
            ),
        });
    }
    if id == BuiltinId::from_canonical_name("is_callable") {
        return Some(EvalSignatureProfile {
            signature: BuiltinSignature {
                params: IS_CALLABLE_PARAMS,
                variadic: None,
                required_param_count: None,
            },
            override_reason: Some(EvalSignatureOverrideReason::AdditionalByReferenceOutput),
        });
    }
    if id == BuiltinId::from_canonical_name("localtime") {
        return Some(EvalSignatureProfile {
            signature: BuiltinSignature {
                params: LOCALTIME_PARAMS,
                variadic: None,
                required_param_count: None,
            },
            override_reason: Some(EvalSignatureOverrideReason::RuntimeDefaultRepresentation),
        });
    }
    if id == BuiltinId::from_canonical_name("nl2br") {
        return Some(EvalSignatureProfile {
            signature: BuiltinSignature {
                params: NL2BR_PARAMS,
                variadic: None,
                required_param_count: None,
            },
            override_reason: Some(EvalSignatureOverrideReason::AdditionalOptionalParameters),
        });
    }
    if id == BuiltinId::from_canonical_name("preg_match") {
        return Some(EvalSignatureProfile {
            signature: BuiltinSignature {
                params: PREG_MATCH_PARAMS,
                variadic: None,
                required_param_count: None,
            },
            override_reason: Some(EvalSignatureOverrideReason::AdditionalOptionalParameters),
        });
    }
    if id == BuiltinId::from_canonical_name("preg_match_all") {
        return Some(EvalSignatureProfile {
            signature: BuiltinSignature {
                params: PREG_MATCH_ALL_PARAMS,
                variadic: None,
                required_param_count: None,
            },
            override_reason: Some(EvalSignatureOverrideReason::AdditionalByReferenceOutput),
        });
    }
    None
}

const IMPLODE_PARAMS: &[ParamSpec] = &[
    ParamSpec {
        name: "separator",
        ty: TypeSpec::Str,
        default: Some(DefaultSpec::Null),
        by_ref: false,
writes: None,
    },
    ParamSpec {
        name: "array",
        ty: TypeSpec::Mixed,
        default: None,
        by_ref: false,
writes: None,
    },
];

const IS_CALLABLE_PARAMS: &[ParamSpec] = &[
    ParamSpec {
        name: "value",
        ty: TypeSpec::Mixed,
        default: None,
        by_ref: false,
writes: None,
    },
    ParamSpec {
        name: "syntax_only",
        ty: TypeSpec::Bool,
        default: Some(DefaultSpec::Bool(false)),
        by_ref: false,
writes: None,
    },
    ParamSpec {
        name: "callable_name",
        ty: TypeSpec::Mixed,
        default: Some(DefaultSpec::Null),
        by_ref: true,
writes: None,
    },
];

const LOCALTIME_PARAMS: &[ParamSpec] = &[
    ParamSpec {
        name: "timestamp",
        ty: TypeSpec::Mixed,
        default: Some(DefaultSpec::Null),
        by_ref: false,
writes: None,
    },
    ParamSpec {
        name: "associative",
        ty: TypeSpec::Bool,
        default: Some(DefaultSpec::Bool(false)),
        by_ref: false,
writes: None,
    },
];

const NL2BR_PARAMS: &[ParamSpec] = &[
    ParamSpec {
        name: "string",
        ty: TypeSpec::Str,
        default: None,
        by_ref: false,
writes: None,
    },
    ParamSpec {
        name: "use_xhtml",
        ty: TypeSpec::Bool,
        default: Some(DefaultSpec::Bool(true)),
        by_ref: false,
writes: None,
    },
];

const PREG_MATCH_PARAMS: &[ParamSpec] = &[
    ParamSpec {
        name: "pattern",
        ty: TypeSpec::Str,
        default: None,
        by_ref: false,
writes: None,
    },
    ParamSpec {
        name: "subject",
        ty: TypeSpec::Str,
        default: None,
        by_ref: false,
writes: None,
    },
    ParamSpec {
        name: "matches",
        ty: TypeSpec::Mixed,
        default: Some(DefaultSpec::EmptyArray),
        by_ref: true,
writes: None,
    },
    ParamSpec {
        name: "flags",
        ty: TypeSpec::Int,
        default: Some(DefaultSpec::Int(0)),
        by_ref: false,
writes: None,
    },
];

const PREG_MATCH_ALL_PARAMS: &[ParamSpec] = PREG_MATCH_PARAMS;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lookup;

    /// Verifies canonical signatures pass through unchanged when no eval profile exists.
    #[test]
    fn canonical_signature_is_the_default_eval_profile() {
        let contract = lookup("strlen").expect("strlen contract must exist");
        let signature = eval_signature(contract);
        assert_eq!(signature.params.len(), contract.params.len());
        assert_eq!(signature.variadic, contract.variadic);
    }

    /// Verifies deliberate eval-only parameters remain explicit shared contract data.
    #[test]
    fn eval_profile_retains_runtime_signature_extensions() {
        let contract = lookup("is_callable").expect("is_callable contract must exist");
        let profile = eval_signature_profile(contract);
        let signature = profile.signature;
        assert_eq!(signature.params.len(), 3);
        assert!(signature.params[2].by_ref);
        assert_eq!(signature.required_param_count(), 1);
        assert_eq!(
            profile.override_reason,
            Some(EvalSignatureOverrideReason::AdditionalByReferenceOutput)
        );
    }

    /// Verifies every compatibility profile carries a non-empty machine-readable reason.
    #[test]
    fn every_eval_signature_override_has_a_reason() {
        let overridden = crate::contracts()
            .iter()
            .filter_map(|contract| {
                eval_signature_profile(contract)
                    .override_reason
                    .map(|reason| (contract.name, reason))
            })
            .collect::<Vec<_>>();
        assert_eq!(overridden.len(), 6);
        assert_eq!(overridden[0].0, "implode");
        assert_eq!(overridden[5].0, "preg_match_all");
    }
}
