//! Purpose:
//! Checks abs result contracts under staged profiles without advertising an incomplete CLI flag.
//!
//! Called from:
//! - Focused checker driver unit tests.
//!
//! Key details:
//! - Public callable contracts remain independent of the physical argument source mode.
//! - Runtime integers cannot silently promote after a checked call.

use crate::parser::ast::{Expr, ExprKind};
use crate::source::SourceMode;
use crate::span::Span;
use crate::types::{PhpType, TypeEnv};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Direct and public callable checking agree on checked integer and ordinary float results.
#[test]
fn strict_abs_checker_retains_integer_results_and_public_callable_contracts() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for checked in [false, true] {
            for source_mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
                let mut checker = super::Checker::new(crate::codegen::platform::Target::parse(name).unwrap());
                checker.semantic_profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, checked);
                checker.semantic_source_mode = source_mode;
                for input in [PhpType::Int, PhpType::Float, PhpType::Mixed] {
                    let mut env = TypeEnv::new();
                    env.insert("number".into(), input.clone());
                    let args = [Expr::new(ExprKind::Variable("number".into()), Span::dummy())];
                    let expected = if input == PhpType::Int { PhpType::Mixed } else { input.clone() };
                    let direct = if input == PhpType::Int && checked && source_mode != SourceMode::Internal { PhpType::Int } else { expected.clone() };
                    let callable = if input == PhpType::Int && checked { PhpType::Int } else { expected };
                    assert_eq!(checker.check_builtin("abs", &args, Span::dummy(), &env).unwrap(), Some(direct), "{name} {source_mode:?}");
                    assert_eq!(checker.check_builtin_callable("abs", &args, Span::dummy(), &env).unwrap(), Some(callable), "{name} {source_mode:?}");
                }
            }
        }
    }
}
