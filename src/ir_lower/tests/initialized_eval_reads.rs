//! Purpose:
//! Verifies Initialization at native and literal-eval scope boundaries.
//!
//! Called from:
//! - Focused staged eval initialization regressions.
//!
//! Key details:
//! - Initialized null must survive synchronization independently of missing names.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const CASES: &[(&str, &str)] = &[
    ("<?php function evalNullValue():void {$value=null;eval('echo gettype($value);');}evalNullValue();", "NULL"),
    ("<?php function evalUnsetValue(int $argc):void {$value='old';if($argc>0){unset($value);}try{eval('echo $value;');}catch(Error $error){echo 'I';}}evalUnsetValue($argc);", "I"),
    ("<?php function evalMadeValue():void {eval('$made=null;');eval('echo gettype($made);');}evalMadeValue();", "NULL"),
    ("<?php function evalQuietValue(int $argc):void {$value='old';if($argc>0){unset($value);}eval('echo isset($value)? \"P\":\"A\";echo empty($value)?\"E\":\"F\";echo $value??\"missing\";');}evalQuietValue($argc);", "AEmissing"),
    ("<?php function evalQuietMade():void {eval('$made=null;');eval('echo isset($made)?\"P\":\"A\";echo empty($made)?\"E\":\"F\";echo $made??\"missing\";');}evalQuietMade();", "AEmissing"),
];

/// Literal eval sees initialized null and refuses reads after a logical unset.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn initialized_eval_reads_native_values_and_owners() {
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
            assert!(failures.is_empty(), "locals={locals}, opt={optimize}, mode={mode:?}: failed cases={failures:?}");
        }
    }
}

/// Scope presence and literal planning stay target independent.
#[test]
fn initialized_eval_reads_emit_on_every_target() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true);
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for (source, _) in CASES {
            let module = super::semantic_sources::lower_capturing_arguments(source, Target::parse(target).unwrap(), profile, false, SourceMode::Php);
            crate::ir::validate_module(&module).unwrap();
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        }
    }
}

/// A scope visibility observation must produce Bool and retain one scope-handle operand.
#[test]
fn initialized_eval_reads_validate_presence_shape() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true);
    let mut module = super::semantic_sources::lower_capturing_arguments(CASES[0].0, Target::detect_host(), profile, false, SourceMode::Php);
    let function = module.functions.iter_mut().find(|function|
        function.instructions.iter().any(|inst| inst.op == crate::ir::Op::EvalScopeContains)).unwrap();
    let inst = function.instructions.iter_mut().find(|inst| inst.op == crate::ir::Op::EvalScopeContains).unwrap();
    assert_eq!(inst.operands.len(), 1);
    inst.result_php_type = crate::types::PhpType::Int;
    let result = inst.result.unwrap();
    function.value_mut(result).unwrap().php_type = crate::types::PhpType::Int;
    assert!(crate::ir::validate_module(&module).is_err());
}

/// Presence-sensitive eval keeps the direct-parameter optimization only for permissive reads.
#[test]
fn initialized_eval_reads_plan_keeps_permissive_direct_parameters() {
    for initialized in [false, true] {
        let semantics = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, initialized);
        let profile = crate::source::EvalSourceProfile::new(false, semantics);
        let plan = crate::eval_aot::plan_literal_fragment_with_source_path_and_static_and_method_calls(
            "echo $value;", None, profile, |_, _| false, |_, _, _| false);
        assert_eq!(plan.uses_scope_read_params(), !initialized);
        assert_eq!(plan.requires_runtime_eval_scope(), initialized);
        assert!(!plan.requires_runtime_eval_bridge());
    }
}
