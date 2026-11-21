//! Purpose:
//! Checks original key kinds before nested foreach fetches separate array storage.
//!
//! Called from:
//! - Focused all-target and native semantic-profile regressions.
//!
//! Key details:
//! - Key temporaries, exact string identity and copy-on-write keep independent contracts.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/array_key_foreach_guards.php");

/// Fetch-for-write checks emit on every supported target without changing borrowed elements.
#[test]
fn array_key_foreach_guards_lower_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for keys in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, keys);
            let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
            crate::ir::validate_module(&module).unwrap();
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        }
    }
}

/// Rejected keys retire owned cells and leave nested aliases unchanged in every source mode.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn array_key_foreach_guards_native_preserve_domains_and_owners() {
    for keys in [false, true] {
        for optimize in [false, true] {
            for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
                let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, keys);
                let module = super::semantic_sources::lower(SOURCE, Target::detect_host(), profile, optimize, mode);
                let output = super::native_module::run(&module);
                let stderr = String::from_utf8_lossy(&output.stderr);
                assert!(output.status.success(), "keys={keys} opt={optimize} mode={mode:?}: {:?}; {stderr}", output.status);
                let expected = if keys { "QK1:2:4|QK1:2:4|QK1:2:4|Q2:13:15|QK7:7|ET|QK1:2|QR1:2|" }
                    else { "QW2:3:4|QW1:4:4|QW1:3:5|Q13:13:15|QW8:7|ET|QR9:3|QR1:9|" };
                assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{stderr}");
                assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
            }
        }
    }
}

/// Reusing a foreach alias after unset promotes a real boxed null on every target.
#[test]
fn reused_foreach_reference_emits_on_every_target() {
    let source = include_str!("../../../tests/fixtures/strict/reused_foreach_reference.php");
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, true);
        let module = super::semantic_sources::lower(source, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// An unset alias can be rebound without dereferencing a missing sentinel or leaking owners.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn reused_foreach_reference_native_preserves_values_and_owners() {
    let source = include_str!("../../../tests/fixtures/strict/reused_foreach_reference.php");
    for keys in [false, true] {
        for optimize in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, keys);
            let module = super::semantic_sources::lower(source, Target::detect_host(), profile, optimize, SourceMode::Php);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "keys={keys} opt={optimize}: {:?}; {stderr}", output.status);
            let expected = if keys { "QKQKQK|1:2:4|2:13:15|QK7:7|ET|" }
                else { "QWQWQW|2:4:5|14:14:15|QW8:7|ET|" };
            assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        }
    }
}
