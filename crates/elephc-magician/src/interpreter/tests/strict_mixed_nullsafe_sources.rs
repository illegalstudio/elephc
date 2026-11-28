//! Purpose:
//! Verifies logical sources and short-circuit identity through nullsafe postfix chains.
//!
//! Called from:
//! - Focused Mixed receiver and lazy source regressions.
//!
//! Key details:
//! - Skipped selectors provide no facts and acquire no runtime owners.

use super::strict_array_contents::run_source;
use super::support::FakeValue;
use elephc_semantics::StrictFeature;

/// Reached dynamic methods retain declared results and null short circuits skip later postfix effects.
#[test]
fn strict_mixed_nullsafe_sources_preserve_reached_results_and_skipped_effects() {
    let source="class MixedChainSources{public function read():int{return 2;} \
        public function next():self{return $this;}public function unknown(){return 2;}} \
        function mixed_chain_selector():string{echo 'E';return 'read';} \
        $object=new MixedChainSources();echo $object?->{mixed_chain_selector()}()+1; \
        $object=null;$object?->next()->{mixed_chain_selector()}(); \
        $object=new MixedChainSources();try{echo $object?->unknown()+1;}catch(TypeError $error){echo 'U';}return 1;";
    for dispatch in [false,true] {
        for features in [vec![StrictFeature::Mixed],vec![StrictFeature::Mixed,StrictFeature::Nullability]] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,"E3U");assert_eq!(returned,FakeValue::Int(1));
        }
    }
}

/// A mixed receiver needs an explicit view even when its actual value happens to be null.
#[test]
fn strict_mixed_nullsafe_sources_require_reached_receiver_proofs() {
    let source="class MixedChainGuard{public function read():int{return 2;}} \
        function mixed_chain_origin():mixed{return null;} \
        function mixed_chain_object():mixed{return new MixedChainGuard();} \
        $object=mixed_chain_origin();try{$object?->read();}catch(TypeError $error){echo 'N';} \
        if(is_null($object)){$object?->read();} \
        $object=mixed_chain_object();try{echo $object?->read()+1;}catch(TypeError $error){echo 'O';} \
        if(is_object($object)){echo $object?->read()+1;}return 1;";
    for dispatch in [false,true] {
        for features in [vec![StrictFeature::Mixed],vec![StrictFeature::Mixed,StrictFeature::Nullability]] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,"NO3");assert_eq!(returned,FakeValue::Int(1));
        }
    }
}

/// Initialized null results are consumed by the next hop and never masquerade as skipped hops.
#[test]
fn strict_mixed_nullsafe_sources_keep_returned_null_distinct_from_short_circuits() {
    let source="class MixedChainNull{public function next():?self{return null;}} \
        function mixed_chain_argument():int{echo 'A';return 0;} \
        $object=new MixedChainNull();try{$object?->next()->read(mixed_chain_argument());} \
        catch(TypeError $error){echo 'R';} \
        $object=null;$object?->next()->read(mixed_chain_argument());return 1;";
    for dispatch in [false,true] {
        for features in [vec![StrictFeature::Mixed],vec![StrictFeature::Mixed,StrictFeature::Nullability]] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,"R");assert_eq!(returned,FakeValue::Int(1));
        }
    }
}

/// Declared property and method array elements survive a chain, and quiet null receivers remain probes.
#[test]
fn strict_mixed_nullsafe_sources_project_property_and_offset_declarations() {
    let source="class MixedChainContents{public array<int> $items=[1]; \
        public function entries():array<int>{return [2];}public function next():?self{return null;}} \
        $object=new MixedChainContents();echo $object?->items[0]+1,$object?->entries()[0]+1; \
        echo ':',$object?->next()->missing??'Q';echo ':',$absent?->read()??'P';return 1;";
    for dispatch in [false,true] {
        for features in [vec![StrictFeature::Mixed],vec![StrictFeature::Mixed,StrictFeature::Nullability],
            vec![StrictFeature::Mixed,StrictFeature::Initialization,StrictFeature::Indexing]] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,"23:Q:P");assert_eq!(returned,FakeValue::Int(1));
        }
    }
}

/// Reached PHP offset keys require source evidence while skipped keys have no effects.
#[test]
fn strict_mixed_nullsafe_sources_validate_only_reached_offset_key_sources() {
    let source="class MixedChainKeys{public array<int> $items=[1];} \
        function mixed_chain_key():mixed{return 0;}function mixed_chain_skipped_key():int{echo 'E';return 0;} \
        $object=new MixedChainKeys();$index=mixed_chain_key(); \
        try{echo $object?->items[$index]+1;}catch(TypeError $error){echo 'K';} \
        if(is_int($index)){echo $object?->items[$index]+1;} \
        $object=null;$object?->items[mixed_chain_skipped_key()];return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"K2");assert_eq!(returned,FakeValue::Int(1));
    }
}
