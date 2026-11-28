//! Purpose:
//! Verifies first informative writes into nested inferred empty arrays.
//!
//! Called from:
//! - Focused ArrayTypes pending-content refinement regressions.
//!
//! Key details:
//! - Failed candidates must leave both the value and pending schema intact.

use super::strict_array_contents::run_source;
use super::support::FakeValue;
use elephc_semantics::StrictFeature;

/// Nested inferred empty arrays acquire content evidence from the first successful leaf write.
#[test]
fn strict_array_origins_nested_empty_arrays_refine_first_leaf_writes() {
    let source = "$items=[[]];$items[0][]=1; \
        try{$items[0][]='bad';}catch(TypeError $error){echo 'N';}return $items[0][0];";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert_eq!(output, "N"); assert_eq!(returned, FakeValue::Int(1));
    }
}

/// Nested member origins and refused first candidates refine only after a successful typed write.
#[test]
fn strict_array_origins_nested_member_writes_and_failed_candidates() {
    let source = "class NestedPendingMember{public $items=[[]];public static $cached=[[]];} \
        class RetryPendingMember{public $items=[];} $object=new NestedPendingMember(); \
        $object->items[0][]=1;try{$object->items[0][]='bad';}catch(TypeError $error){echo 'I';} \
        NestedPendingMember::$cached[0][]=2; \
        try{NestedPendingMember::$cached[0][]='bad';}catch(TypeError $error){echo 'S';} \
        $retry=new RetryPendingMember();try{$retry->items[]=[1,'bad'];}catch(TypeError $error){echo 'H';} \
        $retry->items[]=[2];try{$retry->items[]=['bad'];}catch(TypeError $error){echo 'P';} \
        return $object->items[0][0]+NestedPendingMember::$cached[0][0]+$retry->items[0][0];";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert_eq!(output, "ISHP"); assert_eq!(returned, FakeValue::Int(5));
    }
}
