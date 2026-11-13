//! Purpose:
//! Verifies builtin profile identity across checking, EIR and generated callable bodies.
//!
//! Called from:
//! - Focused AST-to-EIR unit tests.
//!
//! Key details:
//! - A test-only descriptor exposes dropped identity as a result-kind mismatch.
//! - Source modes and all five target ABIs consume the same explicit profile.

use crate::types::PhpType;
use crate::source::SourceMode;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Checker, EIR and generated builtin descriptors agree on profile-sensitive return kinds.
#[test]
fn strict_profile_builtin_inputs_reach_direct_and_callable_lowering_on_all_targets() {
    for profile in [SemanticProfile::PERMISSIVE, SemanticProfile::PERMISSIVE.with(StrictFeature::Bools, true)] {
        for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
            // A public callback contract must not impose its source mode on argument expressions.
            let condition = if mode == SourceMode::Internal { "1" } else { "true" };
            let source = format!("<?php $direct = __semantic_profile_probe(); $callback = __semantic_profile_probe(...); $indirect = $callback({condition} ? 1 : 2); $cuf = call_user_func('__semantic_profile_probe', {condition} ? 1 : 2); $cufa = call_user_func_array('__semantic_profile_probe', [{condition} ? 1 : 2]); echo $direct; echo $indirect; echo $cuf; echo $cufa;");
            let tokens = crate::lexer::tokenize(&source).unwrap();
            let program = crate::parser::parse_with_mode(&tokens, mode).unwrap();
            for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
                let target = crate::codegen::platform::Target::parse(name).unwrap();
                let checked = crate::types::check_with_target_and_options(&program, target,
                    crate::types::CheckOptions { semantic_profile: profile, ..Default::default() },
                    &crate::generics::GenericContext::default()).unwrap();
                let direct = if profile.contains(StrictFeature::Bools) && mode != SourceMode::Internal { PhpType::Int } else { PhpType::Float };
                let indirect = if profile.contains(StrictFeature::Bools) { PhpType::Int } else { PhpType::Float };
                assert_eq!(checked.global_env.get("direct"), Some(&direct), "{name} {profile:?} {mode:?}");
                for variable in ["indirect", "cuf", "cufa"] {
                    assert_eq!(checked.global_env.get(variable), Some(&indirect), "{name} {profile:?} {mode:?} {variable}");
                }
                let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
                crate::ir::validate_module(&module).unwrap();
                crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            }
        }
    }
}

