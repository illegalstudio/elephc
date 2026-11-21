//! Purpose:
//! Regresses actual array evaluation at Reflection invocation boundaries.
//!
//! Called from:
//! - Focused all-target and native semantic-profile tests.
//!
//! Key details:
//! - Duplicate values, runtime arrays and named source order survive forwarding.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/reflection_array_evaluation.php");

/// Every supported backend accepts the same evaluated-array forwarding graph.
#[test]
fn reflection_array_evaluation_lowers_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for keys in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, keys);
            let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
            crate::ir::validate_module(&module).unwrap();
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        }
    }
}

/// Forwarded arrays execute once and keep returned heap payloads alive through source cleanup.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn reflection_array_evaluation_native_preserves_effects_and_owners() {
    for keys in [false, true] {
        for optimize in [false, true] {
            for mode in [SourceMode::Php, SourceMode::Internal] {
                let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, keys);
                let module = super::semantic_sources::lower(SOURCE, Target::detect_host(), profile, optimize, mode);
                let output = super::native_module::run(&module);
                let stderr = String::from_utf8_lossy(&output.stderr);
                assert!(output.status.success(), "{:?}: {:?}; {stderr}", output.status, String::from_utf8_lossy(&output.stdout));
                assert_eq!(String::from_utf8_lossy(&output.stdout), "AB22|C33|MN44|PM55|Q66|UV8|Y:1|RT|E2|", "{stderr}");
                assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
            }
        }
    }
}
