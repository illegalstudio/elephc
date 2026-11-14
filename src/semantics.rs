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

pub(crate) mod numeric;
pub(crate) mod conversions;
mod numeric_constants;
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

    /// Public arithmetic checking is available on every target without enabling other numeric flags.
    #[test]
    fn strict_arithmetic_public_checker_targets_are_independent() {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Arithmetic, true);
        let source = "<?php function checkedDivision(int $left, int $right): float { return $left / $right; } function weakBinding(int $value): int { return $value; } echo checkedDivision(4, 2), weakBinding('3');";
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = crate::codegen::platform::Target::parse(name).unwrap();
            let checked = crate::types::check_with_target_and_options(&program, target,
                crate::types::CheckOptions { semantic_profile: profile, ..Default::default() },
                &crate::generics::GenericContext::default()).unwrap();
            assert_eq!(checked.semantic_profile, profile);
            let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
            crate::ir::validate_module(&module).unwrap();
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        }
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

#[cfg(test)]
mod boolean_codegen_tests {
    use super::*;

    /// User boolean guards lower to typed exception paths on all five target ABIs.
    #[test]
    fn strict_profile_bools_eir_guards_emit_for_every_supported_target() {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Bools, true);
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = crate::codegen::platform::Target::parse(name).unwrap();
            let tokens = crate::lexer::tokenize("<?php function strict_guard(mixed $value): void { if ($value) { echo 'yes'; } } strict_guard(true);").unwrap();
            let program = crate::parser::parse_with_mode(&tokens, crate::source::SourceMode::Php).unwrap();
            let checked = crate::types::check_with_target_and_options(&program, target,
                crate::types::CheckOptions { semantic_profile: profile, ..Default::default() },
                &crate::generics::GenericContext::default()).unwrap();
            let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
            assert!(module.functions.iter().any(|function| function.instructions.iter()
                .any(|inst| inst.op == crate::ir::Op::ThrowTypeError)), "{name}");
            let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            assert!(asm.contains("_spl_type_error_class_id"), "{name}");
            assert!(asm.contains("__rt_throw_current"), "{name}");
        }
    }
}
