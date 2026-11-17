//! Purpose:
//! Verifies exact array key identity across AST and EIR optimization.
//!
//! Called from:
//! - Focused staged-profile compiler and native execution tests.
//!
//! Key details:
//! - The selected checker runs before folding; optimizer mode must not change results.

use crate::codegen_support::platform::Target;
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/array_key_folding.php");

/// Runs the full selected frontend and the production AST optimization sequence.
fn lower(target: Target, strict: bool, optimize: bool) -> crate::ir::Module {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, strict);
    let mut program = crate::parser::parse(&crate::lexer::tokenize(SOURCE).unwrap()).unwrap();
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

/// Folded and unfurled keys remain representable on every supported target.
#[test]
fn strict_array_key_folding_lowers_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for strict in [false, true] {
            for optimize in [false, true] {
                let module = lower(Target::parse(name).unwrap(), strict, optimize);
                crate::ir::validate_module(&module).unwrap();
                crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            }
        }
    }
}

/// Constant and propagated array reads agree with runtime reads without affecting string offsets.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_array_key_folding_native_matches_runtime_identity() {
    for strict in [false, true] {
        for optimize in [false, true] {
            let module = lower(Target::detect_host(), strict, optimize);
            let output = super::native_module::run(&module);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "strict={strict}, opt={optimize}: {stdout} {stderr}");
            let expected = if strict { "int|str|new|pad|int|str|inner-int|nul|b" }
                else { "str|str|int|pad|str|str|inner-str|nul|b" };
            assert_eq!(stdout, expected, "strict={strict}, opt={optimize}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        }
    }
}
