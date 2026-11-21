//! Purpose:
//! Verifies profile-aware keys in native default and Reflection materialization.
//!
//! Called from:
//! - Focused all-target EIR and native ownership regressions.
//!
//! Key details:
//! - Direct, callable and Reflection paths preserve key kind, bytes and append history.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/array_default_keys.php");

/// Lowers the actual selected frontend and optionally runs the complete AST/EIR optimizer.
fn lower(target: Target, strict: bool, optimize: bool, mode: SourceMode) -> crate::ir::Module {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, strict);
    let mut program = crate::parser::parse_with_mode(&crate::lexer::tokenize(SOURCE).unwrap(), mode).unwrap();
    let mut inventory = crate::optimize::reachability::PreludeInventory::new();
    program = crate::core_prelude::inject_if_used(program, false, false, &mut inventory);
    program = crate::object_cast_prelude::inject_with_semantics(program, &mut inventory, profile).unwrap();
    program = crate::name_resolver::resolve(program).unwrap();
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

/// Default metadata and materialization validate and emit on every supported target.
#[test]
fn array_default_keys_lower_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for strict in [false, true] {
            for mode in [SourceMode::Php, SourceMode::Internal] {
                let module = lower(Target::parse(name).unwrap(), strict, false, mode);
                crate::ir::validate_module(&module).unwrap();
                crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            }
        }
    }
}

/// Native defaults retain profile-specific key identity through each metadata consumer.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn array_default_keys_native_preserve_metadata_and_storage() {
    for strict in [false, true] {
        for optimize in [false, true] {
            for mode in [SourceMode::Php, SourceMode::Internal] {
                let module = lower(Target::detect_host(), strict, optimize, mode);
                let output = super::native_module::run(&module);
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                assert!(output.status.success(), "strict={strict}, opt={optimize}: {stdout} {stderr}");
                let keys = if strict { "integer:1=10,string:1=20,string:01=30,|" }
                    else { "integer:1=20,string:01=30,|" };
                let appends = if strict { "string:2=20,integer:0=30,|" }
                    else { "integer:2=20,integer:3=30,|" };
                let spread = if strict { "string:2=20,string:1=10,integer:0=30,|" }
                    else { "integer:2=20,integer:3=10,integer:4=30,|" };
                let expected = format!("{keys}{appends}{appends}{appends}{appends}{keys}{appends}{spread}{spread}");
                assert_eq!(stdout, expected, "strict={strict}, opt={optimize}, mode={mode:?}: {stderr}");
                assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
            }
        }
    }
}
