//! Purpose:
//! Checks numeric local membership guards across the supported target matrix.
//!
//! Called from:
//! - Focused strict local EIR and code generation verification.
//!
//! Key details:
//! - A store contract retains Locals ownership without enabling arithmetic or overflow.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Every target lowers catchable local guards after ordinary PHP numeric operations.
#[test]
fn strict_numeric_locals_guard_writes_and_updates_on_every_target() {
    let source = format!("{}\n{}", include_str!("../../../tests/fixtures/strict/numeric_local_contracts.php"),
        include_str!("../../../tests/fixtures/strict/numeric_local_rebinds.php").strip_prefix("<?php").unwrap());
    let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(&source).unwrap(), SourceMode::Php).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        assert!(checked.local_store_contracts.values().filter(|contract|
            contract.feature == StrictFeature::Locals).count() >= 4, "{name}");
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        let function = module.functions.iter().find(|function| function.name == "numericBindings").unwrap();
        assert!(function.instructions.iter().filter(|instruction|
            instruction.op == crate::ir::Op::ThrowTypeError).count() >= 4, "{name}");
        assert!(!module.semantic_profile.contains(StrictFeature::Overflow));
        assert!(!module.semantic_profile.contains(StrictFeature::Coercions));
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
