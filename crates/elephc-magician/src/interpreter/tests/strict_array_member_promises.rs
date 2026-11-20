//! Purpose:
//! Checks array property obligations across escaped open local aliases and binding retirement.
//!
//! Called from:
//! - Focused ArrayTypes and Mixed declaration-promise regressions.
//!
//! Key details:
//! - An explicitly open local must still respect a live closed property owner.

use super::strict_array_contents::run_source;
use super::support::FakeValue;
use elephc_semantics::StrictFeature;

/// Closed property contents constrain open local aliases while the declaring owner remains live.
#[test]
fn strict_array_member_promises_constrain_open_local_alias_mutations() {
    let source="class ClosedArrayMember{public array<int> $items=[1];public static array<int> $cached=[2];} \
        $object=new ClosedArrayMember();array $instance=[1];array $cached=[2]; \
        $object->items=&$instance;ClosedArrayMember::$cached=&$cached; \
        try{$instance[]='bad';}catch(TypeError $error){echo 'I';} \
        try{$cached[]='bad';}catch(TypeError $error){echo 'S';} \
        echo count($instance),count($cached);return 1;";
    for dispatch in [false,true] {
        for features in [vec![StrictFeature::Mixed],vec![StrictFeature::ArrayTypes],
            vec![StrictFeature::Mixed,StrictFeature::ArrayTypes]] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,"IS11");assert_eq!(returned,FakeValue::Int(1));
        }
    }
}

/// Successful default initialization supplies the same scalar kind promised by property metadata.
#[test]
fn strict_array_member_promises_preserve_float_default_kinds() {
    let source="class FloatDefaultMember{public float $number=0;public static float $cached=0;} \
        $object=new FloatDefaultMember();echo gettype($object->number),':',gettype(FloatDefaultMember::$cached);return 1;";
    for dispatch in [false,true] {
        for features in [vec![],vec![StrictFeature::Mixed],vec![StrictFeature::Coercions]] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,"double:double");assert_eq!(returned,FakeValue::Int(1));
        }
    }
}

/// Visiting an untyped reference parameter never makes a retired property's obligation permanent.
#[test]
fn strict_array_member_promises_retire_after_reference_parameter_visits() {
    let source="class RetiredArrayPromise{public array<int> $items=[1];} \
        function visitArrayPromise(&$items):void{$items[]=3;} \
        $object=new RetiredArrayPromise();array $items=[1];$object->items=&$items; \
        visitArrayPromise($items);unset($object->items);$items[]='allowed';return count($items);";
    for dispatch in [false,true] {
        for features in [vec![StrictFeature::Mixed],vec![StrictFeature::ArrayTypes]] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,"");assert_eq!(returned,FakeValue::Int(3));
        }
    }
}
