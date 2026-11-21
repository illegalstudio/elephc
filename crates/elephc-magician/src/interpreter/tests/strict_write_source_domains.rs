//! Purpose:
//! Exercises original operand domains at mutable array and member consumers.
//!
//! Called from:
//! - Focused Mixed write-domain and operand lifetime regressions.
//!
//! Key details:
//! - Guarded writes use the same receiver, key and exact element domains across storage shapes.

use super::strict_array_contents::run_source;
use super::support::FakeValue;
use elephc_semantics::StrictFeature;

/// Writes require the same receiver and PHP key source domains as corresponding reads.
#[test]
fn strict_member_write_domains_require_receiver_and_key_narrowing() {
    let source="class WriteDomainMember{public int $number=1;} \
        function opaqueWriteReceiver():mixed{return new WriteDomainMember();} \
        function opaqueWriteKey():mixed{return 0;} \
        $object=opaqueWriteReceiver();try{$object->number=2;}catch(TypeError $error){echo 'R';} \
        if(is_object($object)){$object->number=3;echo $object->number;} \
        array<int> $items=[1];$key=opaqueWriteKey();try{$items[$key]=2;}catch(TypeError $error){echo 'K';} \
        if(is_int($key)){$items[$key]=3;echo $items[0];}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"R3K3");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// A declared element input domain must use its original producer before storing a compatible occupant.
#[test]
fn strict_member_write_domains_require_typed_element_source_narrowing() {
    let source="function opaqueWriteElement():mixed{return 12;} \
        array<int> $items=[1];$value=opaqueWriteElement(); \
        try{$items[]=$value;}catch(TypeError $error){echo 'V';} \
        if(is_int($value)){$items[]=$value;}return count($items);";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"V");assert_eq!(returned,FakeValue::Int(2));
    }
}

/// Computed and nested receivers preserve their selected declaration through mutable traversal.
#[test]
fn strict_member_write_domains_keep_computed_and_nested_receiver_proofs() {
    let source="class ProvenWriteReceiver{public int $number=1;} \
        class ProvenWriteHolder{public ProvenWriteReceiver $child; \
        public function __construct(){$this->child=new ProvenWriteReceiver();}} \
        function provenWriteReceiver():ProvenWriteReceiver{return new ProvenWriteReceiver();} \
        $factory=provenWriteReceiver(...);$factory()->number=2; \
        $holder=new ProvenWriteHolder();$holder->child->number=3; \
        $property='child';$holder->{$property}->number=4;return $holder->child->number;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"");assert_eq!(returned,FakeValue::Int(4));
    }
}

/// Every typed dimension destination consumes the original RHS and key source before changing contents.
#[test]
fn strict_member_write_domains_cover_instance_static_and_nested_dimensions() {
    let source="class TypedWriteDimensions{public array<int> $items=[1];public static array<int> $cached=[1];} \
        function opaqueDimensionValue():mixed{return 2;}function opaqueDimensionKey():mixed{return 0;} \
        $object=new TypedWriteDimensions();$value=opaqueDimensionValue();$key=opaqueDimensionKey(); \
        try{$object->items[]=$value;}catch(TypeError $error){echo 'I';} \
        try{TypedWriteDimensions::$cached[]=$value;}catch(TypeError $error){echo 'S';} \
        try{$object->items[$key]=3;}catch(TypeError $error){echo 'K';} \
        try{TypedWriteDimensions::$cached[$key]=3;}catch(TypeError $error){echo 'L';} \
        array<array<int>> $matrix=[[1]];try{$matrix[0][]=$value;}catch(TypeError $error){echo 'N';} \
        if(is_int($value)&&is_int($key)){$object->items[$key]=$value;TypedWriteDimensions::$cached[$key]=$value;$matrix[0][]=$value;} \
        return count($object->items)+count(TypedWriteDimensions::$cached)+count($matrix[0]);";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"ISKLN");assert_eq!(returned,FakeValue::Int(4));
    }
}

/// Array element declarations require exact alternatives independently from PHP weak scalar parameter binding.
#[test]
fn strict_member_write_domains_keep_array_elements_exact_under_weak_scalar_binding() {
    let source="function unionWriteElement(bool $number):int|string{return $number?1:'one';} \
        function weakWriteScalar(int $value):int{return $value;} \
        array<int> $items=[0];$value=unionWriteElement(true); \
        try{$items[]=$value;}catch(TypeError $error){echo 'E';} \
        echo weakWriteScalar($value);if(is_int($value)){$items[]=$value;}return count($items);";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"E1");assert_eq!(returned,FakeValue::Int(2));
    }
}
