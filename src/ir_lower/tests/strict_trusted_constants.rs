//! Purpose:
//! Verifies a trusted typed constant materializes its PHP result before user strict boundaries.
//!
//! Called from:
//! - Staged native constant producer regressions.
//!
//! Key details:
//! - The raw Internal int initializer legally binds to float before user code reads the value.

use crate::{source::SourceMode, codegen_support::platform::Target};
use elephc_semantics::{SemanticProfile, StrictFeature};

const TRUSTED: &str = "<?php class TrustedConstant { public const float VALUE = 1; } interface TrustedInterface { public const float VALUE = 1; } enum TrustedEnum { case A; public const float VALUE = 1; }";
const USER: &str = "<?php class TrustedChild extends TrustedConstant {} function acceptedConstant(float $value = TrustedConstant::VALUE): float { return $value; } echo is_float(TrustedConstant::VALUE) ? 'C' : 'X'; echo acceptedConstant(), '|'; echo is_float(TrustedChild::VALUE) ? 'H' : 'X'; echo is_float(TrustedInterface::VALUE) ? 'I' : 'X'; echo is_float(TrustedEnum::VALUE) ? 'E' : 'X';";

/// Prepares separate physical source policies without changing source literal kinds.
fn program() -> crate::parser::ast::Program {
    let mut program = crate::parser::parse_with_mode(&crate::lexer::tokenize(TRUSTED).unwrap(), SourceMode::Internal).unwrap();
    program.extend(crate::parser::parse(&crate::lexer::tokenize(USER).unwrap()).unwrap());
    program
}

/// Declared float values materialize in metadata while the source AST retains its original initializer.
#[test]
fn strict_trusted_constants_checked_metadata_retains_bound_kind_and_original_source() {
    let program = program();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    let checked = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap();
    assert!(matches!(checked.classes["TrustedConstant"].constants["VALUE"].kind, crate::parser::ast::ExprKind::FloatLiteral(value) if value == 1.0));
    assert!(matches!(checked.interfaces["TrustedInterface"].constants["VALUE"].kind, crate::parser::ast::ExprKind::FloatLiteral(value) if value == 1.0));
    assert!(matches!(checked.classes["TrustedEnum"].constants["VALUE"].kind, crate::parser::ast::ExprKind::FloatLiteral(value) if value == 1.0));
    let crate::parser::ast::StmtKind::ClassDecl { constants, .. } = &program[0].kind else { panic!("trusted class"); };
    assert!(matches!(constants[0].value.kind, crate::parser::ast::ExprKind::IntLiteral(1)));
    let mut user = program.clone(); user[0].source_mode = SourceMode::Php;
    let error = crate::types::checker::check_staged_semantics(&user, Target::detect_host(), profile).err().unwrap();
    assert!(error.message.contains("strict-coercions"));
}

/// Direct, inherited, interface and enum constants share the same typed literal contract on all targets.
#[test]
fn strict_trusted_constants_emit_bound_values_on_all_targets() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true);
    let program = program();
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// A trusted producer's declared Float result must not be mistaken for its original Int initializer.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_trusted_constants_native_declared_kind_before_user_defaults() {
    let program = program();
    let target = Target::detect_host();
    for profile in [SemanticProfile::PERMISSIVE, SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, true)] {
        let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
        for optimize in [false, true] {
            let _policy = crate::optimize::scoped_semantic_profile(profile);
            let mut program = program.clone();
            if optimize {
                program = crate::optimize::fold_constants_for_target(program, target);
                program = crate::optimize::propagate_constants(program, checked.mixed_storage_local_names(), checked.buffer_read_sites.clone());
                program = crate::optimize::prune_constant_control_flow(program, checked.local_binding_decision_spans());
                program = crate::optimize::normalize_control_flow(program, checked.local_binding_decision_spans());
                program = crate::optimize::eliminate_dead_code(program, checked.local_binding_decision_spans());
            }
            let mut module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
            if optimize { crate::ir_passes::optimize_module(&mut module); }
            let output = super::native_module::run(&module);
            let stdout = String::from_utf8_lossy(&output.stdout); let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "opt={optimize}: {stdout} {stderr}");
            assert_eq!(stdout, "C1|HIE", "opt={optimize}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {stderr}");
        }
    }
}
