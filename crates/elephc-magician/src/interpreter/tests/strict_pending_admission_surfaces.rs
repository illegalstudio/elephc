//! Purpose:
//! Exercises declaration source binds and predicate views needed for public strict profile admission.
//!
//! Called from:
//! - Focused Mixed interpreter boundary regressions.
//!
//! Key details:
//! - Each case runs through both dispatch modes and checks refused writes remain unobserved.

use super::strict_array_contents::run_source;
use super::support::FakeValue;
use elephc_semantics::StrictFeature;

/// Member reference binding checks the original source declaration before attaching a compatible occupant.
#[test]
fn strict_admission_member_reference_sources_require_narrowing() {
    let source="class SourceReferenceMember{public int $value=0;public static int $cached=0;} \
        function opaqueMemberReference():mixed{return 1;} $object=new SourceReferenceMember(); \
        $value=opaqueMemberReference();$cached=opaqueMemberReference(); \
        try{$object->value=&$value;}catch(TypeError $error){echo 'I';} \
        try{SourceReferenceMember::$cached=&$cached;}catch(TypeError $error){echo 'S';} \
        if(is_int($value)){$object->value=&$value;}$value=2;return $object->value;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"IS");assert_eq!(returned,FakeValue::Int(2));
    }
}

/// A physical member predicate proves that same live storage while the object and source remain unchanged.
#[test]
fn strict_admission_member_predicates_narrow_live_physical_storage() {
    let source="class GuardedSourceMember{public mixed $value=1;}$object=new GuardedSourceMember(); \
        try{if(is_int($object->value)){echo $object->value+1;}}catch(TypeError $error){echo 'E';}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"2");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Global array predicates narrow the selected global binding without imposing a stable local kind.
#[test]
fn strict_admission_global_predicates_narrow_selected_binding() {
    let source="function opaqueGuardedGlobal():mixed{return 1;}$GLOBALS['guardedNumber']=opaqueGuardedGlobal(); \
        try{if(is_int($GLOBALS['guardedNumber'])){echo $GLOBALS['guardedNumber']+1;}}catch(TypeError $error){echo 'E';}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"2");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Projection guards expire on alias writes, root replacement and reached user calls.
#[test]
fn strict_admission_projection_guards_expire_before_changed_member_reads() {
    let source="class MutableGuardMember{public mixed $value=1;} \
        function touchGuardMember(MutableGuardMember $object):void{$object->value='7';} \
        $object=new MutableGuardMember();$alias=$object; \
        if(is_int($object->value)){$alias->value='7';try{echo $object->value+1;}catch(TypeError $error){echo 'A';}} \
        $object=new MutableGuardMember();if(is_int($object->value)){touchGuardMember($object); \
        try{echo $object->value+1;}catch(TypeError $error){echo 'C';}} \
        $object=new MutableGuardMember();if(is_int($object->value)){$object=new MutableGuardMember(); \
        $object->value='7';try{echo $object->value+1;}catch(TypeError $error){echo 'R';}}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"ACR");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Literal projection keys use the same numeric-string normalization policy as their actual array slots.
#[test]
fn strict_admission_array_projection_guards_share_semantic_key_identity() {
    let source="function opaqueGuardedElement():mixed{return 1;} \
        $items=['0'=>opaqueGuardedElement(),0=>opaqueGuardedElement()]; \
        if(is_int($items['0'])){echo $items['0']+1;try{echo $items[0]+1;}catch(TypeError $error){echo 'K';}}return 1;";
    for dispatch in [false,true] {
        for (features,expected) in [(vec![StrictFeature::Mixed],"22"),
            (vec![StrictFeature::Mixed,StrictFeature::ArrayKeys],"2K")] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,expected);assert_eq!(returned,FakeValue::Int(1));
        }
    }
}

/// Repeated getter calls do not inherit a physical storage proof from a previous predicate value.
#[test]
fn strict_admission_getter_results_never_become_repeatable_member_guards() {
    let source="class ChangingGuardGetter{private int $calls=0;public mixed $value{get{ \
        $this->calls++;return $this->calls===1?1:'7';}}}$object=new ChangingGuardGetter(); \
        if(is_int($object->value)){try{echo $object->value+1;}catch(TypeError $error){echo 'G';}}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"G");assert_eq!(returned,FakeValue::Int(1));
    }
}
