//! Purpose:
//! Checks evaluated array validation and ownership at reflected constructors.
//!
//! Called from:
//! - Focused all-target and native semantic-profile regression tests.
//!
//! Key details:
//! - Receiver rebinding, runtime arrays, default arguments and exceptions share one path.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/reflection_constructor_arrays.php");

/// Evaluated constructor arrays validate and emit for all supported targets.
#[test]
fn reflection_constructor_arrays_lower_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for keys in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, keys);
            let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
            crate::ir::validate_module(&module).unwrap();
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        }
    }
}

/// Constructor array validation preserves receiver identity and retires every dispatch owner.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn reflection_constructor_arrays_native_preserve_kinds_and_owners() {
    for keys in [false, true] {
        for optimize in [false, true] {
            for mode in [SourceMode::Php, SourceMode::Internal] {
                let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, keys);
                let module = super::semantic_sources::lower(SOURCE, Target::detect_host(), profile, optimize, mode);
                let output = super::native_module::run(&module);
                let stderr = String::from_utf8_lossy(&output.stderr);
                assert!(output.status.success(), "keys={keys} opt={optimize} mode={mode:?}: {:?}; {stderr}", output.status);
                assert_eq!(String::from_utf8_lossy(&output.stdout), "K12|K13|T|K7|K9|K10|K7|K11|RK4|ConstructorArrayOther|AT|ET|1|Y:2|", "{stderr}");
                assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
            }
        }
    }
}
