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
    // php's `$count` is a by-reference output. The AOT backend caps both at three arguments
    // (`max_args`), so the canonical contract keeps it by value — a by-ref parameter on a
    // statically pure builtin would break `propagate_args` — while the interpreter, which runs
    // Symfony's container passes and writes the count back, binds it by reference.
    if id == BuiltinId::from_canonical_name("str_replace")
        || id == BuiltinId::from_canonical_name("str_ireplace")
    {
        return Some(EvalSignatureProfile {
            signature: BuiltinSignature {
                params: STR_REPLACE_PARAMS,
                variadic: None,
                required_param_count: None,
            },
            override_reason: Some(EvalSignatureOverrideReason::AdditionalByReferenceOutput),
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
    None
}

const IMPLODE_PARAMS: &[ParamSpec] = &[
    ParamSpec {
        name: "separator",
        ty: TypeSpec::Str,
        default: Some(DefaultSpec::Null),
        by_ref: false,
    },
    ParamSpec {
        name: "array",
        ty: TypeSpec::Mixed,
        default: None,
        by_ref: false,
    },
];

/// `str_replace` / `str_ireplace` with php's by-reference `$count` output.
const STR_REPLACE_PARAMS: &[ParamSpec] = &[
    ParamSpec {
        name: "search",
        ty: TypeSpec::Str,
        default: None,
        by_ref: false,
    },
    ParamSpec {
        name: "replace",
        ty: TypeSpec::Str,
        default: None,
        by_ref: false,
    },
    ParamSpec {
        name: "subject",
        ty: TypeSpec::Str,
        default: None,
        by_ref: false,
    },
    ParamSpec {
        name: "count",
        ty: TypeSpec::Mixed,
        default: Some(DefaultSpec::Null),
        by_ref: true,
    },
];

const IS_CALLABLE_PARAMS: &[ParamSpec] = &[
    ParamSpec {
        name: "value",
        ty: TypeSpec::Mixed,
        default: None,
        by_ref: false,
    },
    ParamSpec {
        name: "syntax_only",
        ty: TypeSpec::Bool,
        default: Some(DefaultSpec::Bool(false)),
        by_ref: false,
    },
    ParamSpec {
        name: "callable_name",
        ty: TypeSpec::Mixed,
        default: Some(DefaultSpec::Null),
        by_ref: true,
    },
];

const LOCALTIME_PARAMS: &[ParamSpec] = &[
    ParamSpec {
        name: "timestamp",
        ty: TypeSpec::Mixed,
        default: Some(DefaultSpec::Null),
        by_ref: false,
    },
    ParamSpec {
        name: "associative",
        ty: TypeSpec::Bool,
        default: Some(DefaultSpec::Bool(false)),
        by_ref: false,
    },
];

const NL2BR_PARAMS: &[ParamSpec] = &[
    ParamSpec {
        name: "string",
        ty: TypeSpec::Str,
        default: None,
        by_ref: false,
    },
    ParamSpec {
        name: "use_xhtml",
        ty: TypeSpec::Bool,
        default: Some(DefaultSpec::Bool(true)),
        by_ref: false,
    },
];

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
        // Named rather than counted: a new override must be added here on purpose.
        let mut names = overridden.iter().map(|(name, _)| *name).collect::<Vec<_>>();
        names.sort_unstable();
        assert_eq!(
            names,
            ["implode", "is_callable", "localtime", "nl2br", "str_ireplace", "str_replace"]
        );
    }
}
