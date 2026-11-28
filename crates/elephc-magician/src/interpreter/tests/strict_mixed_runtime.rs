//! Purpose:
//! Verifies runtime Mixed source proofs independently from concrete storage occupants.
//!
//! Called from:
//! - Focused walker and VM-dispatch semantic profile regressions.
//!
//! Key details:
//! - Logical mixed and union sources retain their alternatives until a guard narrows them.
//! - The shared fixture runner checks owner balance after accepted and rejected operations.

use super::strict_array_contents::run_source;
use super::support::FakeValue;
use elephc_semantics::StrictFeature;

/// An opaque declared mixed producer needs a numeric proof even when its current result is integer.
#[test]
fn strict_mixed_runtime_numeric_sources_require_narrowing() {
    let source = "function opaque_number():mixed{return 1;} \
        $value=opaque_number();try{echo $value+1;}catch(TypeError $error){echo 'M';} \
        if(is_int($value)){echo $value+1;}return 1;";
    for dispatch in [false, true] {
        for features in [vec![StrictFeature::Mixed], vec![StrictFeature::Mixed, StrictFeature::ArrayTypes]] {
        let (output, returned) = run_source(source, None, &features, dispatch);
        assert_eq!(output, "M2"); assert_eq!(returned, FakeValue::Int(1));
        }
    }
}

/// A numeric union is usable directly while an unsupported alternative must be narrowed first.
#[test]
fn strict_mixed_runtime_union_domains_preserve_every_alternative() {
    let source = "function closed_number(int|float $value){return $value+1;} \
        function partial_number(int|array $value){try{echo $value+1;}catch(TypeError $error){echo 'U';} \
        if(is_int($value)){echo $value+1;}return 1;} \
        echo closed_number(1),':';return partial_number(1);";
    for dispatch in [false, true] {
        for features in [vec![StrictFeature::Mixed], vec![StrictFeature::Mixed, StrictFeature::ArrayTypes]] {
        let (output, returned) = run_source(source, None, &features, dispatch);
        assert_eq!(output, "2:U2"); assert_eq!(returned, FakeValue::Int(1));
        }
    }
}

/// A predicate proof expires after an alias changes the shared value to another compatible PHP operand.
#[test]
fn strict_mixed_runtime_alias_writes_retire_numeric_guards() {
    let source = "function opaque_guard_number():mixed{return 1;} \
        function replace_guard_number(&$value){$value='2';} \
        $value=opaque_guard_number();if(is_int($value)){replace_guard_number($value); \
        try{echo $value+1;}catch(TypeError $error){echo 'R';}}return 1;";
    for dispatch in [false, true] {
        for features in [vec![StrictFeature::Mixed], vec![StrictFeature::Mixed, StrictFeature::ArrayTypes]] {
        let (output, returned) = run_source(source, None, &features, dispatch);
        assert_eq!(output, "R"); assert_eq!(returned, FakeValue::Int(1));
        }
    }
}

/// Unselected profiles retain ordinary PHP arithmetic with the same runtime-provided producer.
#[test]
fn strict_mixed_runtime_unselected_producer_control() {
    let source = "function ordinary_opaque_number():mixed{return 1;}$value=ordinary_opaque_number();return $value+1;";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[], dispatch);
        assert!(output.is_empty()); assert_eq!(returned, FakeValue::Int(2));
    }
}

/// A predicate narrows an explicitly mixed declaration and survives value-preserving alias creation.
#[test]
fn strict_mixed_runtime_alias_creation_preserves_predicate_role() {
    let source = "mixed $value=1;try{echo $value+1;}catch(TypeError $error){echo 'M';} \
        if(is_int($value)){$alias=&$value;echo $value+1;}return 1;";
    for dispatch in [false, true] {
        for features in [vec![StrictFeature::Mixed], vec![StrictFeature::Mixed, StrictFeature::ArrayTypes]] {
        let (output, returned) = run_source(source, None, &features, dispatch);
        assert_eq!(output, "M2"); assert_eq!(returned, FakeValue::Int(1));
        }
    }
}

/// VM-eligible fragments and direct bound-body calls consistently select the source-proof walker.
#[test]
fn strict_mixed_runtime_records_walker_selection_for_compiled_bodies() {
    use super::super::bytecode;
    use elephc_semantics::SemanticProfile;
    let source = "function proof_body(){return 1+2;}return proof_body();";
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Mixed, true);
    let program = crate::parser::parse_fragment_with_semantics(source.as_bytes(), profile).unwrap();
    assert!(bytecode::compile_program(&program).is_ok());
    for dispatch in [false, true] {
        let (_, returned) = run_source(source, None, &[], true);
        assert_eq!(returned, FakeValue::Int(3)); assert!(bytecode::last_execution_used_vm());
        let (_, returned) = run_source(source, None, &[StrictFeature::Mixed], dispatch);
        assert_eq!(returned, FakeValue::Int(3)); assert!(!bytecode::last_execution_used_vm());
    }
}

/// The existing user-facing numeric proof example also runs through the interpreter source consumers.
#[test]
fn strict_mixed_runtime_executes_documented_numeric_proof_example() {
    let example = include_str!("../../../../../examples/strict-mixed/main.php");
    let source = format!("{} return 1;", example.strip_prefix("<?php").unwrap());
    for dispatch in [false, true] {
        for features in [vec![StrictFeature::Mixed], vec![StrictFeature::Mixed, StrictFeature::ArrayTypes]] {
            let (output, returned) = run_source(&source, None, &features, dispatch);
            assert_eq!(output, "6:0:3.5"); assert_eq!(returned, FakeValue::Int(1));
        }
    }
}
