//! Purpose:
//! Checks deferred false-parent diagnostics across local, reference, instance, and static writes.
//!
//! Called from:
//! - Focused native and supported-target semantic profile regressions.
//!
//! Key details:
//! - Original key kinds select completion and Throwable chains retain callback identity.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Builds a shared source with explicit PHP and exact-policy completion expectations.
fn source(exact: bool) -> (String, String) {
    let mut source = include_str!("../../../tests/fixtures/strict/false_parent_completion.php").to_string();
    let mut expected = String::new();
    for place in 0..4 {
        for (key, kind, text, invalid, completes) in [
            ("0", "integer", "0", false, true), ("'a'", "string", "a", false, true),
            ("'0'", if exact { "string" } else { "integer" }, "0", false, true),
            ("true", "integer", "1", true, true), ("null", "", "", true, false),
            ("1.0", "", "", true, false), ("[]", "", "", true, true),
            ("new PendingParent()", "", "", true, true),
        ] {
            source.push_str(&format!("pendingCase({place}, {key}, 0);\n"));
            expected.push_str("K|R|W|");
            let type_error = completes && invalid && (exact || kind.is_empty());
            expected.push_str(if type_error { "TypeError|new|handler:same|older:same|" } else { "Exception|same|older:same|" });
            if completes && !type_error { expected.push_str(&format!("array:1|{kind}:{text}:9|")); }
            else { expected.push_str("array:0|"); }
        }
        source.push_str(&format!("pendingCase({place}, null, 1); pendingCase({place}, 0, 2);\n"));
        expected.push_str("R|W|Exception|same|older:same|array:1|integer:0:9|");
        expected.push_str("K|R|W|Exception|same|older:same|array:0|");
    }
    (source, expected)
}

/// All supported targets lower each original-key completion branch and validate its EIR.
#[test]
fn false_parent_completion_emits_on_every_target() {
    let (source, _) = source(true);
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower_capturing_arguments(&source, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Native stores preserve PHP completion and strict-key error priority with both optimizers.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn false_parent_completion_native_key_priority_and_owners() {
    for exact in [false, true] {
        let (source, expected) = source(exact);
        for optimize in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, exact);
            let module = super::semantic_sources::lower_capturing_arguments(&source, Target::detect_host(), profile, optimize, SourceMode::Php);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "exact={exact} opt={optimize}: {:?}; {stderr}", output.status);
            assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "exact={exact} opt={optimize}: {stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "exact={exact} opt={optimize}: {stderr}");
        }
    }
}

/// Known completing keys keep the callback handler without a dynamic key diagnostic graph.
#[test]
fn false_parent_completion_omits_impossible_key_errors() {
    let source = "<?php
        function putKnownKeys(mixed $owner, int $key): void {
            $owner[$key] = 1;
            $owner['name'] = 2;
        }
        function putKnownNestedKeys(mixed $owner): void { $owner['branch'][0] = 3; }
        function putNonNumericOffset(mixed $owner): void { $owner['branch']['word'] = 3; }
        function putOriginalKey(mixed $owner, mixed $key): void { $owner[$key] = 1; }
        function putOriginalNestedKey(mixed $owner, mixed $key): void { $owner['branch'][$key] = 3; }
        $owner = $argc > 2 ? false : [];
        $key = $argc > 2 ? 1 : [];
        putKnownKeys($owner, $argc);
        putKnownNestedKeys($owner);
        putNonNumericOffset($owner);
        putOriginalKey($owner, $key);
        putOriginalNestedKey($owner, $key);";
    let module = super::semantic_sources::lower_capturing_arguments(
        source, Target::detect_host(), SemanticProfile::PERMISSIVE, false, SourceMode::Php,
    );
    for (name, key_error, scalar_key_error) in [
        ("putKnownKeys", false, false),
        ("putKnownNestedKeys", false, false),
        ("putNonNumericOffset", false, false),
        ("putOriginalKey", true, false),
        ("putOriginalNestedKey", true, true),
    ] {
        let function = module.functions.iter().find(|function| function.name == name).unwrap();
        assert!(function.blocks.iter().any(|block| block.name == "nested.false.capture"));
        assert_eq!(function.blocks.iter().any(|block| block.name == "nested.false.invalid.key"), key_error, "{name}");
        assert_eq!(function.blocks.iter().any(|block| block.name == "nested.write.false.pending.type.error"), scalar_key_error, "{name}");
    }
    let nested = module.functions.iter().find(|function| function.name == "putNonNumericOffset").unwrap();
    assert!(nested.blocks.iter().any(|block| block.name == "nested.write.string.error"));
    assert!(!nested.blocks.iter().any(|block| block.name == "nested.write.string"));
    let numeric = module.functions.iter().find(|function| function.name == "putKnownNestedKeys").unwrap();
    assert!(numeric.blocks.iter().any(|block| block.name == "nested.write.string"));
}

/// Opaque runtime eval completes captured writes and preserves callback Throwable identities.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
fn verify_false_parent_completion_native_eval(coordinated: bool, optimize: bool) {
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
    assert_eq!(String::from_utf8_lossy(&output.stdout), expected,
        "casts={coordinated} opt={optimize}: {stderr}");
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"),
        "casts={coordinated} opt={optimize}: {stderr}");
}

opaque_eval_profile_cases!(false_parent_completion_native_eval_priority, verify_false_parent_completion_native_eval);
