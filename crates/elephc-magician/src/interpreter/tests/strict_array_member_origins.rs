//! Purpose:
//! Verifies inferred array contents owned by instance and static property bindings.
//!
//! Called from:
//! - Focused ArrayTypes source-origin and alias regressions.
//!
//! Key details:
//! - Untyped defaults infer contents; explicit bare array declarations remain open.
//! - Candidate rejection preserves the existing value and balances every runtime owner.

use super::strict_array_contents::run_source;
use super::support::FakeValue;
use elephc_semantics::StrictFeature;

/// Untyped property defaults retain their first element contract across instance and static writes.
#[test]
fn strict_array_member_origins_infer_instance_and_static_default_contents() {
    let source = "class ArrayMemberSources{public $items=[1];public static $cached=[1];} \
        $object=new ArrayMemberSources(); \
        try{$object->items[]='bad';}catch(TypeError $error){echo 'P';} \
        try{ArrayMemberSources::$cached[]='bad';}catch(TypeError $error){echo 'S';} \
        return count($object->items)*10+count(ArrayMemberSources::$cached);";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert_eq!(output, "PS"); assert_eq!(returned, FakeValue::Int(11));
    }
}

/// Declared open arrays preserve their documented element alternatives at property boundaries.
#[test]
fn strict_array_member_origins_preserve_declared_open_array_properties() {
    let source = "class OpenArrayMemberSources{public array $items=[1];public static array $cached=[1];} \
        $object=new OpenArrayMemberSources();$object->items[]='allowed'; \
        OpenArrayMemberSources::$cached[]=false; \
        return count($object->items)+count(OpenArrayMemberSources::$cached);";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::ArrayTypes], dispatch);
        assert!(output.is_empty()); assert_eq!(returned, FakeValue::Int(4));
    }
}

/// Whole replacements and rebound references keep only the live physical property obligations.
#[test]
fn strict_array_member_origins_preserve_replacements_and_reference_lifetimes() {
    let source="class ArrayBindingOrigins{public $items=[1];public static $cached=[2];} \
        $object=new ArrayBindingOrigins();try{$object->items=['bad'];}catch(TypeError $error){echo 'W';} \
        $object->items=7;$object->items=[3];array $items=[4];$object->items=&$items; \
        try{$items[]='bad';}catch(TypeError $error){echo 'I';} \
        unset($object->items);$items[]='allowed';array $old=[5];array $next=[6]; \
        ArrayBindingOrigins::$cached=&$old;ArrayBindingOrigins::$cached=&$next;$old[]='allowed'; \
        try{$next[]='bad';}catch(TypeError $error){echo 'S';} \
        try{ArrayBindingOrigins::$cached=['bad'];}catch(TypeError $error){echo 'R';} \
        return count($items)+count($old)+count($next);";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::ArrayTypes],dispatch);
        assert_eq!(output,"WISR");assert_eq!(returned,FakeValue::Int(5));
    }
}

/// Cloning and parent-private shadowing keep distinct physical binding origins.
#[test]
fn strict_array_member_origins_clone_private_bindings_and_explicit_mixed_defaults() {
    let source="class ArrayOriginParent{private $items=[1]; \
        public function append($value):void{$this->items[]=$value;} \
        public function size():int{return count($this->items);}} \
        class ArrayOriginChild extends ArrayOriginParent{public mixed $items=['a',true];} \
        $original=new ArrayOriginChild();$copy=clone $original; \
        try{$copy->append('bad');}catch(TypeError $error){echo 'P';} \
        $copy->append(2);$copy->items[]=3;return $original->size()+$copy->size()+count($copy->items);";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::ArrayTypes],dispatch);
        assert_eq!(output,"P");assert_eq!(returned,FakeValue::Int(6));
    }
}

/// References nested in live property values cannot invalidate a binding's inferred contents.
#[test]
fn strict_array_member_origins_constrain_reference_elements_until_binding_retirement() {
    let source="class ElementArrayOrigins{public $items=[1];public static $cached=[2];} \
        $object=new ElementArrayOrigins();$instance=1;$cached=2; \
        $object->items=[&$instance];ElementArrayOrigins::$cached=[&$cached];$copy=clone $object; \
        try{$instance='bad';}catch(TypeError $error){echo 'I';} \
        try{$cached='bad';}catch(TypeError $error){echo 'S';} \
        unset($object->items);try{$instance='bad';}catch(TypeError $error){echo 'C';} \
        unset($copy->items);$instance='released';ElementArrayOrigins::$cached=[3];$cached='released'; \
        return $instance.':'.$cached;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::ArrayTypes],dispatch);
        assert_eq!(output,"ISC");assert_eq!(returned,FakeValue::String("released:released".into()));
    }
}
