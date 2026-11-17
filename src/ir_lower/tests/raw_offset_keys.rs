//! Purpose:
//! Verifies original PHP offset operands through boxed receiver dispatch.
//!
//! Called from:
//! - Focused raw-key ABI and native ownership tests.
//!
//! Key details:
//! - PHP methods receive value snapshots; native key tuples retain their existing protocol.

use crate::codegen_support::platform::Target;
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/raw_offset_keys.php");
const EXPECTED: &str = "boolean;boolean|double;double|NULL;NULL|integer;integer|string;string|string;string|array;array|object;object|string;1|string;64|two|integer;2|1:2|object;C|1|resource;resource|";

/// Uses the actual selected checker without weakening public profile admission.
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

/// Every supported target accepts the original-key operand and snapshot graphs.
#[test]
fn raw_offset_keys_lower_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for strict in [false, true] {
            let module = lower(Target::parse(name).unwrap(), strict, false);
            crate::ir::validate_module(&module).unwrap();
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        }
    }
}

/// Real ELF and Apple assemblers accept both sides of the raw-key private ABI.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn raw_offset_keys_user_and_runtime_assemble_on_every_target() {
    use std::{io::Write, process::{Command, Stdio}};
    for (name, triple) in [("linux-x86_64", "x86_64-linux-gnu"), ("linux-aarch64", "aarch64-linux-gnu"),
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator")] {
        for strict in [false, true] {
            let module = lower(Target::parse(name).unwrap(), strict, false);
            let user = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            let runtime = crate::codegen_support::generate_runtime_with_semantics(8_388_608,
                module.target, module.required_runtime_features, false, false, module.semantic_profile);
            for (family, assembly) in [("user", user), ("runtime", runtime)] {
                let mut child = Command::new("clang").args(["-target", triple, "-x", "assembler", "-c", "-o", "/dev/null", "-"])
                    .stdin(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("clang target assembler");
                child.stdin.take().unwrap().write_all(assembly.as_bytes()).unwrap();
                let output = child.wait_with_output().unwrap();
                assert!(output.status.success(), "{name}, strict={strict}, {family}: {}", String::from_utf8_lossy(&output.stderr));
            }
        }
    }
}

/// Value kinds, numeric strings, COW and getter failures retain their PHP behavior and owners.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn raw_offset_keys_native_preserve_kinds_cow_and_unwind_owners() {
    for strict in [false, true] {
        for optimize in [false, true] {
            let module = lower(Target::detect_host(), strict, optimize);
            let output = super::native_module::run(&module);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "strict={strict}, opt={optimize}: {stdout} {stderr}");
            assert_eq!(stdout, EXPECTED, "strict={strict}, opt={optimize}: {stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        }
    }
}
