//! Purpose:
//! Keeps unknown producer origins separate from explicit mixed and conditional array obligations.
//!
//! Called from:
//! - Focused Mixed and ArrayTypes composition regressions.
//!
//! Key details:
//! - Unknown origins do not grant deliberately open array element contracts.
//! - A successful array predicate remains a whole-kind proof alongside content metadata.

use super::strict_array_contents::run_source;
use super::support::FakeValue;
use elephc_semantics::StrictFeature;

/// Adding Mixed cannot turn an undeclared scalar producer into permission for heterogeneous arrays.
#[test]
fn strict_mixed_origins_unknown_and_explicit_mixed_remain_distinct() {
    let source = "function origin_unknown(){return 1;}function origin_open():mixed{return 1;} \
        $value=origin_unknown();$items=[$value]; \
        try{$items[]='bad';}catch(TypeError $error){echo 'I';} \
        $open=[origin_open()];$open[]='allowed';return count($items)+count($open);";
    for dispatch in [false, true] {
        for features in [vec![StrictFeature::ArrayTypes], vec![StrictFeature::ArrayTypes,StrictFeature::Mixed]] {
            let (output, returned) = run_source(source, None, &features, dispatch);
            assert_eq!(output,"I");assert_eq!(returned,FakeValue::Int(3));
        }
    }
}

/// Homogeneous literal arms retain their observed element contract through a runtime ternary.
#[test]
fn strict_mixed_origins_ternary_literals_retain_element_contracts() {
    let source = "$flag=true;$items=$flag?[1,2]:[3]; \
        try{$items[]='bad';}catch(TypeError $error){echo 'T';} \
        $flag=false;$items=$flag?[1,2]:[3]; \
        try{$items[]='bad';}catch(TypeError $error){echo 'F';}return count($items);";
    for dispatch in [false, true] {
        for features in [vec![StrictFeature::Locals, StrictFeature::ArrayTypes],
            vec![StrictFeature::Locals, StrictFeature::ArrayTypes, StrictFeature::Mixed]] {
            let (output, returned) = run_source(source, None, &features, dispatch);
            assert_eq!(output, "TF"); assert_eq!(returned, FakeValue::Int(1));
        }
    }
}

/// A missing ternary-arm contract defers to the selected value instead of borrowing its sibling's schema.
#[test]
fn strict_mixed_origins_ternary_partial_contracts_infer_the_selected_array() {
    let source = "$typed=[1,2];$flag=false;$items=$flag?$typed:['a']; \
        try{$items[]=2;}catch(TypeError $error){echo 'T';}$items[]='b'; \
        $other=$flag?1:['a'];try{$other[]=2;}catch(TypeError $error){echo 'S';} \
        $other[]='b';return count($items)+count($other);";
    for dispatch in [false, true] {
        for features in [vec![StrictFeature::Locals, StrictFeature::ArrayTypes],
            vec![StrictFeature::Locals, StrictFeature::ArrayTypes, StrictFeature::Mixed]] {
            let (output, returned) = run_source(source, None, &features, dispatch);
            assert_eq!(output, "TS"); assert_eq!(returned, FakeValue::Int(4));
        }
    }
}

/// Untyped array returns infer contents while an explicit mixed return remains open.
#[test]
fn strict_mixed_origins_direct_unknown_array_return_infers_contents() {
    let source = "function origin_unknown_array(){return [1];} \
        function origin_open_array():mixed{return [1];} \
        $items=origin_unknown_array();$itemsCount=0; \
        if(is_array($items)){try{$items[]='bad';}catch(TypeError $error){echo 'U';}$itemsCount=count($items);} \
        $open=origin_open_array();$openCount=0; \
        if(is_array($open)){$open[]='allowed';$openCount=count($open);}return $itemsCount+$openCount;";
    for dispatch in [false, true] {
        for features in [vec![StrictFeature::Locals, StrictFeature::ArrayTypes],
            vec![StrictFeature::Locals, StrictFeature::ArrayTypes, StrictFeature::Mixed]] {
            let (output, returned) = run_source(source, None, &features, dispatch);
            assert_eq!(output, "U"); assert_eq!(returned, FakeValue::Int(3));
        }
    }
}

/// Conditional element constraints cannot erase an observed outer array predicate at a typed call.
#[test]
fn strict_mixed_origins_array_guard_composes_with_contents() {
    let source = "function guarded_origin():mixed{return [1];} \
        function guarded_array(array $items):int{return count($items);} \
        $items=guarded_origin();try{guarded_array($items);}catch(TypeError $error){echo 'G';} \
        if(is_array($items)){echo guarded_array($items);}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed,StrictFeature::ArrayTypes],dispatch);
        assert_eq!(output,"G1");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// ArrayTypes inference cannot turn an unknown scalar occupant into a Mixed operation proof.
#[test]
fn strict_mixed_origins_unknown_scalar_stays_unproven_with_array_types() {
    let source="function scalar_unknown(){return 1;}$value=scalar_unknown(); \
        try{echo $value+1;}catch(TypeError $error){echo 'U';} \
        if(is_int($value)){echo $value+1;}return 1;";
    for dispatch in [false,true] {
        for features in [vec![StrictFeature::Mixed],vec![StrictFeature::Mixed,StrictFeature::ArrayTypes]] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,"U2");assert_eq!(returned,FakeValue::Int(1));
        }
    }
}
