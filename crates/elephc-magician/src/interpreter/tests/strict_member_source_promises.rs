//! Purpose:
//! Verifies original producer roles at mutable instance and static declaration boundaries.
//!
//! Called from:
//! - Focused Mixed and ArrayTypes member-publication regressions.
//!
//! Key details:
//! - Literal inference remains separate from explicit mixed producers and maintained declarations.

use super::strict_array_contents::run_source;
use super::support::FakeValue;
use elephc_semantics::StrictFeature;

/// Runtime first literal writes establish contents independently on instance and static bindings.
#[test]
fn strict_member_sources_infer_runtime_first_array_producers() {
    let source="class RuntimeArrayProducer{public $items;public static $cached;} \
        $object=new RuntimeArrayProducer();$object->items=[1];RuntimeArrayProducer::$cached=[2]; \
        try{$object->items[]='bad';}catch(TypeError $error){echo 'I';} \
        try{RuntimeArrayProducer::$cached[]='bad';}catch(TypeError $error){echo 'S';} \
        return count($object->items)+count(RuntimeArrayProducer::$cached);";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::ArrayTypes],dispatch);
        assert_eq!(output,"IS");assert_eq!(returned,FakeValue::Int(2));
    }
}

/// A declared mixed producer deliberately keeps runtime-established property contents open.
#[test]
fn strict_member_sources_preserve_explicit_mixed_runtime_producers() {
    let source="function openMemberArray():mixed{return [1];} \
        class OpenRuntimeArrayProducer{public $items;public static $cached;} \
        $object=new OpenRuntimeArrayProducer();$object->items=openMemberArray(); \
        OpenRuntimeArrayProducer::$cached=openMemberArray();$object->items[]='allowed'; \
        OpenRuntimeArrayProducer::$cached[]=false;return count($object->items)+count(OpenRuntimeArrayProducer::$cached);";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::ArrayTypes],dispatch);
        assert_eq!(output,"");assert_eq!(returned,FakeValue::Int(4));
    }
}

/// Typed property stores use the original source proof before scalar candidate normalization.
#[test]
fn strict_member_sources_require_mixed_narrowing_before_typed_stores() {
    let source="function opaqueMemberScalar():mixed{return 12;} \
        class NarrowedMemberSource{public int $number=1;public static int $cached=2;} \
        $object=new NarrowedMemberSource();$value=opaqueMemberScalar(); \
        try{$object->number=$value;}catch(TypeError $error){echo 'I';} \
        try{NarrowedMemberSource::$cached=$value;}catch(TypeError $error){echo 'S';} \
        if(is_int($value)){$object->number=$value;NarrowedMemberSource::$cached=$value;} \
        return $object->number+NarrowedMemberSource::$cached;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"IS");assert_eq!(returned,FakeValue::Int(24));
    }
}

/// Pending aliases acquire producer contents only after valid replacement and keep weak nested roots live.
#[test]
fn strict_member_sources_refine_pending_reference_bindings_atomically() {
    let source="class PendingMemberProducer{public $items=[];public static $cached=[];} \
        $object=new PendingMemberProducer();array $items=[];array $cached=[]; \
        $object->items=&$items;PendingMemberProducer::$cached=&$cached; \
        try{$object->items=[1,'bad'];}catch(TypeError $error){echo 'H';} \
        $leaf=1.5;$staticLeaf=2.5;$object->items=[&$leaf];PendingMemberProducer::$cached=[&$staticLeaf]; \
        try{$items[]='bad';}catch(TypeError $error){echo 'I';} \
        try{$cached[]='bad';}catch(TypeError $error){echo 'S';} \
        try{$leaf=1;}catch(TypeError $error){echo 'R';} \
        try{$staticLeaf=2;}catch(TypeError $error){echo 'T';}return count($items)+count($cached);";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::ArrayTypes],dispatch);
        assert_eq!(output,"HISRT");assert_eq!(returned,FakeValue::Int(2));
    }
}

/// Source capture preserves late receiver selection and computed-name effects without reevaluating RHS.
#[test]
fn strict_member_sources_preserve_late_receiver_and_name_effect_order() {
    let source="class LateMemberProducer{public $items;} \
        class LateMemberName{public function __toString():string{echo 'N';return 'items';}} \
        function replaceMemberReceiver(&$object):array<int>{echo 'R';$object=new LateMemberProducer();return [1];} \
        $object=new LateMemberProducer();$before=$object;$name=new LateMemberName(); \
        $object->{$name}=replaceMemberReceiver($object); \
        try{$object->items[]='bad';}catch(TypeError $error){echo 'V';}echo gettype($before->items);return count($object->items);";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::ArrayTypes],dispatch);
        assert_eq!(output,"RNVNULL");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Setter hooks consume their own input declarations and preserve original proofs into backing writes.
#[test]
fn strict_member_sources_validate_selected_setter_hook_inputs() {
    let source="function opaqueHookMember():mixed{return 12;} \
        class HookMemberProducer{public int $number=0{set(mixed $raw){if(is_int($raw)){$this->number=$raw;}}} \
            public string $text=''{set(string $raw){$this->text=$raw;}}} \
        $object=new HookMemberProducer();$object->number=opaqueHookMember();$object->text='x'; \
        try{$object->text=opaqueHookMember();}catch(TypeError $error){echo 'S';} \
        echo $object->number,$object->text;return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"S12x");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Authorization errors precede source-domain errors on readonly and inaccessible physical slots.
#[test]
fn strict_member_sources_preserve_write_authorization_error_order() {
    let source="function opaquePrivateMember():mixed{return 12;} \
        class PrivateMemberProducer{public readonly int $number;private int $secret=1; \
            public function __construct(){$this->number=0;}}$object=new PrivateMemberProducer(); \
        try{$object->number=opaquePrivateMember();}catch(TypeError $error){echo 'bad';}catch(Error $error){echo 'R';} \
        try{$object->secret=opaquePrivateMember();}catch(TypeError $error){echo 'bad';}catch(Error $error){echo 'P';}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"RP");assert_eq!(returned,FakeValue::Int(1));
    }
}
