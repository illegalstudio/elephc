//! Purpose:
//! Checks reference key lifetime through calls and escaped promoted property aliases.
//!
//! Called from:
//! - Focused native, opaque eval, and supported-target strict profile regressions.
//!
//! Key details:
//! - Keys retain integer/string kind and embedded NUL bytes after original operands retire.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Builds shared observations with distinct PHP-normalized and exact-key expectations.
fn source(exact: bool) -> (String, &'static str) {
    let mut source = include_str!("../../../tests/fixtures/strict/owned_reference_keys.php").to_string();
    source.push_str("ownedKeyObservations();");
    let expected = if exact { "31/41|31/41|7|8|9/6|10|31/41|31/41|" }
        else { "41/41|41/41|7|8|9/9|10|31/41|31/41|" };
    (source, expected)
}

/// Every target lowers owned key observations without collapsing exact string keys.
#[test]
fn owned_reference_keys_emit_on_every_target() {
    let (source, _) = source(true);
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower_capturing_arguments(&source, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Real eval may keep promoted aliases after constructor key operands and scopes have retired.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn owned_reference_keys_native_eval_escape_and_owners() {
    let (fragment, expected) = source(false);
    let fragment = fragment.strip_prefix("<?php\n").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php $code = $argc > 0 ? '{fragment}' : ''; eval($code);");
    for coordinated in [false, true] {
        for optimize in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, coordinated);
            let module = super::semantic_sources::lower_capturing_arguments(&source, Target::detect_host(), profile, optimize, SourceMode::Php);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "casts={coordinated} opt={optimize}: {:?}; {stderr}", output.status);
            assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "casts={coordinated} opt={optimize}: {stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "casts={coordinated} opt={optimize}: {stderr}");
        }
    }
}
