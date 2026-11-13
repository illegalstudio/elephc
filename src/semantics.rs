//! Purpose:
//! Exposes the shared semantic profile at compiler integration boundaries.
//!
//! Called from:
//! - CLI resolution, type checking, EIR lowering, and runtime generation.
//!
//! Key details:
//! - Profiles are immutable; syntax restrictions remain owned by php_only.

#[allow(unused_imports)]
pub use elephc_semantics::{SemanticProfile, StrictFeature, StrictRequests, FEATURES};

mod validation;
pub use validation::validate_source;

/// Lists guarantees whose compiler/runtime implementation is still being integrated.
/// Rejecting these during staged implementation prevents silently permissive binaries.
pub fn pending_guarantees(profile: SemanticProfile) -> Vec<&'static str> {
    FEATURES.iter().filter(|spec| profile.contains(spec.feature)
        && spec.feature.bit() & elephc_semantics::INTEGRATED_FEATURE_BITS == 0)
        .map(|spec| spec.flag).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Checked guarantees survive AST-to-EIR lowering without consulting CLI state.
    #[test]
    fn strict_profile_eir_inherits_checked_guarantees() {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true)
            .with(StrictFeature::Opcache, true);
        let tokens = crate::lexer::tokenize("<?php $value = 1; echo $value;").unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        let target = crate::codegen::platform::Target::detect_host();
        let checked = crate::types::check_with_target_and_options(&program, target,
            crate::types::CheckOptions { semantic_profile: profile, ..Default::default() },
            &crate::generics::GenericContext::default()).unwrap();
        assert_eq!(checked.semantic_profile, profile);
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        assert_eq!(module.semantic_profile, profile);
    }

    /// Incomplete feature integration is visible to library callers as well as the CLI.
    #[test]
    fn strict_profile_pending_features_are_not_silent_library_noops() {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
        let error = crate::types::check_with_target_and_options(&Vec::new(),
            crate::codegen::platform::Target::detect_host(),
            crate::types::CheckOptions { semantic_profile: profile, ..Default::default() },
            &crate::generics::GenericContext::default()).unwrap_err();
        assert!(error.message.contains("strict-overflow"));
    }
}
