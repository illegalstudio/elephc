//! Purpose:
//! Verifies per-entry logical sources at spread and callback-array boundaries.
//!
//! Called from:
//! - Focused Mixed argument normalization and shared reference regressions.
//!
//! Key details:
//! - A later tuple entry may invalidate an earlier reference proof before binding.
//! - Independent value entries keep their source-order snapshots.

use super::strict_array_contents::run_source;
use super::support::FakeValue;
use elephc_semantics::StrictFeature;

/// Later spread entries cannot refresh a proof captured for a mutated shared reference.
#[test]
fn strict_mixed_spread_sources_preserve_original_reference_stamps() {
    let source="function spread_origin():mixed{return 1;} \
        function spread_target(int &$value,$later):int{return $value;} \
        function spread_replace(&$value):bool{$value='2';return true;} \
        $value=spread_origin();if(is_int($value)){ \
        try{spread_target(...[&$value,spread_replace($value)]);}catch(TypeError $error){echo 'R';} \
        echo gettype($value);}return 1;";
    for dispatch in [false,true] {
        for features in [vec![StrictFeature::Mixed],vec![StrictFeature::Mixed,StrictFeature::ArrayTypes]] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,"Rstring");assert_eq!(returned,FakeValue::Int(1));
        }
    }
}

/// Callback arrays retain each element's declaration through invocation and reflection.
#[test]
fn strict_mixed_spread_sources_callback_arrays_keep_entry_proofs() {
    let source="function tuple_origin():mixed{return 1;}function tuple_target(int $value):int{return $value;} \
        $value=tuple_origin();try{call_user_func_array('tuple_target',[$value]);}catch(TypeError $error){echo 'M';} \
        if(is_int($value)){echo call_user_func_array('tuple_target',[$value]); \
        $reflection=new ReflectionFunction('tuple_target');echo $reflection->invokeArgs([$value]);}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"M11");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Inferred tuple schemas never substitute a logical proof for an undeclared element producer.
#[test]
fn strict_mixed_spread_sources_unknown_entries_do_not_inherit_inferred_schemas() {
    let source="function unknown_entry(){return 1;}function entry_target(int $value):int{return $value;} \
        try{entry_target(...[unknown_entry()]);}catch(TypeError $error){echo 'U';} \
        try{call_user_func_array('entry_target',[unknown_entry()]);}catch(TypeError $error){echo 'C';} \
        $reflection=new ReflectionFunction('entry_target'); \
        try{$reflection->invokeArgs([unknown_entry()]);}catch(TypeError $error){echo 'R';}return 1;";
    for dispatch in [false,true] {
        for features in [vec![StrictFeature::Mixed],vec![StrictFeature::Mixed,StrictFeature::ArrayTypes]] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,"UCR");assert_eq!(returned,FakeValue::Int(1));
        }
    }
}

/// Spread value arguments detach before later effects, while reference arguments keep canonical stamps.
#[test]
fn strict_mixed_spread_sources_later_named_effects_preserve_passing_modes() {
    let source="function later_origin():mixed{return 1;} \
        function later_value(int $value,$later):int{return $value;} \
        function later_reference(int &$value,$later):int{return $value;} \
        function later_replace(&$value):bool{$value='2';return true;} \
        $value=later_origin();if(is_int($value)){echo later_value(...[&$value],later:later_replace($value)),':',gettype($value),':';} \
        $value=later_origin();if(is_int($value)){try{later_reference(...[&$value],later:later_replace($value));} \
        catch(TypeError $error){echo 'R';}echo ':',gettype($value);}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"1:string:R:string");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Nested callback tuples retain unknown element proofs instead of promoting inferred data schemas.
#[test]
fn strict_mixed_spread_sources_nested_callback_tuples_keep_proof_roles() {
    let source="function nested_unknown_entry(){return 1;}function nested_entry_target(int $value):int{return $value;} \
        try{echo call_user_func_array('call_user_func_array',['nested_entry_target',[nested_unknown_entry()]]);} \
        catch(TypeError $error){echo 'M';} \
        echo call_user_func_array('call_user_func_array',['nested_entry_target',[1]]);return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"M1");assert_eq!(returned,FakeValue::Int(1));
    }
}
