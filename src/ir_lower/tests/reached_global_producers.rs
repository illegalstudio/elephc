//! Purpose:
//! Keeps reached program views independent of the activation bindings they replace.
//!
//! Called from:
//! - Focused reached-global producer and incoming reference ABI regressions.
//!
//! Key details:
//! - The same declaration replaces parameters, foreign references, statics and ordinary locals.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/reached_global_producers.php");
const EXPECTED: &str = "string:after|after|string:after|1:after|string:after|after|string:after|string:after|after|double:2.5|string:ready|";

/// Every target replaces incompatible old source views without changing parameter ABIs.
#[test]
fn reached_global_producers_emit_on_every_target() {
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

/// Global views replace old bindings while a displaced caller reference keeps its value.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn reached_global_producers_native_values_and_owners() {
    for locals in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, locals);
        for optimize in [false, true] {
            let module = super::semantic_sources::lower(SOURCE, Target::detect_host(), profile, optimize, SourceMode::Php);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "locals={locals}, opt={optimize}: {:?}; stdout={}; {stderr}", output.status, String::from_utf8_lossy(&output.stdout));
            assert_eq!(String::from_utf8_lossy(&output.stdout), EXPECTED, "locals={locals}, opt={optimize}: {stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
            assert!(!stderr.contains("Undefined variable"), "{stderr}");
        }
    }
}

/// The real interpreter uses the same reached source for every displaced binding role.
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
    assert_eq!(stdout, EXPECTED, "locals={locals}, opt={optimize}: {stderr}");
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    assert!(!stderr.contains("Undefined variable"), "{stderr}");
}

opaque_eval_profile_cases!(reached_global_producers_opaque_scope, verify_opaque_scope_values_and_owners);

/// A displaced string parameter cannot make a closed integer global accept a string store.
#[test]
fn reached_global_producers_reject_the_wrong_source_kind_on_every_target() {
    let source = "<?php $closedReachedGlobal = 1; function wrongReachedGlobal(string $closedReachedGlobal): void { global $closedReachedGlobal; $closedReachedGlobal = 'bad'; } wrongReachedGlobal('old');";
    let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let error = crate::types::checker::check_staged_semantics(&program, Target::parse(name).unwrap(), profile).unwrap_err();
        assert!(error.message.contains("--strict-locals: cannot reassign $closedReachedGlobal from int to string"), "{name}: {error:?}");
    }
}
