//! Purpose:
//! Verifies legitimate reference-source creation independently of ordinary read initialization.
//!
//! Called from:
//! - Focused strict-local lowering and native ownership regressions.
//!
//! Key details:
//! - Missing reference sources start at null with an open contract; existing origins stay closed.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/fresh_reference_sources.php");
const CHECKED: &str = "NULL|old|NULL|old:new|Tstring|integer|old:4|NULL|alias|NULL:alias|9|sorted|NULL|self|NULL:NULL|seen|TNULL|";
const PERMISSIVE: &str = "NULL|old|NULL|old:new|integer|integer|3:4|NULL|alias|NULL:alias|9|sorted|NULL|self|NULL:NULL|seen|Wstring|";

/// Every supported ABI can lower missing, unset and escaping native reference sources.
#[test]
fn fresh_reference_sources_emit_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for locals in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, locals);
            let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
            crate::ir::validate_module(&module).unwrap();
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        }
    }
}

/// Reference creation initializes null while old escaped aliases retain their own contracts and owners.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn fresh_reference_sources_native_values_and_owners() {
    for locals in [false, true] {
        for optimize in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, locals);
            let module = super::semantic_sources::lower(SOURCE, Target::detect_host(), profile, optimize, SourceMode::Php);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "locals={locals} opt={optimize}: {:?}; {stderr}", output.status);
            assert_eq!(String::from_utf8_lossy(&output.stdout), if locals { CHECKED } else { PERMISSIVE }, "{stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
            assert!(!stderr.contains("Undefined variable"), "{stderr}");
        }
    }
}

/// Ordinary by-value captures do not gain the source-creation semantics of reference formation.
#[test]
fn fresh_reference_sources_keep_by_value_capture_diagnostics() {
    for locals in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, locals);
        for source in ["<?php $read=function() use ($missing) {return $missing;};",
            "<?php $value=1; unset($value); $read=function() use ($value) {return $value;};"] {
            let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
            let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap_err();
            assert!(error.message.contains("Undefined variable in use()"), "{error:?}");
        }
    }
}

/// Real runtime source uses the same open-reference creation and closed-origin checks.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn fresh_reference_sources_opaque_eval_values_and_owners() {
    let body = SOURCE.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php $text=$argc > 0 ? '{body}' : ''; eval($text);");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for optimize in [false, true] {
        let module = super::semantic_sources::lower_capturing_arguments(&source, Target::detect_host(), profile, optimize, SourceMode::Php);
        let output = super::native_module::run(&module);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "opt={optimize}: {:?}; {stderr}", output.status);
        assert_eq!(String::from_utf8_lossy(&output.stdout), CHECKED, "{stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    }
}
