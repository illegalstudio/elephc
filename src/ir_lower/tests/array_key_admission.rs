//! Purpose:
//! Verifies public exact-key admission and key-producing native runtime boundaries.
//!
//! Called from:
//! - Focused public checker, target emitter and native eval regressions.
//!
//! Key details:
//! - JSON names preserve bytes; serialized keys preserve the format's explicit kind.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const EXPECTED: &str = "json:string=0:1|string=01:2|string=-1:3|\nserialized:string=0:11|integer=0:22|string=01:33|\ninvalid|scratch|thrown|";

/// The ordinary public checker accepts exact keys and rejects original invalid keys in both modes.
#[test]
fn array_key_public_checker_modes_and_targets() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, true);
    for mode in [SourceMode::Php, SourceMode::Lfc] {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(
                "<?php $values = ['1' => 10, 1 => 20, '01' => 30]; echo count($values);").unwrap(), mode).unwrap();
            let checked = crate::types::check_with_target_and_options(&program, target,
                crate::types::CheckOptions { semantic_profile: profile, ..Default::default() },
                &crate::generics::GenericContext::default()).unwrap();
            assert_eq!(checked.semantic_profile, profile);
            let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
            crate::ir::validate_module(&module).unwrap();
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            let invalid = crate::parser::parse_with_mode(&crate::lexer::tokenize(
                "<?php if (false) { $values = [true => 1]; }").unwrap(), mode).unwrap();
            let error = crate::types::check_with_target_and_options(&invalid, target,
                crate::types::CheckOptions { semantic_profile: profile, ..Default::default() },
                &crate::generics::GenericContext::default()).unwrap_err();
            assert!(error.message.contains("--strict-array-keys"), "{name} {mode:?}: {error:?}");
        }
    }
}

/// Runtime producer contracts lower through every supported target.
#[test]
fn array_key_runtime_producers_emit_on_every_target() {
    let source = format!("{}exactKeyProducerObservations();",
        include_str!("../../../tests/fixtures/strict/exact_key_producers.php"));
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower_capturing_arguments(&source, Target::parse(name).unwrap(),
            SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, true), false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Real assemblers accept the eval wrapper guards on every supported native ABI.
#[test]
#[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
fn array_key_eval_runtime_assembles_on_every_target() {
    use std::{io::Write, process::{Command, Stdio}};
    for (name, triple) in [("linux-x86_64", "x86_64-linux-gnu"), ("linux-aarch64", "aarch64-linux-gnu"),
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator")] {
        let mut features = crate::codegen::RuntimeFeatures::none();
        features.eval_bridge = true;
        let assembly = crate::codegen_support::generate_runtime_with_semantics(8_388_608,
            Target::parse(name).unwrap(), features, false, false,
            SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, true));
        let mut child = Command::new("clang").args(["-target", triple, "-x", "assembler", "-c", "-o", "/dev/null", "-"])
            .stdin(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("clang target assembler");
        child.stdin.take().unwrap().write_all(assembly.as_bytes()).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
    }
}

/// Native producers preserve numeric string keys and typed serialized integer keys in both modes.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn array_key_runtime_producers_native_and_eval_owners() {
    let direct = format!("{}exactKeyProducerObservations();",
        include_str!("../../../tests/fixtures/strict/exact_key_producers.php"));
    let fragment = direct.strip_prefix("<?php\n").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let opaque = format!("<?php $code = $argc > 0 ? '{fragment}' : ''; eval($code);");
    for (eval, source) in [(false, direct), (true, opaque)] {
        for optimize in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, true);
            let module = super::semantic_sources::lower_capturing_arguments(&source,
                Target::detect_host(), profile, optimize, SourceMode::Php);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "eval={eval} opt={optimize}: {:?}; stdout={}; {stderr}",
                output.status, String::from_utf8_lossy(&output.stdout));
            assert_eq!(String::from_utf8_lossy(&output.stdout), EXPECTED, "eval={eval} opt={optimize}: {stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "eval={eval} opt={optimize}: {stderr}");
        }
    }
}
