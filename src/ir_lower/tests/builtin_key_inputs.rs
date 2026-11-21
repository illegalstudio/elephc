//! Purpose:
//! Checks original builtin key arguments and exact key-producing runtime helpers.
//!
//! Called from:
//! - Focused staged original-key compiler and target regressions.
//!
//! Key details:
//! - Shared metadata survives named/spread calls and prepared callable aliases.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Builds runtime-unknown key calls and independently expected producer key kinds.
fn source(exact: bool) -> (String, String) {
    let mut source = include_str!("../../../tests/fixtures/strict/builtin_key_inputs.php").to_string();
    let mut expected = String::new();
    for (arg, value, invalid) in [("true", 20, true), ("0.0", 10, true), ("null", 30, true),
        ("0", 10, false), ("'0'", if exact { 31 } else { 10 }, false), ("''", 30, false)] {
        source.push_str(&format!("builtinKeyInputs({arg});\n"));
        expected.push_str(&if exact && invalid { "D|N|S|C|U|I|E|Q|".into() }
            else { format!("d|n|s|c|u|i|e|{value}|") });
    }
    source.push_str("builtinProducedKeys();");
    let kind = if exact { "string" } else { "integer" };
    expected.push_str(&format!("{kind}:0:7|string:01:7|{kind}:0:11|string:01:22|{kind}:0:2|string:01:1|{kind}:0:2|string:01:1|"));
    expected.push_str(if exact { "11|33|" } else { "22|44|" });
    (source, expected)
}

/// Original builtin key contracts and producer helpers emit across all supported targets.
#[test]
fn builtin_key_inputs_emit_on_every_target() {
    let (source, _) = source(true);
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower(&source, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Native key guards keep original values, callback behavior, exact result keys and balanced owners.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn builtin_key_inputs_native_values_and_owners() {
    for exact in [false, true] {
        let (source, expected) = source(exact);
        for optimize in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, exact);
            let module = super::semantic_sources::lower(&source, Target::detect_host(), profile, optimize, SourceMode::Php);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "exact={exact} opt={optimize}: {:?}; {stderr}", output.status);
            assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "exact={exact} opt={optimize}: {stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "exact={exact} opt={optimize}: {stderr}");
        }
    }
}

/// The real eval bridge keeps permissive builtin results and independent temporary leases balanced.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn builtin_key_inputs_native_eval_permissive_values_and_owners() {
    assert_native_eval(false);
}

/// Public exact-key eval preserves producer identities and diagnoses original invalid inputs.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn builtin_key_inputs_native_eval_exact_values_and_owners() {
    assert_native_eval(true);
}

/// Runs the same builtin observations under one independently selected native eval profile.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
fn assert_native_eval(exact: bool) {
    let (fragment, expected) = source(exact);
    let fragment = fragment.strip_prefix("<?php\n").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php $code = $argc > 0 ? '{fragment}' : ''; eval($code);");
    for optimize in [false, true] {
        let module = super::semantic_sources::lower_capturing_arguments(&source, Target::detect_host(),
            SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, exact), optimize, SourceMode::Php);
        let output = super::native_module::run(&module);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "opt={optimize}: {:?}; {stderr}", output.status);
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "opt={optimize}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {stderr}");
    }
}

/// Source proof rejects original invalid builtin keys before constant folding or dead-code pruning.
#[test]
fn builtin_key_inputs_validate_original_source_before_folding() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, true);
    for mode in [SourceMode::Php, SourceMode::Lfc, SourceMode::Internal] {
        for call in ["array_key_exists(true, [])", "array_key_exists(array: [], key: null)",
            "\\ARRAY_KEY_EXISTS(...['array' => [], 'key' => true])",
            "array_fill_keys([false], 1)", "array_combine([0.5], [1])", "array_column([], 1.5)"] {
            let source = format!("<?php if (false) {{ {call}; }}");
            let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(&source).unwrap(), mode).unwrap();
            let error = crate::semantics::validate_source(&program, profile).expect_err(call);
            assert!(error.message.contains("--strict-array-keys"), "{mode:?} {call}: {}", error.message);
            assert!(crate::semantics::validate_source(&program, SemanticProfile::PERMISSIVE).is_ok());
        }
        for call in ["array_key_exists((int)true, [])", "array_column([], null)",
            "array_fill_keys(['0'], 1)", "array_fill_keys([], 1)", "array_combine([], [])"] {
            let source = format!("<?php {call};");
            let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(&source).unwrap(), mode).unwrap();
            crate::semantics::validate_source(&program, profile).unwrap();
        }
    }
}
