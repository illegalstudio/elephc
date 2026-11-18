//! Purpose:
//! Checks original null keys from typed missing reads without integer sentinel ambiguity.
//!
//! Called from:
//! - Focused original-key target and native regressions.
//!
//! Key details:
//! - Explicit casts remain explicit, and ArrayAccess receives the original null.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/array_key_missing_reads.php");
const EXPECTED: &str = include_str!("../../../tests/fixtures/strict/array_key_missing_reads.out");

/// Missing scalar keys and valid sentinel-valued integers emit on every supported target.
#[test]
fn array_key_missing_reads_lower_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, true);
        let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Typed misses reject as null while legitimate integers, casts and object offsets survive.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn array_key_missing_reads_native_preserve_original_null() {
    for coercions in [false, true] {
        for optimize in [false, true] {
            for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
                let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, true)
                    .with(StrictFeature::Coercions, coercions);
                let module = super::semantic_sources::lower(SOURCE, Target::detect_host(), profile, optimize, mode);
                let output = super::native_module::run(&module);
                let stderr = String::from_utf8_lossy(&output.stderr);
                assert!(output.status.success(), "coercions={coercions} opt={optimize} mode={mode:?}: {:?}; {stderr}", output.status);
                assert_eq!(String::from_utf8_lossy(&output.stdout), EXPECTED.trim_end(),
                    "coercions={coercions} opt={optimize} mode={mode:?}: {stderr}");
                assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
            }
        }
    }
}
