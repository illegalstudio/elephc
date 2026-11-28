//! Purpose:
//! Checks original eval array keys through the real native bridge and every target emitter.
//!
//! Called from:
//! - Focused public semantic-profile integration regressions.
//!
//! Key details:
//! - Runtime-selected source reaches Magician rather than literal-eval AOT lowering.
//! - One fragment shares declarations and exercises scalar, array and object keys.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Builds an opaque eval source with independently expected original key kinds.
fn source(exact: bool) -> (String, String) {
    let mut code = include_str!("../../../tests/fixtures/strict/eval_array_key_guards.php")
        .strip_prefix("<?php\n").unwrap().to_string();
    let mut expected = String::new();
    for (argument, kind, read) in [("true", "boolean", "20"), ("0.0", "double", "10"),
        ("null", "NULL", "30"), ("0", "integer", "10"), ("''", "string", "30")]
    {
        code.push_str(&format!("evalArrayKeyGuardObservation({argument});\n"));
        if exact && !matches!(kind, "integer" | "string") { expected.push_str("R|W|L|U|P|Q|S|T|"); }
        else { expected.push_str(&format!("{read}|w|l|u|p|q|s|t|")); }
        expected.push_str(&format!("G:{kind}|1|O:{kind}|V:{kind}|"));
    }
    if exact {
        for (argument, kind) in [("[]", "array"), ("new EvalGuardOwnedKey()", "object")] {
            code.push_str(&format!("evalArrayKeyGuardObservation({argument});\n"));
            expected.push_str(&format!("R|W|L|U|P|Q|S|T|G:{kind}|1|O:{kind}|V:{kind}|"));
            if kind == "object" { expected.push_str("D|"); }
        }
    }
    let quoted = code.replace('\\', "\\\\").replace('\'', "\\'");
    (format!("<?php $code = '{quoted}'; eval($argc > 0 ? $code : '');"), expected)
}

/// Profiled runtime eval source emits with the complete bridge on all five supported targets.
fn verify_eval_array_key_guards_emission(name: &str) {
    let (source, _) = source(true);
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, true);
    let module = super::semantic_sources::lower_capturing_arguments(&source,
        Target::parse(name).unwrap(), profile, false, SourceMode::Php);
    crate::ir::validate_module(&module).unwrap();
    crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
}

supported_target_cases!(eval_array_key_guards_emit_by_target, verify_eval_array_key_guards_emission);

/// Native permissive eval keeps its values and owners alongside the independent exact profile.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn eval_array_key_guards_native_permissive_bridge_compatibility() { assert_native(false); }

/// Strict native guards preserve original key kinds through the public interpreter boundary.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn eval_array_key_guards_native_strict_bridge_kinds_and_owners() { assert_native(true); }

/// Runs one real bridge profile through both optimization modes with exact stdout and heap checks.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
fn assert_native(exact: bool) {
    let (source, expected) = source(exact);
    for optimize in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, exact);
        let module = super::semantic_sources::lower_capturing_arguments(&source,
            Target::detect_host(), profile, optimize, SourceMode::Php);
        let output = super::native_module::run(&module);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "exact={exact} opt={optimize}: {:?}; {stderr}", output.status);
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "exact={exact} opt={optimize}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "exact={exact} opt={optimize}: {stderr}");
    }
}
