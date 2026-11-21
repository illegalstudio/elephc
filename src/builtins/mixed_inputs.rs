//! Purpose:
//! Attaches reusable logical input domains to builtins with generic storage signatures.
//!
//! Called from:
//! - Builtin homes whose supported PHP input domain is narrower than boxed mixed storage.
//!
//! Key details:
//! - The checker retains inference order and full nominal compatibility.
//! - These domains select Mixed proofs without enabling value conversion guarantees.

use super::semantics::{BuiltinSemanticInput, BuiltinSemantics, BuiltinValidation, MixedInputsFn};
use crate::types::PhpType;

/// Installs an overload-aware input selector without duplicating the public signature.
pub(crate) const fn with_domains(mut semantics: BuiltinSemantics, domains: MixedInputsFn) -> BuiltinSemantics {
    semantics.validation = BuiltinValidation::MixedInputs(domains);
    semantics
}

/// Requires a resource while preserving its existing resource-kind subtype rules.
pub(crate) fn first_resource(input: &BuiltinSemanticInput<'_>) -> Vec<(usize, PhpType)> {
    shared_domains(input)
}

/// Requires the values container while leaving its intentionally mixed elements unrestricted.
pub(crate) fn second_array(input: &BuiltinSemanticInput<'_>) -> Vec<(usize, PhpType)> {
    shared_domains(input)
}

/// Projects the shared operation capabilities into the checker's existing PHP type terms.
pub(crate) fn shared_domains(input: &BuiltinSemanticInput<'_>) -> Vec<(usize, PhpType)> {
    use elephc_builtin_contract::MixedInputDomain as D;
    let Some(contract) = elephc_builtin_contract::lookup(input.name) else { return Vec::new(); };
    contract.mixed_input_domains(input.arg_types.len()).into_iter().map(|(index, domain)| {
        let ty = match domain {
            D::Array => PhpType::php_array(), D::Resource => PhpType::Resource(None),
            D::String => PhpType::Str,
            D::Countable => PhpType::Union(vec![PhpType::php_array(),PhpType::Object("Countable".into())]),
        };
        (index,ty)
    }).collect()
}
