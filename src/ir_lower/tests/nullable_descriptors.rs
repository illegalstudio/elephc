//! Purpose:
//! Verifies original-null contracts at runtime-selected native callable boundaries.
//!
//! Called from:
//! - Focused staged Nullability native and all-target regressions.
//!
//! Key details:
//! - Higher-order calls and dynamic spread containers retain descriptor dispatch.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Regular, named and variadic descriptor arguments reject null before scalar adaptation.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn nullable_descriptor_parameters_preserve_original_values_and_owners() {
    let source = include_str!("../../../tests/fixtures/strict/nullable_descriptors.php");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for optimize in [false, true] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::detect_host(), profile, optimize, SourceMode::Php);
        let output = super::native_module::run(&module);
        let stdout = String::from_utf8_lossy(&output.stdout); let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "optimize={optimize}: {stdout}; {stderr}");
        assert_eq!(stdout, "SFIBNVWGHLP|7:NULL:NULL:NULL", "{stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    }
}

/// Public descriptor null obligations lower through the same reusable EIR on every target.
#[test]
fn nullable_descriptor_parameters_emit_on_every_target() {
    let source = include_str!("../../../tests/fixtures/strict/nullable_descriptors.php");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Nullability, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower_capturing_arguments(source, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
