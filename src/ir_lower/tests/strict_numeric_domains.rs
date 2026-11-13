//! Purpose:
//! Verifies strict numeric operand guards across source policies and supported targets.
//!
//! Called from:
//! - Focused compiler library tests.
//!
//! Key details:
//! - Domain guards survive EIR optimization without implicitly enabling overflow.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Both strict numeric domains lower on every target and exempt trusted implementation source.
#[test]
fn strict_numeric_domain_eir_targets_profiles_and_source_modes() {
    let source = "<?php function domainAdd(mixed $a, mixed $b): mixed { return $a + $b; } function domainPow(mixed $a, mixed $b): mixed { return $a ** $b; } function domainMod(mixed $a, mixed $b): int { return $a % $b; }";
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        for feature in [StrictFeature::Arithmetic, StrictFeature::Coercions] {
            for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
                let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
                let mut checked = crate::types::check_with_target(&program, target).unwrap();
                checked.semantic_profile = SemanticProfile::PERMISSIVE.with(feature, true);
                let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
                for optimize in [false, true] {
                    let mut module = module.clone();
                    if optimize { crate::ir_passes::optimize_module(&mut module); }
                    crate::ir::validate_module(&module).unwrap();
                    let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
                    assert_eq!(asm.contains(&format!("--{} requires", feature.flag())), mode != SourceMode::Internal,
                        "{name}, {feature:?}, {mode:?}, opt={optimize}");
                    assert_eq!(asm.contains("integer power requires a nonnegative exponent"),
                        feature == StrictFeature::Arithmetic && mode != SourceMode::Internal,
                        "{name}, {feature:?}, {mode:?}, opt={optimize}");
                    assert!(!module.semantic_profile.contains(StrictFeature::Overflow));
                }
            }
        }
    }
}
