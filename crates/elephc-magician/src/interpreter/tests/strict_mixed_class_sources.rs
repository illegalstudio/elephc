//! Purpose:
//! Verifies logical source boundaries for dynamic class consumers.
//!
//! Called from:
//! - Focused Mixed constructor, static access and callable selector regressions.
//!
//! Key details:
//! - Class resolution precedes member selectors and later arguments.

use super::strict_array_contents::run_source;
use super::support::FakeValue;
use elephc_semantics::StrictFeature;

/// Dynamic construction, static reads and first-class methods require proven class selector domains.
#[test]
fn strict_mixed_class_sources_require_narrowing_across_read_consumers() {
    let source="class MixedClassReads{public int $value=2;public static int $cached=3; \
        public const ANSWER=4;public static function read():int{return 5;}} \
        function mixed_class_origin():mixed{return 'MixedClassReads';}$class=mixed_class_origin(); \
        try{$object=new $class();}catch(TypeError $error){echo 'N';} \
        try{echo $class::$cached;}catch(TypeError $error){echo 'P';} \
        try{echo $class::ANSWER;}catch(TypeError $error){echo 'K';} \
        try{$callback=$class::read(...);}catch(TypeError $error){echo 'F';} \
        if(is_string($class)){$object=new $class();echo $object->value+1,$class::$cached+1,$class::ANSWER+1; \
            $callback=$class::read(...);echo $callback()+1;}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"NPKF3456");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Class source failures precede dynamic selectors while reached selectors still execute exactly once.
#[test]
fn strict_mixed_class_sources_preserve_class_resolution_order() {
    let source="class MixedClassOrder{public static int $cached=3;public static function read():int{return 4;}} \
        function mixed_class_order_origin():mixed{return 'MixedClassOrder';} \
        function mixed_class_property():string{echo 'P';return 'cached';} \
        function mixed_class_method():string{echo 'M';return 'read';}$class=mixed_class_order_origin(); \
        try{echo $class::${mixed_class_property()};}catch(TypeError $error){echo 'R';} \
        try{$callback=$class::{mixed_class_method()}(...);}catch(TypeError $error){echo 'F';} \
        if(is_string($class)){echo $class::${mixed_class_property()}+1; \
            $callback=$class::{mixed_class_method()}(...);echo $callback()+1;}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"RFP4M5");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Static writes share class proofs, leave rejected storage unchanged and accept guarded selectors.
#[test]
fn strict_mixed_class_sources_validate_static_mutation_selectors() {
    let source="class MixedClassWrites{public static int $cached=3;public static array $items=[1];} \
        function mixed_class_write_origin():mixed{return 'MixedClassWrites';}$class=mixed_class_write_origin(); \
        try{$class::$cached=4;}catch(TypeError $error){echo 'S';} \
        try{$class::$items[]=2;}catch(TypeError $error){echo 'A';} \
        try{$class::$items[0]=2;}catch(TypeError $error){echo 'I';} \
        try{$class::$cached++;}catch(TypeError $error){echo 'U';} \
        if(is_string($class)){$class::$cached=5;$class::$items[]=2;} \
        echo MixedClassWrites::$cached,count(MixedClassWrites::$items);return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"SAIU52");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Selected constructors retain nominal results and object class-name consumers still need a whole-kind view.
#[test]
fn strict_mixed_class_sources_preserve_constructor_results_and_object_class_names() {
    let source="class MixedConstructedClass{public int $value=2;public const ANSWER=4;} \
        function mixed_class_factory(string $class):MixedConstructedClass{return new $class();} \
        function mixed_class_object_origin():mixed{return new MixedConstructedClass();} \
        echo mixed_class_factory('MixedConstructedClass')->value+1,MixedConstructedClass::ANSWER+1; \
        $object=mixed_class_object_origin();try{echo $object::class;}catch(TypeError $error){echo 'C';} \
        if(is_object($object)){echo ':',$object::class.'!';}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"35C:MixedConstructedClass!");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Runtime property-name mutations keep rejected storage unchanged and reuse normal reference/unset behavior.
#[test]
fn strict_mixed_class_sources_share_dynamic_property_name_mutations() {
    let source="class MixedNamedClassWrites{public static int $counter=1;public static array $items=[1];public static mixed $value=1;} \
        function mixed_named_class_origin():mixed{return 'MixedNamedClassWrites';}$class=mixed_named_class_origin(); \
        $field='counter';try{$class::${$field}=4;}catch(TypeError $error){echo 'S';} \
        try{$class::${$field}++;}catch(TypeError $error){echo 'U';} \
        $field='items';try{$class::${$field}[]=2;}catch(TypeError $error){echo 'A';} \
        try{$class::${$field}[0]=2;}catch(TypeError $error){echo 'I';} \
        $field='value';$input=7;try{$class::${$field}=&$input;}catch(TypeError $error){echo 'R';} \
        try{unset($class::${$field});}catch(TypeError $error){echo 'D';} \
        if(is_string($class)){$field='counter';$class::${$field}=5;$class::${$field}++; \
            $field='items';$class::${$field}[0]=2;$class::${$field}[]=3; \
            $field='value';$class::${$field}=&$input;$input=8;echo MixedNamedClassWrites::$value;try{unset($class::${$field});}catch(Error $error){echo 'Z';}} \
        echo MixedNamedClassWrites::$counter,count(MixedNamedClassWrites::$items);return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"SUAIRD8Z62");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Temporary class objects stay alive through the consumer and retire after success or a thrown RHS.
#[test]
fn strict_mixed_class_sources_release_temporary_class_owners() {
    let source="class MixedClassLease{public static mixed $value=2;public function __destruct(){echo 'D';}} \
        function mixed_class_lease():MixedClassLease{return new MixedClassLease();} \
        function mixed_class_lease_throw():int{throw new RuntimeException('rhs');} \
        echo mixed_class_lease()::$value;mixed_class_lease()::$value=3;echo MixedClassLease::$value; \
        try{mixed_class_lease()::$value=mixed_class_lease_throw();}catch(RuntimeException $error){echo 'E';}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"D2D3DE");assert_eq!(returned,FakeValue::Int(1));
    }
}
