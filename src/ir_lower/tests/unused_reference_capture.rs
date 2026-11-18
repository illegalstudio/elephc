//! Purpose:
//! Verifies observable reference-source creation when its closure result is discarded.
//!
//! Called from:
//! - Focused optimizer and strict-local native regressions.
//!
//! Key details:
//! - Formation creates the binding; the deferred body never executes.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/unused_reference_capture.php");

/// Every supported target retains the statement that creates the reference source.
#[test]
fn unused_reference_capture_source_emits_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for locals in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, locals);
            let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, true, SourceMode::Php);
            crate::ir::validate_module(&module).unwrap();
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        }
    }
}

/// AST and EIR optimization preserve the new name and open stores without running its closure body.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn unused_reference_capture_native_values_and_owners() {
    for mode in [SourceMode::Php, SourceMode::Lfc] {
        for locals in [false, true] {
            for optimize in [false, true] {
                let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, locals);
                let module = super::semantic_sources::lower(SOURCE, Target::detect_host(), profile, optimize, mode);
                let output = super::native_module::run(&module);
                let stderr = String::from_utf8_lossy(&output.stderr);
                assert!(output.status.success(), "{mode:?} locals={locals} opt={optimize}: {:?}; {stderr}", output.status);
                assert_eq!(String::from_utf8_lossy(&output.stdout), "yes|NULL|after|", "{stderr}");
                assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
                assert!(!stderr.contains("Undefined variable"), "{stderr}");
            }
        }
    }
}
