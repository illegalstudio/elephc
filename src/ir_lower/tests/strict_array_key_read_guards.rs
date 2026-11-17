//! Purpose:
//! Verifies catchable exact-key guards for reads, probes and associative construction.
//!
//! Called from:
//! - Focused all-target EIR and native ownership regressions.
//!
//! Key details:
//! - Original operands survive receiver dispatch, optimization and throwing cleanup.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/array_key_read_guards.php");

/// Lowers the actual selected frontend and optionally runs the complete AST/EIR optimizer.
fn lower(target: Target, strict: bool, optimize: bool, mode: SourceMode) -> crate::ir::Module {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, strict);
    let mut program = crate::parser::parse_with_mode(&crate::lexer::tokenize(SOURCE).unwrap(), mode).unwrap();
    let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
    let _profile = crate::optimize::scoped_semantic_profile(profile);
    if optimize {
        program = crate::optimize::fold_constants_for_target(program, target);
        program = crate::optimize::propagate_constants(program, checked.mixed_storage_local_names(), checked.buffer_read_sites.clone());
        program = crate::optimize::prune_constant_control_flow(program, checked.local_binding_decision_spans());
        program = crate::optimize::normalize_control_flow(program, checked.local_binding_decision_spans());
        program = crate::optimize::eliminate_dead_code(program, checked.local_binding_decision_spans());
    }
    let mut module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
    if optimize { crate::ir_passes::optimize_module(&mut module); }
    module
}

/// Original-key guard graphs validate and emit on every supported target.
#[test]
fn strict_array_key_read_guards_lower_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for strict in [false, true] {
            for mode in [SourceMode::Php, SourceMode::Internal] {
                let module = lower(Target::parse(name).unwrap(), strict, false, mode);
                crate::ir::validate_module(&module).unwrap();
                crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
                if strict {
                    assert!(module.functions.iter().flat_map(|function| &function.instructions)
                        .any(|inst| inst.op == crate::ir::Op::ThrowTypeError));
                }
            }
        }
    }
}

/// Key failures preserve receiver semantics, discard effects and partial-construction owners.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_array_key_read_guards_native_preserve_domains_and_owners() {
    for strict in [false, true] {
        for optimize in [false, true] {
            for mode in [SourceMode::Php, SourceMode::Internal] {
                let module = lower(Target::detect_host(), strict, optimize, mode);
                let output = super::native_module::run(&module);
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                assert!(output.status.success(), "strict={strict}, opt={optimize}: {stdout} {stderr}");
                let expected = if strict { "K,K,K,|K|20:30:40|b|boolean:object|K|DK|K|K:K|" }
                    else { "10,11,12,|10|30:30:40|b|boolean:object|A|AD|5|Y:N|" };
                assert_eq!(stdout, expected, "strict={strict}, opt={optimize}, mode={mode:?}: {stderr}");
                assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
            }
        }
    }
}
