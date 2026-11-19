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
pub(crate) fn first_resource(_input: &BuiltinSemanticInput<'_>) -> Vec<(usize, PhpType)> {
    vec![(0, PhpType::Resource(None))]
}

/// Requires the values container while leaving its intentionally mixed elements unrestricted.
pub(crate) fn second_array(_input: &BuiltinSemanticInput<'_>) -> Vec<(usize, PhpType)> {
    vec![(1, PhpType::php_array())]
}
