//! Purpose:
//! Checks immutable producer metadata carried with independent closure value snapshots.
//!
//! Called from:
//! - Focused strict-local checker and native capture regressions.
//!
//! Key details:
//! - Hidden descriptors outlive the source activation without sharing the source binding.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/captured_value_producers.php");
const CHECKED: &str = "2|Tinteger:1|integer:1|2|Tstring:old|string:old|2|Winteger:3|integer:1|2|Wstring:ready|string:old|Tinteger:1|Winteger:2|Tinteger:1|Tstring:old|Wstring:ready|Wstring:new|Tinteger:1|";

/// Every target carries immutable descriptors through the existing hidden capture ABI.
#[test]
fn captured_value_producers_emit_on_every_target() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        assert!(module.closures.iter().any(|function| function.params.iter()
            .any(|parameter| crate::names::is_generated_local_name(&parameter.name))), "{name}");
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(assembly.contains("__elephc_reference_scope_type_v1"), "{name}");
        let mut invalid = module.clone();
        invalid.semantic_profile = invalid.semantic_profile.with(StrictFeature::Locals, false);
        assert!(crate::ir::validate_module(&invalid).is_err(), "{name}: descriptors require Locals");
        let mut invalid = module.clone();
        let instruction = invalid.closures.iter_mut().flat_map(|function| &mut function.instructions)
            .find(|instruction| instruction.op == crate::ir::Op::EvalScopeCopyBindingType
                && instruction.operands.len() == 2).unwrap();
        instruction.operands[1] = instruction.operands[0];
        assert!(crate::ir::validate_module(&invalid).is_err(), "{name}: descriptors cannot be Bool");
    }
}

/// Escaped, nested and descriptor-invoked closures retain contracts and retire every owner.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn captured_value_producers_native_values_and_owners() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for optimize in [false, true] {
        let module = super::semantic_sources::lower(SOURCE, Target::detect_host(), profile, optimize, SourceMode::Php);
        let output = super::native_module::run(&module);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "opt={optimize}: {:?}; stdout={}; {stderr}", output.status, String::from_utf8_lossy(&output.stdout));
        assert_eq!(String::from_utf8_lossy(&output.stdout), CHECKED, "opt={optimize}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        assert!(!stderr.contains("Undefined variable"), "{stderr}");
    }
}

/// Native closure bodies preserve frozen contracts when opaque source writes the captured name.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn captured_value_producers_opaque_scope_values_and_owners() {
    let fragment = SOURCE.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let native = r#"
        function nativeSelectedValue194(bool $numeric,string $source):void {
            if($numeric){$value=1;}else{$value='old';}
            $read=function()use($value,$source):void {
                eval($source);echo gettype($value),':',$value,'|';
            };
            $read();echo gettype($value),':',$value,'|';
        }
        $text=$argc>0?'try{$value="new";echo "W";}catch(TypeError $error){echo "T";}':'';
        $number=$argc>0?'try{$value=2;echo "W";}catch(TypeError $error){echo "T";}':'';
        nativeSelectedValue194(true,$text);nativeSelectedValue194(false,$number);
    "#;
    let source = format!("<?php $fragment=$argc>0?'{fragment}':'';eval($fragment);{native}");
    let expected = format!("{CHECKED}Tinteger:1|integer:1|Tstring:old|string:old|");
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for optimize in [false, true] {
        let module = super::semantic_sources::lower_capturing_arguments(&source, Target::detect_host(), profile, optimize, SourceMode::Php);
        assert!(module.required_runtime_features.eval_bridge, "exercise the interpreter bridge");
        let output = super::native_module::run(&module);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "opt={optimize}: {:?}; stdout={}; {stderr}", output.status, String::from_utf8_lossy(&output.stdout));
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "opt={optimize}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    }
}

/// Frozen metadata keeps following string pairs aligned when capture arguments spill to the stack.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn captured_value_producers_stack_arguments_preserve_contracts_and_owners() {
    let source = r#"<?php
        function stackCaptured194(bool $numeric,mixed $next):void {
            $a='a';$b='b';$c='c';$d='d';
            if($numeric){$value=1;}else{$value='old';}
            $read=function(mixed $next)use($a,$b,$value,$c,$d):void {
                echo $a,$b,$c,$d;
                try{$value=$next;echo'W';}catch(TypeError $error){echo'T';}
                echo gettype($value),':',$value,'|';
            };
            call_user_func($read,$next);
        }
        stackCaptured194(true,'bad');stackCaptured194(false,2);
        stackCaptured194(true,3);stackCaptured194(false,'ready');
    "#;
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower(source, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
    for optimize in [false, true] {
        let module = super::semantic_sources::lower(source, Target::detect_host(), profile, optimize, SourceMode::Php);
        let output = super::native_module::run(&module);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "opt={optimize}: {:?}; stdout={}; {stderr}", output.status, String::from_utf8_lossy(&output.stdout));
        assert_eq!(String::from_utf8_lossy(&output.stdout),
            "abcdTinteger:1|abcdTstring:old|abcdWinteger:3|abcdWstring:ready|", "opt={optimize}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    }
}
