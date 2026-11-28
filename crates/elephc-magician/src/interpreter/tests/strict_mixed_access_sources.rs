//! Purpose:
//! Verifies source proofs at ordinary property and offset read boundaries.
//!
//! Called from:
//! - Focused Mixed consumer and independent-profile regressions.
//!
//! Key details:
//! - Shared postfix handling must preserve unknown contents and custom key domains.

use super::strict_array_contents::run_source;
use super::support::FakeValue;
use elephc_semantics::StrictFeature;

/// Ordinary property reads require a receiver proof and retain selected member declarations.
#[test]
fn strict_mixed_access_sources_require_property_receiver_narrowing() {
    let source="class MixedAccessMember{public int $value=2;public mixed $dynamic=2;} \
        function mixed_access_object():mixed{return new MixedAccessMember();} \
        $object=mixed_access_object();try{$value=$object->value;}catch(TypeError $error){echo 'R';} \
        if(is_object($object)){echo $object->value+1;} \
        $object=new MixedAccessMember();$name='value';echo $object->$name+1; \
        try{echo $object->dynamic+1;}catch(TypeError $error){echo 'M';}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"R33M");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Array receivers and PHP keys need narrowing while guarded array elements remain independently unknown.
#[test]
fn strict_mixed_access_sources_require_offset_receiver_and_key_narrowing() {
    let source="function mixed_access_array():mixed{return [2];}function mixed_access_key():mixed{return 0;} \
        $items=mixed_access_array();try{$value=$items[0];}catch(TypeError $error){echo 'R';} \
        if(is_array($items)){$entry=$items[0];try{echo $entry+1;}catch(TypeError $error){echo 'E';} \
            if(is_int($entry)){echo $entry+1;}} \
        array<int> $typed=[2];$key=mixed_access_key();try{$value=$typed[$key];}catch(TypeError $error){echo 'K';} \
        if(is_int($key)){echo $typed[$key]+1;}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"RE3K3");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// A custom ArrayAccess receiver accepts its own arbitrary key domain and retains unknown get results.
#[test]
fn strict_mixed_access_sources_preserve_custom_array_access_key_domains() {
    let source="class MixedAccessOffsets implements ArrayAccess{ \
        public function offsetExists(mixed $offset):bool{return true;} \
        public function offsetGet(mixed $offset):mixed{return 2;} \
        public function offsetSet(mixed $offset,mixed $value):void{}public function offsetUnset(mixed $offset):void{}} \
        function mixed_access_custom_key():mixed{return new stdClass();} \
        $object=new MixedAccessOffsets();$key=mixed_access_custom_key();$result=$object[$key]; \
        try{echo $result+1;}catch(TypeError $error){echo 'U';}if(is_int($result)){echo $result+1;}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"U3");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Key and member selector effects occur before their reached read fails; call arguments remain skipped.
#[test]
fn strict_mixed_access_sources_preserve_operand_effect_order() {
    let source="class MixedAccessOrder{public int $value=2;public function read(int $ignored):int{return 2;}} \
        function mixed_access_order_array():mixed{return [2];} \
        function mixed_access_order_object():mixed{return new MixedAccessOrder();} \
        function mixed_access_order_key():int{echo 'K';return 0;} \
        function mixed_access_order_property():string{echo 'P';return 'value';} \
        function mixed_access_order_method():string{echo 'N';return 'read';} \
        function mixed_access_order_argument():int{echo 'A';return 0;} \
        $items=mixed_access_order_array();try{$value=$items[mixed_access_order_key()];}catch(TypeError $error){echo 'R';} \
        $object=mixed_access_order_object();try{$value=$object->{mixed_access_order_property()};}catch(TypeError $error){echo 'R';} \
        try{$object->{mixed_access_order_method()}(mixed_access_order_argument());}catch(TypeError $error){echo 'R';}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"KRPRNR");assert_eq!(returned,FakeValue::Int(1));
    }
}
