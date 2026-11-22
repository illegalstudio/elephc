//! Purpose:
//! Checks original keys after write operand effects and before array publication.
//!
//! Called from:
//! - Focused semantic-profile target and native regressions.
//!
//! Key details:
//! - Key snapshots, late variable reads and owning RHS values preserve PHP evaluation order.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/array_key_write_guards.php");

/// Write checks emit for both profiles on every supported target.
#[test]
fn array_key_write_guards_lower_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for keys in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, keys);
            let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
            crate::ir::validate_module(&module).unwrap();
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        }
    }
}

/// Rejected keys preserve writes, snapshots, receiver dispatch and RHS cleanup in every mode.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn array_key_write_guards_native_preserve_order_and_owners() {
    for keys in [false, true] {
        for optimize in [false, true] {
            for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
                let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, keys);
                let module = super::semantic_sources::lower(SOURCE, Target::detect_host(), profile, optimize, mode);
                let output = super::native_module::run(&module);
                let stderr = String::from_utf8_lossy(&output.stderr);
                assert!(output.status.success(), "keys={keys} opt={optimize} mode={mode:?}: {:?}; {stderr}", output.status);
                let expected = if keys { "KVE1:2:3:1|KVE1:2:3:1|KVE1:2:3:1|VE1:2|KVEarray|KVENULL|KVEarray|KVWstring|KVDE|GKVE1|ENULL|Eboolean|ENULL|Earray|ENULL|Eboolean|KE1|KE1|Earray|Eboolean|Earray|Earray|" }
                    else { "KVW9:2:3:1|KVW1:9:3:1|KVW1:2:9:1|VW1:9|KVWarray|KVWarray|KVWarray|KVWstring|KVWD|GKVW1|W1array|W1array|W1array|W1array|W1array|W1array|KW2|KW3|Warray|Warray|Warray|Warray|" };
                assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{stderr}");
                assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
            }
        }
    }
}
