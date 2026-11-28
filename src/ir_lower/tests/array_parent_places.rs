//! Purpose:
//! Checks property and static parent publication before original-key errors and callbacks.
//!
//! Called from:
//! - Staged original array-key native and supported-target regressions.
//!
//! Key details:
//! - Key and RHS effects run once before the selected parent's materialization.
//! - Callback writes remain visible when an in-flight receiver becomes detached.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Builds parent/key combinations with independent PHP and exact-policy expectations.
fn source(exact: bool) -> (String, String) {
    let mut source = include_str!("../../../tests/fixtures/strict/array_parent_places.php").to_string();
    let mut expected = String::new();
    for static_parent in [false, true] {
        for false_parent in [false, true] {
            for (key, invalid, legacy_invalid, text) in [
                ("0", false, false, "0"), ("true", true, false, "1"),
                ("[]", true, true, ""), ("new ParentPlace()", true, true, ""),
            ] {
                source.push_str(&format!("placeWrite({static_parent}, {false_parent}, {key}, 0);\n"));
                expected.push_str("K|R|");
                let strict_rejected = exact && invalid;
                if false_parent {
                    expected.push_str("F|array:0|");
                }
                if legacy_invalid || strict_rejected {
                    expected.push_str("T|");
                    if strict_rejected {
                        expected.push_str(if false_parent {
                            "array:0|"
                        } else if static_parent {
                            "NULL:-1|"
                        } else {
                            "absent|"
                        });
                    } else {
                        expected.push_str("array:0|");
                    }
                } else {
                    expected.push_str(&format!("ok|array:1|integer:{text}:9|"));
                }
            }
        }
        source.push_str(&format!("placeWrite({static_parent}, true, 0, 3);\n"));
        expected.push_str("K|R|F|array:0|ok|array:1|integer:0:9|");
        expected.push_str("array:1|integer:0:9|");
        for mutation in [1, 2, 4, 5] {
            source.push_str(&format!("placeWrite({static_parent}, true, 0, {mutation});\n"));
            expected.push_str("K|R|F|array:0|ok|array:1|string:h:7|");
            if mutation >= 4 { expected.push_str("array:1|string:h:7|"); }
        }
    }
    for mutation in [6, 7] {
        for (key, text) in [("0", "0"), ("true", "1")] {
            source.push_str(&format!("placeWrite(false, true, {key}, {mutation});\n"));
            let rejected = exact && key == "true";
            expected.push_str("K|R|");
            if rejected {
                expected.push_str("F|array:0|T|absent|");
                if mutation == 6 {
                    expected.push_str("array:0|");
                }
            } else {
                expected.push_str("F|array:0|ok|absent|");
                if mutation == 6 {
                    expected.push_str(&format!("array:1|integer:{text}:9|"));
                }
            }
        }
    }
    (source, expected)
}

/// Parent conversion and original-key guards emit for every supported target.
#[test]
fn array_parent_places_emit_on_every_target() {
    let (source, _) = source(true);
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower_capturing_arguments(&source, Target::parse(name).unwrap(),
            profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        let write = module.functions.iter().find(|function| function.name == "placeWrite").unwrap();
        assert!(write.instructions.iter().any(|instruction| {
            instruction.op == crate::ir::Op::PropGetForWrite
                && instruction.result_php_type == crate::types::PhpType::Mixed
        }), "{name}: a mixed property write fetch must retain its dynamic contract");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Native writes publish empty parents before errors and preserve warning-handler replacement owners.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn array_parent_places_native_effects_publication_and_owners() {
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
fn verify_array_parent_places_native_eval(coordinated: bool, optimize: bool) {
    let (fragment, expected) = source(false);
    let fragment = fragment.strip_prefix("<?php\n").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php $code = $argc > 0 ? '{fragment}' : ''; eval($code);");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, coordinated);
    let module = super::semantic_sources::lower_capturing_arguments(
        &source, Target::detect_host(), profile, optimize, SourceMode::Php,
    );
    let output = super::native_module::run(&module);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "casts={coordinated} opt={optimize}: {:?}; {stderr}", output.status);
    assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "casts={coordinated} opt={optimize}: {stderr}");
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "casts={coordinated} opt={optimize}: {stderr}");
}

opaque_eval_profile_cases!(array_parent_places_native_eval_publication, verify_array_parent_places_native_eval);
