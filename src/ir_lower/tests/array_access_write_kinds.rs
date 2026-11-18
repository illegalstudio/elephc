//! Purpose:
//! Checks raw ArrayAccess write arguments through shared EIR method dispatch.
//!
//! Called from:
//! - Focused target and native semantic-profile regressions.
//!
//! Key details:
//! - Custom methods, native SPL and WeakMap keep their own key contracts and cleanup.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/array_access_write_kinds.php");
const EXPECTED: &str = include_str!("../../../tests/fixtures/strict/array_access_write_kinds.out");

/// Interface-directed writes validate and emit on every supported target.
#[test]
fn array_access_write_kinds_lower_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for keys in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, keys);
            let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
            crate::ir::validate_module(&module).unwrap();
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        }
    }
}

/// Original offset kinds and both exceptional argument owners survive every execution mode.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn array_access_write_kinds_native_preserve_values_and_owners() {
    for keys in [false, true] {
        for optimize in [false, true] {
            for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
                let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, keys);
                let module = super::semantic_sources::lower(SOURCE, Target::detect_host(), profile, optimize, mode);
                let output = super::native_module::run(&module);
                let stderr = String::from_utf8_lossy(&output.stderr);
                assert!(output.status.success(), "keys={keys} opt={optimize} mode={mode:?}: {:?}; {stderr}", output.status);
                assert_eq!(String::from_utf8_lossy(&output.stdout), EXPECTED.trim_end(), "{stderr}");
                assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
            }
        }
    }
}
