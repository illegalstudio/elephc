//! Purpose:
//! Exercises pending property mutations and original operand domains at mutable array/member consumers.
//!
//! Called from:
//! - Focused ArrayTypes pending-origin and Mixed write-domain regressions.
//!
//! Key details:
//! - Declared bottom arrays stay closed; inferred empties wait for an informative successful write.

use super::strict_array_contents::run_source;
use super::support::FakeValue;
use elephc_semantics::StrictFeature;

/// First direct and builtin property mutations establish the same content contracts as whole stores.
#[test]
fn strict_pending_member_mutations_refine_first_element_and_builtin_writes() {
    let source="class PendingElementMember{public $items=[];public static $cached=[];} \
        $object=new PendingElementMember();$builtin=new PendingElementMember(); \
        $object->items[]=1;PendingElementMember::$cached[0]=2;array_push($builtin->items,3); \
        try{$object->items[]='bad';}catch(TypeError $error){echo 'I';} \
        try{PendingElementMember::$cached[]='bad';}catch(TypeError $error){echo 'S';} \
        try{$builtin->items[]='bad';}catch(TypeError $error){echo 'B';} \
        return count($object->items)+count(PendingElementMember::$cached)+count($builtin->items);";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::ArrayTypes],dispatch);
        assert_eq!(output,"ISB");assert_eq!(returned,FakeValue::Int(3));
    }
}

/// Explicit generated bottom contracts cannot become pending inference merely because their schemas are empty.
#[test]
fn strict_pending_member_consumers_keep_declared_bottom_arrays_closed() {
    let source="$items=[];try{$items[]=1;}catch(TypeError $error){echo 'R';}return count($items);";
    for dispatch in [false,true] {
        for arguments in [vec![elephc_generics::Type::atom("never")],
            vec![elephc_generics::Type::atom("never"),elephc_generics::Type::atom("never")]] {
            let schema=elephc_generics::Type::Apply("array".into(),arguments);
            let (output,returned)=super::strict_array_contents::run_source_with_type(source,Some(schema),&[StrictFeature::ArrayTypes],dispatch);
            assert_eq!(output,"R");assert_eq!(returned,FakeValue::Int(0));
        }
    }
}

/// Capturing logical literal proofs cannot activate inferred homogeneous contents when ArrayTypes is disabled.
#[test]
fn strict_pending_array_roles_keep_literal_inference_independent_from_mixed() {
    let source="function unknownNestedMemberArray(){return [1,'a'];} \
        class IndependentMemberLiteral{public $items;}$object=new IndependentMemberLiteral(); \
        try{$object->items=[unknownNestedMemberArray()];}catch(TypeError $error){echo 'H';}return 1;";
    for dispatch in [false,true] {
        for (features,expected) in [(vec![StrictFeature::Mixed],""),(vec![StrictFeature::ArrayTypes],"H")] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,expected);assert_eq!(returned,FakeValue::Int(1));
        }
    }
}
