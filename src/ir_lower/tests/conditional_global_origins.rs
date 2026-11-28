//! Purpose:
//! Preserves executed global and recreated local producer origins across conditional binding ends.
//!
//! Called from:
//! - Focused reached-global producer and incoming reference ABI regressions.
//!
//! Key details:
//! - Old captured references keep the program value while detached locals start fresh contracts.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/conditional_global_origins.php");
const EXPECTED: &str = "Wstring:new:1|1|Tinteger:1:1|1|Tinteger:1:1|1|Winteger:2:2|2|integer:2|integer:2|Tstring:local|2|Winteger:3|3|";

/// Every target tracks reached global contracts and conditional local recreation.
#[test]
fn conditional_global_origins_emit_on_every_target() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(assembly.contains("__rt_reference_cell_retain_mixed"), "{name}");
    }
}

/// Detached globals create local origins while attached paths retain the program contract.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn conditional_global_origins_native_values_and_owners() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for optimize in [false, true] {
        let module = super::semantic_sources::lower(SOURCE, Target::detect_host(), profile, optimize, SourceMode::Php);
        let output = super::native_module::run(&module);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "opt={optimize}: {:?}; stdout={}; {stderr}", output.status, String::from_utf8_lossy(&output.stdout));
        assert_eq!(String::from_utf8_lossy(&output.stdout), EXPECTED, "opt={optimize}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        assert!(!stderr.contains("Undefined variable"), "{stderr}");
    }
}

/// Opaque eval keeps old global captures and recreated local origins separate.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn conditional_global_origins_opaque_scope_values_and_owners() {
    let fragment = SOURCE.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php $fragment=$argc>0?'{fragment}':'';eval($fragment);");
    let expected = EXPECTED;
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for optimize in [false, true] {
        let module = super::semantic_sources::lower_capturing_arguments(&source, Target::detect_host(), profile, optimize, SourceMode::Php);
        assert!(module.required_runtime_features.eval_bridge);
        let output = super::native_module::run(&module);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "opt={optimize}: {:?}; stdout={}; {stderr}", output.status, String::from_utf8_lossy(&output.stdout));
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "opt={optimize}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        assert!(!stderr.contains("Undefined variable"), "{stderr}");
    }
}


/// Permissive native checking retains its existing conditional cross-kind diagnostic.
#[test]
fn conditional_global_origins_keep_the_legacy_retyping_diagnostic() {
    let program = crate::parser::parse(&crate::lexer::tokenize(SOURCE).unwrap()).unwrap();
    let error = crate::types::checker::check_staged_semantics(&program, Target::detect_host(),
        SemanticProfile::PERMISSIVE).unwrap_err();
    assert!(error.message.contains("cannot reassign $conditionalBindingGlobal from int to string"), "{error:?}");
}
