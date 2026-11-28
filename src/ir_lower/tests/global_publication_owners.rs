//! Purpose:
//! Keeps source owners balanced when program-global publication retires a throwing value.
//!
//! Called from:
//! - Focused EIR/native global publication and main-scope binding regressions.
//!
//! Key details:
//! - A retained source lease is retired on both normal and exceptional completion.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/global_publication_owners.php");
const EXPECTED: &str = "DCobject:2|DCarray:3|DCstring:xx|";

/// Every target gives global stores unwind-visible source ownership.
#[test]
fn global_publication_sources_emit_on_every_target() {
    for locals in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, locals);
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
            crate::ir::validate_module(&module).unwrap();
            let main = module.functions.iter().find(|function| function.name == "main").unwrap();
            assert!(main.instructions.iter().any(|inst| inst.op == crate::ir::Op::StoreGlobal));
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        }
    }
}

/// Object, array and string sources survive throwing retirement without extra owners.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn global_publication_sources_native_values_and_owners() {
    for locals in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, locals);
        for optimize in [false, true] {
            let module = super::semantic_sources::lower(SOURCE, Target::detect_host(), profile, optimize, SourceMode::Php);
            let output = super::native_module::run(&module);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "locals={locals} opt={optimize}: {stderr}");
            assert_eq!(String::from_utf8_lossy(&output.stdout), EXPECTED, "locals={locals} opt={optimize}: {stderr}");
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
            assert!(!stderr.contains("Warning:"), "{stderr}");
        }
    }
}

/// Main binding metadata uses the same hidden global-scope slot codegen consumes.
#[test]
fn main_binding_origins_declare_the_global_scope_on_every_target() {
    let source = r#"<?php
        class RetiredMainScope {
            public function __destruct() { throw new RuntimeException('retired'); }
        }
        class KeptMainScope { public int $value = 2; }
        function mainScopeProducer(): mixed { return new RetiredMainScope(); }
        $value = mainScopeProducer();
        try { $value = new KeptMainScope(); } catch (Throwable $error) {}
        echo gettype($value);
    "#;
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower(source, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        let main = module.functions.iter().find(|function| function.name == "main").unwrap();
        assert!(main.locals.iter().any(|local| local.kind == crate::ir::LocalKind::EvalGlobalScope));
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Checks opaque eval's native value ABI with one local and optimizer profile.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
fn assert_opaque_global_publication_owners(locals: bool, optimize: bool) {
    let fragment = SOURCE.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php $fragment=$argc>0?'{fragment}':'';eval($fragment);");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, locals);
    let module = super::semantic_sources::lower_capturing_arguments(&source,
        Target::detect_host(), profile, optimize, SourceMode::Php);
    assert!(module.required_runtime_features.eval_bridge);
    let output = super::native_module::run(&module);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "locals={locals} opt={optimize}: {stderr}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), EXPECTED, "locals={locals} opt={optimize}: {stderr}");
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    assert!(!stderr.contains("Warning:"), "{stderr}");
}

/// Leaves both strict locals and EIR optimization off.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn global_publication_sources_opaque_owners_permissive_unoptimized() {
    assert_opaque_global_publication_owners(false, false);
}

/// Retains permissive local bindings with EIR optimization.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn global_publication_sources_opaque_owners_permissive_optimized() {
    assert_opaque_global_publication_owners(false, true);
}

/// Checks strict local bindings with EIR optimization off.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn global_publication_sources_opaque_owners_strict_unoptimized() {
    assert_opaque_global_publication_owners(true, false);
}

/// Checks strict local bindings with EIR optimization.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn global_publication_sources_opaque_owners_strict_optimized() {
    assert_opaque_global_publication_owners(true, true);
}
