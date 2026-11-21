//! Purpose:
//! Verifies maintained mutable property declarations used as logical read proofs.
//!
//! Called from:
//! - Focused Mixed/ArrayTypes property write and reference regressions.
//!
//! Key details:
//! - Weak scalar conversion is independent from exact Coercions and explicit Casts.

use super::strict_array_contents::run_source;
use super::support::FakeValue;
use elephc_semantics::StrictFeature;

/// Declared scalar properties normalize weak inputs and reject incompatible replacements before publication.
#[test]
fn strict_mixed_member_promises_preserve_scalar_instance_and_static_declarations() {
    let source="class MixedMemberPromise{public int $number=1;public static int $cached=1;} \
        $object=new MixedMemberPromise();$object->number='12';MixedMemberPromise::$cached='13'; \
        echo gettype($object->number),':',gettype(MixedMemberPromise::$cached); \
        try{$object->number='bad';}catch(TypeError $error){echo ':I';} \
        try{MixedMemberPromise::$cached=[];}catch(TypeError $error){echo ':S';} \
        echo ':',$object->number,':',MixedMemberPromise::$cached;return 1;";
    for dispatch in [false,true] {
        for features in [vec![StrictFeature::Mixed],vec![StrictFeature::Mixed,StrictFeature::Casts]] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,"integer:integer:I:S:12:13");assert_eq!(returned,FakeValue::Int(1));
        }
    }
}

/// Explicit structural declarations stay valid under independently selected ArrayTypes and Mixed.
#[test]
fn strict_mixed_member_promises_preserve_declared_structural_properties() {
    let source="class StructuralMemberPromise{public array<int> $items=[1];public static array<int> $cached=[2];} \
        $object=new StructuralMemberPromise();try{$object->items=['bad'];}catch(TypeError $error){echo 'I';} \
        try{StructuralMemberPromise::$cached=['bad'];}catch(TypeError $error){echo 'S';} \
        echo $object->items[0],StructuralMemberPromise::$cached[0];return 1;";
    for dispatch in [false,true] {
        for features in [vec![StrictFeature::Mixed],vec![StrictFeature::ArrayTypes],
            vec![StrictFeature::Mixed,StrictFeature::ArrayTypes]] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,"IS12");assert_eq!(returned,FakeValue::Int(1));
        }
    }
}

/// Persistent property aliases keep their scalar promise on later canonical source writes.
#[test]
fn strict_mixed_member_promises_preserve_alias_write_contracts() {
    let source="class AliasMemberPromise{public int $number=1;public static int $cached=1;} \
        $object=new AliasMemberPromise();$instance=2;$static=3; \
        $object->number=&$instance;AliasMemberPromise::$cached=&$static; \
        $instance='12';$static='13';echo gettype($instance),':',gettype($static); \
        try{$instance='bad';}catch(TypeError $error){echo ':I';} \
        try{$static=[];}catch(TypeError $error){echo ':S';} \
        echo ':',$object->number,':',AliasMemberPromise::$cached;return 1;";
    for dispatch in [false,true] {
        for features in [vec![StrictFeature::Mixed],vec![StrictFeature::ArrayTypes],
            vec![StrictFeature::Mixed,StrictFeature::ArrayTypes]] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,"integer:integer:I:S:12:13");assert_eq!(returned,FakeValue::Int(1));
        }
    }
}

/// Initial weak aliases normalize their canonical sources while incompatible sources leave the old property intact.
#[test]
fn strict_mixed_member_promises_validate_initial_reference_candidates() {
    let source="class InitialMemberPromise{public int $number=1;public static int $cached=1;} \
        $object=new InitialMemberPromise();$instance='12';$static='13'; \
        $object->number=&$instance;InitialMemberPromise::$cached=&$static; \
        echo gettype($instance),':',gettype($static),':',$object->number+1,':',InitialMemberPromise::$cached+1; \
        $bad='bad';try{$object->number=&$bad;}catch(TypeError $error){echo ':I';} \
        try{InitialMemberPromise::$cached=&$bad;}catch(TypeError $error){echo ':S';}return 1;";
    for dispatch in [false,true] {
        for features in [vec![StrictFeature::Mixed],vec![StrictFeature::ArrayTypes]] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,"integer:integer:13:14:I:S");assert_eq!(returned,FakeValue::Int(1));
        }
    }
}

/// Prospective declarations apply only to the outer bind, leaving reentrant writes on unbound storage valid.
#[test]
fn strict_mixed_member_promises_scope_initial_reference_admission_across_callbacks() {
    let source="class AdmissionMemberPromise{public string $text='old';} \
        class AdmissionStringSource{public mixed $mutator;public function __construct($mutator){$this->mutator=$mutator;} \
            public function __toString():string{$callback=$this->mutator;if(is_callable($callback)){$callback();}echo 'C';return '12';}} \
        $source=null;$mutator=function()use(&$source):void{$source=[];}; \
        $source=new AdmissionStringSource($mutator);$object=new AdmissionMemberPromise(); \
        $object->text=&$source;echo gettype($source),':',$object->text;return 1;";
    for dispatch in [false,true] {
        for features in [vec![StrictFeature::Mixed],vec![StrictFeature::Mixed,StrictFeature::Casts]] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,"Cstring:12");assert_eq!(returned,FakeValue::Int(1));
        }
    }
}
