//! Purpose:
//! Checks raw constructor ABI string conversion owners and their unwind records.
//!
//! Called from:
//! - Focused target emission and native constructor ownership regressions.
//!
//! Key details:
//! - Formatting survives later callbacks, throws and reference writeback.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/constructor_string_owners.php");

/// Raw ABI conversions publish valid string owners on every supported target.
#[test]
fn constructor_string_owners_lower_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for coercions in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Coercions, coercions);
            let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Internal);
            crate::ir::validate_module(&module).unwrap();
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        }
    }
}

/// Borrowed formatting and owned conversions retire after normal calls and every throwing phase.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn constructor_string_owners_native_preserve_values_and_cleanup() {
    for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
        for optimize in [false, true] {
            let module = super::semantic_sources::lower(SOURCE, Target::detect_host(), SemanticProfile::PERMISSIVE, optimize, mode);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "{mode:?} opt={optimize}: {:?}: {:?}; {stderr}", output.status, String::from_utf8_lossy(&output.stdout));
            assert_eq!(String::from_utf8_lossy(&output.stdout), "[vv][2][][1.5][mm][lit]|1237:late|QT|ET|rr:2|ss:2|1234:5:6:late|", "{mode:?} opt={optimize}: {stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{mode:?} opt={optimize}: {stderr}");
        }
    }
}
