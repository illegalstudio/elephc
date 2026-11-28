//! Purpose:
//! Verifies ordinary initialized-frame reads independently of local type stability.
//!
//! Called from:
//! - Focused staged Initialization emission, execution and diagnostic tests.
//!
//! Key details:
//! - Public admission is covered alongside the dedicated frame-presence fixtures.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/initialized_frame_reads.php");
const EXPECTED: &str = "P:7:7|A:missing:I|NULL:fallback|AE:fallback|NULL|integer|3|0|2|ready|A:none|P:5|0:0|1:1|9|I|9|4|I|4|I|I|P|6:8|boolean:double:string:3|";

/// Every target validates presence branches and a catchable missing-binding failure.
#[test]
fn initialized_frame_reads_emit_on_every_target() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        let function = module.functions.iter().find(|function| function.name == "readOptionalBinding").unwrap();
        assert!(function.flags.tracks_local_binding_state);
        assert!(function.instructions.iter().any(|inst| inst.op == crate::ir::Op::ThrowError));
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Presence guards survive optimization without confusing initialized null or silent probes.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn initialized_frame_reads_native_values_and_owners() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true);
    for (optimize, mode) in [(false, SourceMode::Php), (true, SourceMode::Php),
        (false, SourceMode::Lfc), (true, SourceMode::Lfc)] {
        let module = super::semantic_sources::lower(SOURCE, Target::detect_host(), profile, optimize, mode);
        let output = super::native_module::run(&module);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "opt={optimize}: {stderr}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), EXPECTED, "opt={optimize}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        assert!(!stderr.contains("Undefined variable"), "{stderr}");
    }
}

/// A statically absent ordinary read is refused before native storage is allocated.
#[test]
fn initialized_frame_reads_reject_known_missing_bindings() {
    let source = "<?php echo $notInitialized;";
    let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true);
    let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap_err();
    assert!(error.message.contains("--strict-initialization: variable $notInitialized is not initialized"), "{error:?}");
}

/// Missing bindings fail without relying on a user-written catch to retain Error metadata.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn initialized_frame_reads_fail_without_a_catch() {
    let source = "<?php function readMissingInitialization(bool $write): void { if ($write) { $maybe = 1; } echo $maybe; } readMissingInitialization(false);";
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true);
    let module = super::semantic_sources::lower(source, Target::detect_host(), profile, true, SourceMode::Php);
    let output = super::native_module::run(&module);
    let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(!output.status.success(), "{text}");
    assert!(text.contains("--strict-initialization: variable $maybe is not initialized"), "{text}");
}
