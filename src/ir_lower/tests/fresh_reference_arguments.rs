//! Purpose:
//! Checks source-order creation of plain locals supplied to known reference parameters.
//!
//! Called from:
//! - Focused call-planner and strict-local native regressions.
//!
//! Key details:
//! - Reference sources are selected by the shared plan and created at their argument point.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/fresh_reference_arguments.php");
const CHECKED: &str = "bound|bound|bound|closure|NULL:NULL|observed|present:NULL|observed|formed|closure-formed|later:string|observed|positional:named|NULL|private|NULL|Tinteger|";
const PERMISSIVE: &str = "bound|bound|bound|closure|NULL:NULL|observed|present:NULL|observed|formed|closure-formed|later:string|observed|positional:named|NULL|private|NULL|Wstring|";

/// Every supported target lowers direct, named, variadic and tracked callable reference sources.
#[test]
fn fresh_reference_arguments_emit_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for locals in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, locals);
            let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
            crate::ir::validate_module(&module).unwrap();
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        }
    }
}

/// Reference creation precedes later reads and writes, survives throws, and preserves closed origins.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn fresh_reference_arguments_native_values_and_owners() {
    for locals in [false, true] {
        for optimize in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, locals);
            let module = super::semantic_sources::lower(SOURCE, Target::detect_host(), profile, optimize, SourceMode::Php);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "locals={locals} opt={optimize}: {:?}; {stderr}", output.status);
            assert_eq!(String::from_utf8_lossy(&output.stdout), if locals { CHECKED } else { PERMISSIVE }, "locals={locals} opt={optimize}: {stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
            assert!(!stderr.contains("Undefined variable"), "{stderr}");
        }
    }
}

/// Opaque runtime source preserves the same binding identity and balances native heap owners.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
fn verify_opaque_eval_values_and_owners(locals: bool, optimize: bool) {
    let body = SOURCE.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php $text=$argc > 0 ? '{body}' : ''; eval($text);");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, locals);
    let module = super::semantic_sources::lower_capturing_arguments(
        &source, Target::detect_host(), profile, optimize, SourceMode::Php,
    );
    let output = super::native_module::run(&module);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "locals={locals} opt={optimize}: {:?}; {stderr}", output.status);
    assert_eq!(String::from_utf8_lossy(&output.stdout), if locals { CHECKED } else { PERMISSIVE }, "{stderr}");
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    assert!(!stderr.contains("Undefined variable"), "{stderr}");
}

opaque_eval_profile_cases!(fresh_reference_arguments_opaque_eval, verify_opaque_eval_values_and_owners);

/// Planning cannot initialize a later reference before an earlier ordinary argument reads that name.
#[test]
fn fresh_reference_arguments_preserve_earlier_read_diagnostics() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for call in ["observe(value: $missing, target: $missing);", "observe($missing, $missing);"] {
        let source = format!("<?php function observe(mixed $value, mixed &$target):void {{}} {call}");
        let program = crate::parser::parse(&crate::lexer::tokenize(&source).unwrap()).unwrap();
        let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap_err();
        assert!(error.message.contains("Undefined variable: $missing"), "{error:?}");
    }
}

/// A preliminary shape query leaves the final call validator's diagnostic label authoritative.
#[test]
fn fresh_reference_arguments_keep_callee_diagnostic_labels() {
    let source = "<?php function namedRef(mixed &$target):void {} namedRef(other: 1);";
    let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap_err();
    assert!(error.message.contains("Function 'namedRef'"), "{error:?}");
    assert!(!error.message.contains("Reference argument"), "{error:?}");
}
