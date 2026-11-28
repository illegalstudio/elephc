//! Purpose:
//! Verifies initialized reads through dynamic object containers and physical reflection.
//!
//! Called from:
//! - Focused staged Initialization property regressions.
//!
//! Key details:
//! - Intentional probes stay silent and missing reads precede mutations.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/initialized_object_reads.php");
const EXPECTED: &str = "NULL|I|I|AE:missing|I|I:A|I|I|N|";

/// Every target lowers the same dynamic container and reflection boundaries.
#[test]
fn initialized_object_reads_emit_on_every_target() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower_capturing_arguments(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Initialized null, missing dynamic names and reads before updates stay distinct.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn initialized_object_reads_native_values_and_owners() {
    for (initialization, locals) in [(true, false), (true, true), (false, false)] {
        for (optimize, mode) in [(false, SourceMode::Php), (true, SourceMode::Php),
            (false, SourceMode::Lfc), (true, SourceMode::Lfc)] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, initialization)
                .with(StrictFeature::Locals, locals);
            let module = super::semantic_sources::lower_capturing_arguments(SOURCE, Target::detect_host(), profile, optimize, mode);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "stdout={}; {stderr}", String::from_utf8_lossy(&output.stdout));
            assert_eq!(String::from_utf8_lossy(&output.stdout), if initialization { EXPECTED } else { "NULL|bad|bad|AE:missing|bad|bad:P|bad|bad|N|" }, "{stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
            if initialization { assert!(!stderr.contains("Warning:"), "{stderr}"); }
        }
    }
}

/// Named stdClass reads share the interpreter's property-name diagnostic.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn initialized_object_reads_named_errors_use_shared_message() {
    let source = "<?php $object=new stdClass();try{echo $object->value;}catch(Error $error){echo $error->getMessage();}";
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true);
    let module = super::semantic_sources::lower_capturing_arguments(source, Target::detect_host(), profile, false, SourceMode::Php);
    let output = super::native_module::run(&module);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), elephc_semantics::initialization::property_message("value"));
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
}

/// A computed owned key survives until a later variable receiver read throws.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn initialized_object_reads_key_before_missing_receiver_retires_owners() {
    let source = "<?php function initializedOwnedName():string{echo 'K';return str_repeat('x',2);} $object=new stdClass();if($argc>0){unset($object);}try{echo $object->{initializedOwnedName()};}catch(Error $error){echo 'I';}";
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true);
    let module = super::semantic_sources::lower_capturing_arguments(source, Target::detect_host(), profile, false, SourceMode::Php);
    let output = super::native_module::run(&module);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "KI", "{stderr}");
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
}
