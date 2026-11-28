//! Purpose:
//! Verifies overwritten unpack values still execute before call binding.
//!
//! Called from:
//! - Focused all-target and native source-order regressions.
//!
//! Key details:
//! - Normal calls and Reflection consume one evaluated array with last-value semantics.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/duplicate_unpack_values.php");

/// Declining static expansion preserves valid graphs on every supported target.
#[test]
fn duplicate_unpack_values_lower_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for keys in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, keys);
            let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
            crate::ir::validate_module(&module).unwrap();
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        }
    }
}

/// Every overwritten value executes once and an earlier throw stops later values and invocation.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn duplicate_unpack_values_native_preserve_effects_throws_and_owners() {
    for keys in [false, true] {
        for optimize in [false, true] {
            for mode in [SourceMode::Php, SourceMode::Internal] {
                let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, keys);
                let module = super::semantic_sources::lower(SOURCE, Target::detect_host(), profile, optimize, mode);
                let output = super::native_module::run(&module);
                let stderr = String::from_utf8_lossy(&output.stderr);
                assert!(output.status.success(), "{:?}: {:?}; {stderr}", output.status, String::from_utf8_lossy(&output.stdout));
                assert_eq!(String::from_utf8_lossy(&output.stdout), "AB2|TC|DEK4|FGG:2|IJK3:2|", "{stderr}");
                assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
            }
        }
    }
}

/// Runtime array construction preserves lossy float-key diagnostics before invocation.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn unpack_float_key_warning_stays_after_value_evaluation() {
    let source = r#"<?php
function floatUnpackValue(): int { echo 'H'; return 5; }
function floatUnpackRead(int $value): void { echo $value, '|'; }
set_error_handler(function($severity, $message): bool { echo 'W'; return true; });
floatUnpackRead(...[1.5 => floatUnpackValue()]);
restore_error_handler();
"#;
    for optimize in [false, true] {
        for mode in [SourceMode::Php, SourceMode::Internal] {
            let module = super::semantic_sources::lower(source, Target::detect_host(), SemanticProfile::PERMISSIVE, optimize, mode);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "{stderr}");
            assert_eq!(String::from_utf8_lossy(&output.stdout), "HW5|", "{stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        }
    }
}
