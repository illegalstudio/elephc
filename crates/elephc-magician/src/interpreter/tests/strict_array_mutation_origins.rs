//! Purpose:
//! Exercises inferred array contracts across builtin mutation and reference-element writes.
//!
//! Called from:
//! - Focused semantic-profile runtime regressions.
//!
//! Key details:
//! - Failed candidates must preserve storage and retire their temporary owners.
//! - Source producer alternatives remain distinct from actual argument cell kinds.

use super::strict_array_contents::run_source;
use super::support::FakeValue;
use elephc_semantics::StrictFeature;

/// Direct and dynamic mutators cannot replace a closed inferred element contract.
#[test]
fn strict_array_mutations_preserve_closed_origins_and_owners() {
    let source = "$items=[1];try{array_push($items,'bad');}catch(TypeError $error){echo 'P';} \
        try{array_unshift($items,'bad');}catch(TypeError $error){echo 'U';} \
        try{array_splice($items,0,1,['bad']);}catch(TypeError $error){echo 'S';} \
        $push='array_push';try{$push($items,'bad');}catch(TypeError $error){echo 'D';}return $items[0];";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert_eq!(output, "PUSD"); assert_eq!(returned, FakeValue::Int(1));
    }
}

/// A successful first mutating builtin write establishes the same contract as a direct append.
#[test]
fn strict_array_mutations_first_writes_refine_empty_origins() {
    let source = "$items=[];array_push($items,1); \
        try{$items[]='bad';}catch(TypeError $error){echo 'P';} \
        $other=[];array_unshift($other,2); \
        try{$other[]='bad';}catch(TypeError $error){echo 'U';} \
        $splice=[];array_splice($splice,0,0,[3]); \
        try{$splice[]='bad';}catch(TypeError $error){echo 'S';} \
        return count($items)+count($other)+count($splice);";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert_eq!(output, "PUS"); assert_eq!(returned, FakeValue::Int(3));
    }
}

/// Mixed and union source arguments remain open through direct, dynamic and named mutations.
#[test]
fn strict_array_mutations_preserve_explicit_element_sources() {
    let source = "function mutation_mixed():mixed{return [1];} \
        function mutation_union(int|string $value){$items=[];$push='array_push';$push($items,$value); \
        $items[]='allowed';try{$items[]=false;}catch(TypeError $error){echo 'U';}return count($items);} \
        $items=[];array_push($items,mutation_mixed());$items[]='allowed'; \
        $other=[];array_splice(array:$other,offset:0,length:0,replacement:[mutation_mixed()]);$other[]=false; \
        echo mutation_union(1);return count($items)+count($other);";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert_eq!(output, "U2"); assert_eq!(returned, FakeValue::Int(4));
    }
}

/// All source effects run before failed inferred first writes, which leave the original empty array.
#[test]
fn strict_array_mutations_failed_first_writes_preserve_effects_and_storage() {
    let source = "$items=[];try{array_push($items,1,'bad',($effect='E'));}catch(TypeError $error){echo 'P';} \
        echo $effect,count($items);$items[]='valid'; \
        try{$items[]=2;}catch(TypeError $error){echo 'T';}return count($items);";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert_eq!(output, "PE0T"); assert_eq!(returned, FakeValue::Int(1));
    }
}

/// Mutations preserve copies, sparse keys, append history and catchable index exhaustion.
#[test]
fn strict_array_mutations_preserve_keys_history_copies_and_owners() {
    let source = "$items=[0=>1,100=>2];unset($items[100]);$copy=$items;array_push($copy,3); \
        echo count($items),':',$copy[101],':'; \
        array_unshift($copy,0);echo implode(',',array_keys($copy)); \
        $max=[9223372036854775807=>1];try{array_push($max,2);}catch(Error $error){echo 'E';}return count($max);";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert_eq!(output, "1:3:0,1,2E"); assert_eq!(returned, FakeValue::Int(1));
    }
}

/// Shared nested array arguments retain their element kind instead of inheriting neutral mixed metadata.
#[test]
fn strict_array_mutations_nested_alias_arguments_preserve_array_kinds() {
    let source = "$inner=[1];$alias=&$inner;$items=[];array_push($items,$alias); \
        try{$items[]='bad';}catch(TypeError $error){echo 'N';}$items[]=[2]; \
        try{$items[]=['bad'];}catch(TypeError $error){echo 'E';}return count($items);";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert_eq!(output, "NE"); assert_eq!(returned, FakeValue::Int(2));
    }
}

/// Literal argument tuples preserve each entry's producer through positional and named unpacking.
#[test]
fn strict_array_mutations_literal_spreads_preserve_argument_sources() {
    let source = "$items=[];array_push(...[&$items,1]); \
        try{$items[]='bad';}catch(TypeError $error){echo 'P';} \
        $other=[];array_splice(...['array'=>&$other,'offset'=>0,'length'=>0,'replacement'=>[2]]); \
        try{$other[]='bad';}catch(TypeError $error){echo 'S';}return count($items)+count($other);";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert_eq!(output, "PS"); assert_eq!(returned, FakeValue::Int(2));
    }
}

/// Variable spreads retain explicit mixed elements while inferred homogeneous producers stay closed.
#[test]
fn strict_array_mutations_variable_spreads_keep_declared_sources() {
    let source = "function mutation_spread_mixed():array<mixed>{return [1];} \
        $values=mutation_spread_mixed();$items=[];array_push($items,...$values);$items[]='allowed'; \
        $numbers=[1,2];$other=[];array_push($other,...$numbers); \
        try{$other[]='bad';}catch(TypeError $error){echo 'T';}return count($items)+count($other);";
    for dispatch in [false, true] {
        for features in [vec![StrictFeature::ArrayTypes], vec![StrictFeature::ArrayTypes, StrictFeature::ArrayKeys]] {
            let (output, returned) = run_source(source, None, &features, dispatch);
            assert_eq!(output, "T"); assert_eq!(returned, FakeValue::Int(4));
        }
    }
}

/// Legal element reference writes remain visible through fresh copies and integer-key reindexing.
#[test]
fn strict_array_mutations_alias_metadata_survives_copy_and_reindex() {
    let source = "$value=1;$items=[&$value];array_push($items,2);$value=3; \
        $copy=$items;array_unshift($copy,9);$value=4; \
        echo $items[0],':',$copy[1];return count($items)+count($copy);";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert_eq!(output, "4:4"); assert_eq!(returned, FakeValue::Int(5));
    }
}
