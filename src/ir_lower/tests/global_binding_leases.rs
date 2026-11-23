//! Purpose:
//! Keeps reached global aliases separate from the activation names that can detach.
//!
//! Called from:
//! - Focused native global ownership and interpreter synchronization regressions.
//!
//! Key details:
//! - A reference capture shares the program wrapper; a value capture owns a snapshot.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/global_binding_leases.php");
const EXPECTED: &str = "2|1|2:1|2:2|2|1|2:2|2:3|2|absent:created|present:created|after|2|3.5|1.5|after|before|double:2.5|DCNULL:|DCNULL:|";

/// All targets lower reached global aliases to guarded managed leases on frame bindings.
#[test]
fn global_binding_leases_emit_on_every_target() {
    for locals in [false, true] {
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, locals);
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
            crate::ir::validate_module(&module).unwrap();
            assert!(crate::ir::Op::StoreGlobal.default_effects().contains(crate::ir::Effects::MAY_THROW));
            let conditional = module.functions.iter().find(|function| function.name == "conditionalGlobalLocal").unwrap();
            assert!(conditional.instructions.iter().any(|inst| inst.op == crate::ir::Op::StoreRefCell),
                "{name}: the joined store must preserve the still-attached predecessor");
            let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            assert!(assembly.contains("__rt_reference_cell_retain_mixed"), "{name}");
        }
    }
}

/// Binding ends and reassociation preserve globals, escaped aliases and independent value owners.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn global_binding_leases_native_values_and_owners() {
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

/// Eval publishes through an attached wrapper and leaves a conditionally detached local independent.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
fn verify_opaque_scope_values_and_owners(locals: bool, optimize: bool) {
    let native = r#"
        $opaqueGlobal = 1;
        function updateOpaqueGlobal(bool $reset, string $source): void {
            global $opaqueGlobal;
            $old = function() use (&$opaqueGlobal): mixed { return $opaqueGlobal; };
            if ($reset) { unset($opaqueGlobal); $opaqueGlobal = 2; }
            eval($source);
            echo $opaqueGlobal, ':', $old(), '|';
        }
        $source = $argc > 0 ? '$opaqueGlobal=3;' : '';
        updateOpaqueGlobal(true, $source); echo $opaqueGlobal, '|';
        updateOpaqueGlobal(false, $source); echo $opaqueGlobal, '|';
    "#;
    let fragment = SOURCE.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php $fragment=$argc>0?'{fragment}':'';eval($fragment);{native}");
    let expected = format!("{EXPECTED}3:1|1|3:3|3|");
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
    assert_eq!(stdout, expected, "locals={locals}, opt={optimize}: {stderr}");
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    assert!(!stderr.contains("Undefined variable"), "{stderr}");
}

opaque_eval_profile_cases!(global_binding_leases_opaque_scope, verify_opaque_scope_values_and_owners);
