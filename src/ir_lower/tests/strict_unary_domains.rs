//! Purpose:
//! Verifies numeric unary and update contracts before folding and across target lowering.
//!
//! Called from:
//! - Focused compiler library tests.
//!
//! Key details:
//! - Staged checking runs the real frontend while public admission remains closed.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Known bad kinds fail before optimization while explicit numeric and string-complement cases survive.
#[test]
fn strict_unary_domain_frontend_preserves_source_kinds_and_typed_update_contracts() {
    for feature in [StrictFeature::Arithmetic, StrictFeature::Coercions] {
        let profile = SemanticProfile::PERMISSIVE.with(feature, true);
        for source in [
            "<?php if (false) { echo -true; }", "<?php const BAD = 'bad'; if (false) { echo -BAD; }",
            "<?php echo -null;", "<?php echo -[1];", "<?php echo ~7.0;",
            "<?php const BAD = false; echo ~BAD;",
            "<?php function typedUnary(string $value): mixed { return -$value; }",
            "<?php function typedUpdate(string $value): void { $value++; }",
            "<?php $value = true; $value++;", "<?php $value = null; --$value;",
        ] {
            let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
            let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).expect_err(source);
            assert!(error.message.contains(feature.flag()), "{source}: {error:?}");
        }
        for source in ["<?php echo -1.5; echo ~7;", "<?php $value = 7; ++$value; $value--;",
            "<?php function dynamicUpdate(mixed &$value): mixed { return $value++; }"] {
            let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
            assert!(crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).is_ok(), "{source}");
        }
        let string = crate::parser::parse(&crate::lexer::tokenize("<?php echo ~'A';").unwrap()).unwrap();
        assert_eq!(crate::types::checker::check_staged_semantics(&string, Target::detect_host(), profile).is_ok(),
            feature == StrictFeature::Coercions);
    }
    let unknown = crate::parser::parse(&crate::lexer::tokenize("<?php if (false) { echo -'bad'; }").unwrap()).unwrap();
    assert!(crate::semantics::validate_source(&unknown, SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true)).is_ok());
}

/// Guards and typed scalar bit-not lower identically on every target with trusted source exemptions.
#[test]
fn strict_unary_domain_eir_targets_optimizers_and_internal_source() {
    let source = "<?php function domainNegate(mixed $value): mixed { return -$value; } function domainBitNot(mixed $value): mixed { return ~$value; } function domainUpdate(mixed &$value): mixed { return $value++; }";
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
            for feature in [StrictFeature::Arithmetic, StrictFeature::Coercions] {
                let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
                let checked = crate::types::checker::check_staged_semantics(&program, target,
                    SemanticProfile::PERMISSIVE.with(feature, true)).unwrap();
                let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
                for optimize in [false, true] {
                    let mut module = module.clone();
                    if optimize { crate::ir_passes::optimize_module(&mut module); }
                    crate::ir::validate_module(&module).unwrap();
                    let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
                    assert!(asm.contains("__rt_mixed_bit_not"), "{name}, {mode:?}, {feature:?}");
                    assert_eq!(asm.contains(&format!("--{} requires", feature.flag())), mode != SourceMode::Internal,
                        "{name}, {mode:?}, {feature:?}, opt={optimize}");
                }
            }
        }
    }
}
