//! Purpose:
//! Checks property and nested keys through shared EIR write dispatch.
//!
//! Called from:
//! - Focused target and native semantic-profile regressions.
//!
//! Key details:
//! - Key rejection keeps COW snapshots and retires owned RHS values across every writer.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/property_key_writes.php");
const EXPECTED: &str = include_str!("../../../tests/fixtures/strict/property_key_writes.out");

const STRICT_EXPECTED: &str = include_str!("../../../tests/fixtures/strict/property_key_writes.strict.out");

/// Property and nested key guards validate and emit on every supported target.
#[test]
fn property_key_writes_lower_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for coercions in [false, true] {
            for keys in [false, true] {
                let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, keys)
                    .with(StrictFeature::Coercions, coercions);
                let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
                crate::ir::validate_module(&module).unwrap();
                crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            }
        }
    }
}

/// Property and nested writes preserve exact key checks, COW and operand owners.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn property_key_writes_native_preserve_values_and_owners() {
    for coercions in [false, true] {
        for keys in [false, true] {
            for optimize in [false, true] {
                for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
                    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, keys)
                        .with(StrictFeature::Coercions, coercions);
                    let module = super::semantic_sources::lower(SOURCE, Target::detect_host(), profile, optimize, mode);
                    let output = super::native_module::run(&module);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    assert!(output.status.success(), "coercions={coercions} keys={keys} opt={optimize} mode={mode:?}: {:?}; {stderr}", output.status);
                    assert_eq!(String::from_utf8_lossy(&output.stdout), if keys { STRICT_EXPECTED.trim_end() } else { EXPECTED.trim_end() }, "coercions={coercions} keys={keys} opt={optimize} mode={mode:?}: {stderr}");
                    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
                }
            }
        }
    }
}
