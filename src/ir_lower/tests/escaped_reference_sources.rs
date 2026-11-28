//! Purpose:
//! Checks escaped promoted aliases through target emission and real opaque eval.
//!
//! Called from:
//! - Focused persistent property source regressions.
//!
//! Key details:
//! - Factory scopes end before either escaped owner reads or updates the shared cell.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Escaped scalar and nested element aliases lower to every supported target.
#[test]
fn escaped_property_sources_emit_on_every_target() {
    let source = format!("{}escapedPromotedObservations();",
        include_str!("../../../tests/fixtures/strict/escaped_promoted_aliases.php"));
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower_capturing_arguments(&source,
            Target::parse(name).unwrap(), SemanticProfile::PERMISSIVE, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Real eval keeps aliases valid after caller scope teardown with either conversion policy.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
fn verify_escaped_property_sources_native_eval(casts: bool, optimize: bool) {
    let fragment = format!("{}escapedPromotedObservations();",
        include_str!("../../../tests/fixtures/strict/escaped_promoted_aliases.php")
            .strip_prefix("<?php\n").unwrap());
    let fragment = fragment.replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php $code = $argc > 0 ? '{fragment}' : ''; eval($code);");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, casts);
    let module = super::semantic_sources::lower_capturing_arguments(
        &source, Target::detect_host(), profile, optimize, SourceMode::Php,
    );
    let output = super::native_module::run(&module);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "casts={casts} opt={optimize}: {:?}; {stderr}", output.status);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "3/7/11|4/8/12|5/9/13|",
        "casts={casts} opt={optimize}: {stderr}");
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "casts={casts} opt={optimize}: {stderr}");
}

opaque_eval_profile_cases!(escaped_property_sources_native_eval_profiles, verify_escaped_property_sources_native_eval);
