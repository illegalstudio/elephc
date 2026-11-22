//! Purpose:
//! Checks parent publication before original-key rejection and false-conversion callbacks.
//!
//! Called from:
//! - Staged original array-key native and supported-target regressions.
//!
//! Key details:
//! - Key and RHS effects run once before the selected parent's materialization.
//! - Exact-key rejection leaves null unchanged after converting false with PHP warning priority.
//! - Callback writes remain visible when an in-flight receiver becomes detached.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Builds parent/key combinations with independent PHP and exact-policy expectations.
fn source(exact: bool) -> (String, String) {
    let mut source = include_str!("../../../tests/fixtures/strict/array_parent_key_order.php").to_string();
    let mut expected = String::new();
    for false_parent in [false, true] {
        for (key, invalid, legacy_invalid, kind, text) in [
            ("true", true, false, "integer", "1"), ("0.5", true, false, "integer", "0"),
            ("null", true, false, "string", ""), ("[]", true, true, "", ""),
            ("new ParentKeyOperand()", true, true, "", ""), ("0", false, false, "integer", "0"),
            ("'0'", false, false, if exact { "string" } else { "integer" }, "0"),
            ("'01'", false, false, "string", "01"), ("''", false, false, "string", ""),
        ] {
            source.push_str(&format!("parentKeyWrite({key}, {false_parent}, 0);\n"));
            expected.push_str("K|R|");
            if exact && invalid {
                expected.push_str(if false_parent { "F:array:0|T|array:0|" } else { "T|NULL:-1|" });
            } else {
                if false_parent { expected.push_str("F:array:0|"); }
                if legacy_invalid { expected.push_str("T|array:0|"); }
                else { expected.push_str(&format!("ok|array:1|{kind}:{text}:9|")); }
            }
        }
    }
    for false_parent in [false, true] {
        for (key, invalid) in [("0", false), ("true", true)] {
            source.push_str(&format!("parentKeyWrite({key}, {false_parent}, 3);\n"));
            expected.push_str("K|R|");
            if exact && invalid {
                expected.push_str(if false_parent { "F:array:0|T|array:0|" } else { "T|NULL:-1|" });
            } else {
                if false_parent { expected.push_str("F:array:0|"); }
                expected.push_str(&format!("ok|array:1|integer:{}:9|", if invalid { 1 } else { 0 }));
            }
        }
    }
    for mutation in [1, 2] {
        for (key, invalid) in [("0", false), ("true", true)] {
            source.push_str(&format!("parentKeyWrite({key}, true, {mutation});\n"));
            expected.push_str("K|R|");
            if exact && invalid { expected.push_str("F:array:0|T|array:1|string:h:7|"); }
            else { expected.push_str("F:array:0|ok|array:1|string:h:7|"); }
        }
    }
    source.push_str("parentKeyCopyCheck();\n");
    expected.push_str("K|R|9:3|");
    (source, expected)
}

/// Parent conversion and original-key guards emit for every supported target.
#[test]
fn array_parent_key_order_emit_on_every_target() {
    let (source, _) = source(true);
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower_capturing_arguments(&source, Target::parse(name).unwrap(),
            profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Native writes enforce profile ordering and preserve warning-handler replacement owners.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn array_parent_key_order_native_effects_publication_and_owners() {
    for exact in [false, true] {
        let (source, expected) = source(exact);
        for optimize in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, exact);
            let module = super::semantic_sources::lower_capturing_arguments(&source, Target::detect_host(),
                profile, optimize, SourceMode::Php);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "exact={exact} opt={optimize}: {:?}; {stderr}", output.status);
            assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "exact={exact} opt={optimize}: {stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "exact={exact} opt={optimize}: {stderr}");
        }
    }
}

/// Real runtime eval preserves parent publication, handler replacements and captured owners.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn array_parent_key_order_native_eval_publication_and_owners() {
    let (fragment, expected) = source(false);
    let fragment = fragment.strip_prefix("<?php\n").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php $code = $argc > 0 ? '{fragment}' : ''; eval($code);");
    for optimize in [false, true] {
        let module = super::semantic_sources::lower_capturing_arguments(&source, Target::detect_host(),
            SemanticProfile::PERMISSIVE, optimize, SourceMode::Php);
        let output = super::native_module::run(&module);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "opt={optimize}: {:?}; {stderr}", output.status);
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "opt={optimize}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {stderr}");
    }
}
