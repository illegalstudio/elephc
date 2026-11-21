//! Purpose:
//! Verifies builtin input domains and source proof forwarding through shared callable adapters.
//!
//! Called from:
//! - Focused Mixed builtin, callback and normalized argument regressions.
//!
//! Key details:
//! - Predicate guards preserve proofs through the same owner boundary as direct values.
//! - Intentional mixed formatting inputs stay unrestricted.

use super::strict_array_contents::run_source;
use super::support::FakeValue;
use elephc_semantics::StrictFeature;

/// Ordinary builtin parameters reject opaque inputs and preserve guarded callback argument proofs.
#[test]
fn strict_mixed_builtin_sources_validate_inputs_and_keep_callback_proofs() {
    let source="function builtin_origin():mixed{return 'abc';} \
        function builtin_target(string $value):int{return strlen($value);} \
        $value=builtin_origin();try{strlen($value);}catch(TypeError $error){echo 'S';} \
        if(is_string($value)){echo strlen($value),call_user_func('builtin_target',$value),call_user_func('strlen',$value);}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"S333");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// A countable contract proves the receiver without restricting intentionally mixed array elements.
#[test]
fn strict_mixed_builtin_sources_validate_countable_domains() {
    let source="function count_origin():mixed{return [1];} \
        $items=count_origin();try{count($items);}catch(TypeError $error){echo 'C';} \
        if(is_array($items)){echo count($items);}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"C1");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Join overloads prove array inputs while preserving weak scalar separator alternatives.
#[test]
fn strict_mixed_builtin_sources_preserve_join_overload_domains() {
    let source="function join_origin():mixed{return ['a','b'];} \
        function join_separator(int|string $glue):string{return implode($glue,['a','b']);} \
        $items=join_origin();try{implode($items);}catch(TypeError $error){echo 'I';} \
        if(is_array($items)){echo implode(',',$items),':',implode($items);}echo ':',join_separator(1);return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"Ia,b:ab:a1b");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Introspection intentionally accepts mixed inputs, and independent profiles retain weak builtins.
#[test]
fn strict_mixed_builtin_sources_keep_introspection_and_permissive_controls() {
    let source="function inspect_origin():mixed{return 'abc';}$value=inspect_origin(); \
        echo gettype($value);if(is_string($value)){echo ':',strlen($value);}return 1;";
    for dispatch in [false,true] {
        for features in [vec![],vec![StrictFeature::Mixed]] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,"string:3");assert_eq!(returned,FakeValue::Int(1));
        }
    }
}

/// Separator conversion uses weak binding unless the independent Coercions guarantee forbids it.
#[test]
fn strict_mixed_builtin_sources_separator_conversions_follow_profiles() {
    let source="try{echo implode(1,['a','b']);}catch(TypeError $error){echo 'C';}return 1;";
    for dispatch in [false,true] {
        for (features,expected) in [(vec![],"a1b"),(vec![StrictFeature::Mixed],"a1b"),
            (vec![StrictFeature::Mixed,StrictFeature::Coercions],"C")] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,expected);assert_eq!(returned,FakeValue::Int(1));
        }
    }
}
