//! Purpose:
//! Verifies checked native casts accept broad object and interface input domains.
//!
//! Called from:
//! - Selected-profile checker and all-target EIR regressions.
//!
//! Key details:
//! - Partial nominal domains use original-value runtime guards; known invalid classes stay errors.

use crate::codegen_support::platform::Target;
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = "<?php interface NativeCastDomain {} class DomainXml extends SimpleXMLElement implements NativeCastDomain {} function domainObject(object $value): int { return intval($value); } function domainInterface(NativeCastDomain $value): float { return (float)$value; } function domainNullable(?NativeCastDomain $value): int { return (int)$value; } function domainUnion(int|NativeCastDomain $value): int { return (int)$value; } function domainGmp(GMP $value): int { return domainObject($value); }";

/// A selected target keeps broad nominal inputs in the guarded native dispatch graph.
fn verify_domains(name: &str) {
    let (program, _) = super::strict_object_casts::prepare(SOURCE);
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true);
    let target = Target::parse(name).unwrap();
    let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
    let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
    for name in ["domainObject", "domainInterface", "domainNullable", "domainUnion"] {
        let function = module.functions.iter().find(|function| function.name == name).unwrap();
        assert!(function.instructions.iter().any(|inst| inst.op == crate::ir::Op::InstanceOf), "{name}");
    }
    crate::ir::validate_module(&module).unwrap();
    crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
}

/// Closed ordinary classes and arrays cannot acquire a native cast merely from a broad-domain rule.
#[test]
fn strict_object_cast_domains_keep_known_invalid_source_diagnostics() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true);
    for source in ["<?php class ClosedDomain {} function f(ClosedDomain $value): int { return (int)$value; }",
        "<?php function f(array $value): float { return floatval($value); }"] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap_err();
        assert!(error.message.contains("--strict-casts"), "{error:?}");
    }
}

/// Splits real-prelude validation by target to keep the ordinary CI timeout useful.
macro_rules! target_test {
    ($name:ident, $target:literal) => {
        /// Verifies broad original-value cast domains on one supported target.
        #[test]
        fn $name() { verify_domains($target); }
    };
}
target_test!(strict_object_cast_domains_macos, "macos-aarch64");
target_test!(strict_object_cast_domains_ios, "ios-arm64");
target_test!(strict_object_cast_domains_ios_sim, "ios-sim-arm64");
target_test!(strict_object_cast_domains_linux_arm, "linux-aarch64");
target_test!(strict_object_cast_domains_linux_x86, "linux-x86_64");
