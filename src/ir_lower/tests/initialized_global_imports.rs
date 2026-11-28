//! Purpose:
//! Verifies absent canonical globals at materialized eval-scope import boundaries.
//!
//! Called from:
//! - Focused staged Initialization synchronization regressions.
//!
//! Key details:
//! - A PHP null is a present boxed cell; a null cell pointer is an absent binding.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const CASES: &[(&str, &str)] = &[
    ("<?php $root='old';function importRootTouch():void {global $root;}if($argc>0){unset($root);}try{eval('echo $root;');}catch(Error $error){echo 'I';}", "I"),
    ("<?php $root='old';function importRootAgainTouch():void {global $root;}eval('echo $root;');if($argc>0){unset($root);}try{eval('echo $root;');}catch(Error $error){echo 'I';}", "oldI"),
    ("<?php $root=null;function importNullTouch():void {global $root;}eval('echo gettype($root);');", "NULL"),
    ("<?php class ImportRetiredRoot {public function __destruct(){echo 'D';throw new Error('retired');}}$root=new ImportRetiredRoot();function importRetiredTouch():void {global $root;}eval('echo gettype($root);');if($argc>0){unset($root);}try{eval('echo $root;');}catch(Error $error){echo 'C';}try{eval('echo $root;');}catch(Error $error){echo 'J';}try{echo $root;}catch(Error $error){echo 'I';}", "objectDCJI"),
];

/// Every target emits the same absent-import transition and native runtime helper.
#[test]
fn initialized_global_imports_emit_on_every_target() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        for (source, _) in CASES {
            let module = super::semantic_sources::lower_capturing_arguments(source, target, profile, false, SourceMode::Php);
            crate::ir::validate_module(&module).unwrap();
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            let runtime = crate::codegen_support::generate_runtime_with_semantics(
                8_388_608, target, module.required_runtime_features, false, false, profile);
            assert!(runtime.contains("__elephc_eval_scope_set_absent_done"), "{name}");
        }
    }
}

/// Repeated imports preserve absence and publish it before a displaced destructor can throw.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn initialized_global_imports_native_values_and_owners() {
    for locals in [false, true] {
        for (optimize, mode) in [(false, SourceMode::Php), (true, SourceMode::Php),
            (false, SourceMode::Lfc), (true, SourceMode::Lfc)] {
            let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true)
            .with(StrictFeature::Locals, locals);
            let mut failures = Vec::new();
            for (index, (source, expected)) in CASES.iter().copied().enumerate() {
                let module = super::semantic_sources::lower_capturing_arguments(source, Target::detect_host(), profile, optimize, mode);
                let output = super::native_module::run(&module);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                if !output.status.success() || stdout != expected || !stderr.contains("HEAP DEBUG: leak summary: clean") {
                    eprintln!("case={index}: stdout={stdout}; {stderr}");
                    failures.push(index);
                }
            }
            assert!(failures.is_empty(), "failed cases={failures:?}");
        }
    }
}
