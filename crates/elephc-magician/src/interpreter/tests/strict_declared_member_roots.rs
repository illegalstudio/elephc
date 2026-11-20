//! Purpose:
//! Verifies maintained structural property declarations through defaults and nested reference writes.
//!
//! Called from:
//! - Focused checked-property profiles without requiring inferred array policy.
//!
//! Key details:
//! - Conditional contents validate candidates without joining weak scalar coercion arbitration.

use super::strict_array_contents::run_source;
use super::support::FakeValue;
use elephc_semantics::StrictFeature;

/// Structural default promises are checked before a successful initialized getter can expose them.
#[test]
fn strict_member_sources_validate_declared_structural_defaults() {
    let source="class InvalidArrayDefault{public array<int> $items=['bad'];} \
        try{$object=new InvalidArrayDefault();}catch(TypeError $error){echo 'D';}return 1;";
    for dispatch in [false,true] {
        for feature in [StrictFeature::Mixed,StrictFeature::ArrayTypes,StrictFeature::Casts] {
            let (output,returned)=run_source(source,None,&[feature],dispatch);
            assert_eq!(output,"D");assert_eq!(returned,FakeValue::Int(1));
        }
    }
}

/// Declared contents constrain nested references without requiring inferred array policy.
#[test]
fn strict_member_sources_preserve_declared_reference_element_promises() {
    let source="class DeclaredArraySource{public array<int> $items=[1];public static array<int> $cached=[2];} \
        $object=new DeclaredArraySource();$instance=1;$cached=2; \
        $object->items=[&$instance];DeclaredArraySource::$cached=[&$cached]; \
        try{$instance='bad';}catch(TypeError $error){echo 'I';} \
        try{$cached='bad';}catch(TypeError $error){echo 'S';} \
        unset($object->items);$next=3;$object->items=[&$next]; \
        try{$next='bad';}catch(TypeError $error){echo 'R';}return 1;";
    for dispatch in [false,true] {
        for feature in [StrictFeature::Mixed,StrictFeature::ArrayTypes,StrictFeature::Casts] {
            let (output,returned)=run_source(source,None,&[feature],dispatch);
            assert_eq!(output,"ISR");assert_eq!(returned,FakeValue::Int(1));
        }
    }
}

/// A conditional content obligation cannot veto a valid scalar conversion selected by a real declaration.
#[test]
fn strict_member_sources_keep_conditional_contents_out_of_scalar_arbitration() {
    let source="class ConditionalMemberSource{public array<int>|string $items=[1];} \
        $object=new ConditionalMemberSource();mixed $value=[1];$object->items=&$value; \
        $value=12;echo gettype($value),':',$value;return 1;";
    for dispatch in [false,true] {
        for feature in [StrictFeature::Mixed,StrictFeature::ArrayTypes,StrictFeature::Casts] {
            let (output,returned)=run_source(source,None,&[feature],dispatch);
            assert_eq!(output,"string:12");assert_eq!(returned,FakeValue::Int(1));
        }
    }
}
