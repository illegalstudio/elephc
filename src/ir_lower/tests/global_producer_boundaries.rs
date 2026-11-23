//! Purpose:
//! Verifies original global producer alternatives after earlier root-level stores.
//!
//! Called from:
//! - Focused reached-global producer and incoming reference ABI regressions.
//!
//! Key details:
//! - Open, union, nullable and concrete origins retain their distinct runtime write domains.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/global_producer_boundaries.php");
const EXPECTED: &str = "Wdouble:2.5|Winteger:3|Wstring:final|Wstring:next|Winteger:3|Tinteger:3|WNULL:|TNULL:|Winteger:4|Tinteger:1|Tinteger:1|Winteger:2|";
const PERMISSIVE: &str = "Wdouble:2.5|Winteger:3|Wstring:final|Wstring:next|Winteger:3|Wdouble:1.5|WNULL:|Wdouble:1.5|Winteger:4|Wdouble:2.5|Wstring:bad|Winteger:2|";

/// Every target preserves original global alternatives independently of current stored values.
#[test]
fn global_producer_boundaries_emit_on_every_target() {
    for locals in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, locals);
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
            crate::ir::validate_module(&module).unwrap();
            assert!(crate::ir::Op::StoreGlobal.default_effects().contains(crate::ir::Effects::MAY_THROW));
            let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            assert!(assembly.contains("__rt_reference_cell_retain_mixed"), "{name}");
        }
    }
}

/// Runtime candidates respect open, union, nullable and concrete producer boundaries.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn global_producer_boundaries_native_values_and_owners() {
    for locals in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, locals);
        for optimize in [false, true] {
            let module = super::semantic_sources::lower(SOURCE, Target::detect_host(), profile, optimize, SourceMode::Php);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "locals={locals}, opt={optimize}: {:?}; stdout={}; {stderr}", output.status, String::from_utf8_lossy(&output.stdout));
            assert_eq!(String::from_utf8_lossy(&output.stdout), if locals { EXPECTED } else { PERMISSIVE }, "locals={locals}, opt={optimize}: {stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
            assert!(!stderr.contains("Undefined variable"), "{stderr}");
        }
    }
}

/// Opaque eval enforces the same original producer alternatives through native value hooks.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
fn verify_opaque_scope_values_and_owners(locals: bool, optimize: bool) {
    let fragment = SOURCE.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php $fragment=$argc>0?'{fragment}':'';eval($fragment);");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, locals);
    let module = super::semantic_sources::lower_capturing_arguments(
        &source, Target::detect_host(), profile, optimize, SourceMode::Php,
    );
    assert!(module.required_runtime_features.eval_bridge);
    let output = super::native_module::run(&module);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(),
        "locals={locals}, opt={optimize}: {:?}; stdout={stdout}; {stderr}", output.status);
    assert_eq!(stdout, if locals { EXPECTED } else { PERMISSIVE },
        "locals={locals}, opt={optimize}: {stderr}");
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    assert!(!stderr.contains("Undefined variable"), "{stderr}");
}

opaque_eval_profile_cases!(global_producer_boundaries_opaque_scope, verify_opaque_scope_values_and_owners);
