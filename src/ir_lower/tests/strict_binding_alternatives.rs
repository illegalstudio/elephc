//! Purpose:
//! Checks source-path local contracts without widening the first executed binding.
//!
//! Called from:
//! - Focused native and all-target semantic-profile regressions.
//!
//! Key details:
//! - Physical Mixed storage never establishes an open semantic producer contract.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/local_binding_alternatives.php");
const CHECKED: &str = "integer|string|string|integer|Winteger|Tstring|Tinteger|Wstring|Winteger|Tstring|string|string|string|integer|Winteger|Tdouble|TNULL|Wstring|Tstring|Winteger|Tstring|Tstring|";

/// Every target retains runtime producer registration and original-value validation.
#[test]
fn strict_binding_alternatives_emit_on_every_target() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for mode in [SourceMode::Php, SourceMode::Lfc] {
            for source in [SOURCE, include_str!("../../../tests/fixtures/strict/local_binding_destructor_unset.php")] {
                let module = super::semantic_sources::lower(source, Target::parse(name).unwrap(), profile, false, mode);
                crate::ir::validate_module(&module).unwrap();
                crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            }
        }
    }
}

/// Joins, missing predecessors, copies and unset keep independent live origin contracts.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_binding_alternatives_native_values_and_owners() {
    let locals = true;
    for mode in [SourceMode::Php, SourceMode::Lfc] {
        for optimize in [false, true] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, locals);
            let module = super::semantic_sources::lower(SOURCE, Target::detect_host(), profile, optimize, mode);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "locals={locals} opt={optimize}: {:?}; {stderr}", output.status);
            assert_eq!(String::from_utf8_lossy(&output.stdout), CHECKED, "{stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        }
    }
}

/// A runtime origin never permits a statically incompatible write to a definitely live binding.
#[test]
fn strict_binding_alternatives_reject_known_invalid_live_origins() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for source in [
        "<?php function invalidBinding(bool $pick): void { $value=1; if($pick) {$value='bad';} }",
        "<?php function invalidBinding(bool $pick): void { if($pick) {$value=1; $value='bad';} }",
        "<?php function invalidBinding(bool $pick): void { if($pick) {$value=1;}else{$value='s';} $value=2.5; }",
    ] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        let errors = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap_err();
        assert!(errors.message.contains("--strict-locals"), "{errors:?}");
    }
}

/// Native functions retire first-path contracts with an interpreter-backed scope layout too.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_binding_alternatives_native_interpreter_scope_values_and_owners() {
    let source = format!("{SOURCE}\n$runtimeText = $argc > 0 ? 'echo \"bridge|\";' : ''; eval($runtimeText);");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for optimize in [false, true] {
        let module = super::semantic_sources::lower_capturing_arguments(&source, Target::detect_host(), profile, optimize, SourceMode::Php);
        assert!(module.required_runtime_features.eval_bridge);
        let output = super::native_module::run(&module);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "opt={optimize}: {:?}; {stderr}", output.status);
        assert_eq!(String::from_utf8_lossy(&output.stdout), format!("{CHECKED}bridge|"), "{stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    }
}

/// A destructor throw cannot leave a detached name pointing at a cleared reference owner.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn strict_binding_destructor_unset_native_values_and_owners() {
    let fixture = include_str!("../../../tests/fixtures/strict/local_binding_destructor_unset.php");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for bridge in [false, true] {
        let source = if bridge { format!("{fixture}\n$text = $argc > 0 ? 'echo \"B|\";' : ''; eval($text);") }
            else { fixture.to_string() };
        for optimize in [false, true] {
            let module = super::semantic_sources::lower_capturing_arguments(&source, Target::detect_host(), profile, optimize, SourceMode::Php);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "bridge={bridge} opt={optimize}: {:?}; {stderr}", output.status);
            assert_eq!(String::from_utf8_lossy(&output.stdout), if bridge { "D|CM|Tstring|Tstring|B|" } else { "D|CM|Tstring|Tstring|" }, "{stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        }
    }
}
