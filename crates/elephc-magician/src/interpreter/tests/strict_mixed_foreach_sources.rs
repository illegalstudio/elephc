//! Purpose:
//! Verifies logical iterable and loop binding sources under Mixed.
//!
//! Called from:
//! - Focused foreach source, publication and ownership regressions.
//!
//! Key details:
//! - Declared contents remain distinct from actual scalar occupants.

use super::strict_array_contents::run_source;
use super::support::FakeValue;
use elephc_semantics::StrictFeature;

/// An opaque iterable needs narrowing and its elements remain unknown after an array guard.
#[test]
fn strict_mixed_foreach_sources_require_iterable_narrowing() {
    let source="function mixed_foreach_origin():mixed{return [2];}$items=mixed_foreach_origin(); \
        try{foreach($items as $value){echo 'B';}}catch(TypeError $error){echo 'R';} \
        if(is_array($items)){foreach($items as $value){ \
            try{echo $value+1;}catch(TypeError $error){echo 'E';}if(is_int($value)){echo $value+1;}}}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"RE3");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Declared array keys and values retain their logical domains independently from Locals and ArrayTypes.
#[test]
fn strict_mixed_foreach_sources_publish_declared_array_domains() {
    let source="array<int> $items=[2];foreach($items as $key=>$value){echo $key+1,$value+1;} \
        array<mixed> $open=[2];foreach($open as $value){ \
            try{echo $value+1;}catch(TypeError $error){echo 'M';}}return 1;";
    for dispatch in [false,true] {
        for features in [vec![StrictFeature::Mixed],vec![StrictFeature::Mixed,StrictFeature::ArrayTypes],
            vec![StrictFeature::Mixed,StrictFeature::Locals]] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,"13M");assert_eq!(returned,FakeValue::Int(1));
        }
    }
}

/// Loop keys and values share PHP publication order when the same destination is named twice.
#[test]
fn strict_mixed_foreach_sources_preserve_shared_destination_and_empty_loops() {
    let source="array<string,int> $items=['key'=>2];foreach($items as $same=>$same){echo $same.'!';} \
        $unchanged='saved';array<int> $empty=[];foreach($empty as $unchanged){} \
        echo ':',$unchanged.'!';return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"key!:saved!");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Iterator and aggregate methods supply their own declared keys and values, including boolean predicates.
#[test]
fn strict_mixed_foreach_sources_preserve_iterator_and_aggregate_declarations() {
    let source="class MixedSourceIterator implements Iterator{public int $position=0; \
        public function rewind():void{$this->position=0;}public function valid():bool{return $this->position<1;} \
        public function current():int{return 2;}public function key():string{return 'k';} \
        public function next():void{$this->position=1;}} \
        class MixedSourceAggregate implements IteratorAggregate{public function getIterator():Traversable{return new MixedSourceIterator();}} \
        foreach(new MixedSourceIterator() as $key=>$value){echo $key.'!',$value+1;} \
        foreach(new MixedSourceAggregate() as $key=>$value){echo $key.'!',$value+1;}return 1;";
    for dispatch in [false,true] {
        for features in [vec![StrictFeature::Mixed],vec![StrictFeature::Mixed,StrictFeature::Bools]] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,"k!3k!3");assert_eq!(returned,FakeValue::Int(1));
        }
    }
}

/// Explicit mixed iterator returns never acquire narrower facts from their actual occupants.
#[test]
fn strict_mixed_foreach_sources_preserve_unknown_iterator_value_domains() {
    let source="class MixedUnknownIterator implements Iterator{public int $position=0; \
        public function rewind():void{$this->position=0;}public function valid():bool{return $this->position<1;} \
        public function current():mixed{return 2;}public function key():mixed{return 'k';} \
        public function next():void{$this->position=1;}} \
        foreach(new MixedUnknownIterator() as $key=>$value){ \
            try{echo $value+1;}catch(TypeError $error){echo 'V';} \
            try{echo $key.'!';}catch(TypeError $error){echo 'K';} \
            if(is_int($value)){echo $value+1;}}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"VK3");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Loop content origins remain conditional and never enable a Locals whole-kind guarantee.
#[test]
fn strict_mixed_foreach_sources_keep_array_contents_independent_from_local_kinds() {
    let source="array<array<int>> $items=[[1]];foreach($items as $copy){ \
        try{$copy[]='bad';}catch(TypeError $error){echo 'C';}$copy=7;}return $copy;";
    for dispatch in [false,true] {
        for features in [vec![StrictFeature::ArrayTypes],vec![StrictFeature::Mixed,StrictFeature::ArrayTypes]] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,"C");assert_eq!(returned,FakeValue::Int(7));
        }
    }
}

/// An untyped valid method stays unknown only when both Mixed and Bools require a logical bool proof.
#[test]
fn strict_mixed_foreach_sources_boolean_predicates_keep_independent_profiles() {
    let source="class MixedUntypedValidIterator implements Iterator{ \
        public function rewind():void{}public function valid(){return true;} \
        public function current():int{return 2;}public function key():mixed{return null;}public function next():void{}} \
        try{foreach(new MixedUntypedValidIterator() as $value){echo 'B';break;}} \
        catch(TypeError $error){echo 'V';}return 1;";
    for dispatch in [false,true] {
        for (features,expected) in [(vec![StrictFeature::Mixed],"B"),
            (vec![StrictFeature::Mixed,StrictFeature::Bools],"V")] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,expected);assert_eq!(returned,FakeValue::Int(1));
        }
    }
}
