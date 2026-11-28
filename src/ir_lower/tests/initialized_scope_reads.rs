//! Purpose:
//! Verifies Initialization across persistent declarations, nested probes and properties.
//!
//! Called from:
//! - Focused staged initialized-read tests.
//!
//! Key details:
//! - Program presence is independent of a frame's incidental allocation state.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::{SemanticProfile, StrictFeature};

const SOURCE: &str = include_str!("../../../tests/fixtures/strict/initialized_scope_reads.php");
const EXPECTED: &str = "3|I|3|4|I|4|A:missing|I|5|AE:missing|I|PF:7|7|KA|KE|Kmissing|I|NULL|I|I|AE:missing|9:9|PF:3:3|G6|A|E|";

/// Every supported target lowers the same scope and property initialization boundaries.
#[test]
fn initialized_scope_reads_emit_on_every_target() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true);
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::semantic_sources::lower_capturing_arguments(SOURCE, Target::parse(name).unwrap(), profile, false, SourceMode::Php);
        crate::ir::validate_module(&module).unwrap();
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Missing source storage throws while intentional nested probes remain silent.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn initialized_scope_reads_native_values_and_owners() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true);
    for (optimize, mode) in [(false, SourceMode::Php), (true, SourceMode::Php),
        (false, SourceMode::Lfc), (true, SourceMode::Lfc)] {
        let module = super::semantic_sources::lower_capturing_arguments(SOURCE, Target::detect_host(), profile, optimize, mode);
        let output = super::native_module::run(&module);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "opt={optimize}: stdout={}; {stderr}", String::from_utf8_lossy(&output.stdout));
        assert_eq!(String::from_utf8_lossy(&output.stdout), EXPECTED, "opt={optimize}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        assert!(!stderr.contains("Undefined variable"), "{stderr}");
    }
}

/// A program presence predicate must stay a well-formed, zero-operand boolean observation.
#[test]
fn initialized_scope_reads_validate_global_presence_shape() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true);
    let mut module = super::semantic_sources::lower_capturing_arguments(SOURCE, Target::detect_host(), profile, false, SourceMode::Php);
    let function = module.functions.iter_mut().find(|function|
        function.instructions.iter().any(|inst| inst.op == crate::ir::Op::GlobalBindingPresent)).unwrap();
    let inst = function.instructions.iter_mut()
        .find(|inst| inst.op == crate::ir::Op::GlobalBindingPresent).expect("canonical global presence query");
    assert_eq!(inst.effects, crate::ir::Effects::READS_GLOBAL);
    inst.result_php_type = crate::types::PhpType::Int;
    let result = inst.result.unwrap();
    function.value_mut(result).unwrap().php_type = crate::types::PhpType::Int;
    assert!(crate::ir::validate_module(&module).is_err());
}

/// A source property failure retains Error metadata without requiring a catch in user code.
#[cfg(any(all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
#[test]
fn initialized_scope_reads_property_failure_without_catch() {
    let source = "<?php class MissingInitializedProperty { public $value; } $object=new MissingInitializedProperty();unset($object->value);echo $object->value;";
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Initialization, true);
    let module = super::semantic_sources::lower(source, Target::detect_host(), profile, true, SourceMode::Php);
    let output = super::native_module::run(&module);
    let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(!output.status.success(), "{text}");
    assert!(text.contains("--strict-initialization: property $value is not initialized"), "{text}");
}
