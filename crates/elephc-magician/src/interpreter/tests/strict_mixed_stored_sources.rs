//! Purpose:
//! Checks logical argument packets retained by stored array bindings and value copies.
//!
//! Called from:
//! - Focused Mixed source transport tests in the interpreter test module.
//!
//! Key details:
//! - Inference schemas cannot prove opaque producers after tuple storage.
//! - Entry snapshots retain their original shared-reference write epochs.

use super::strict_array_contents::run_source;
use super::support::FakeValue;
use elephc_semantics::StrictFeature;

/// Stored and copied callback arrays retain unknown entries and known literal producers.
#[test]
fn strict_mixed_stored_sources_keep_optional_tuple_proofs() {
    let source="function stored_unknown(){return 1;}function stored_target(int $value):int{return $value;} \
        $args=[stored_unknown()];$copy=$args; \
        try{stored_target(...$args);}catch(TypeError $error){echo 'U';} \
        try{call_user_func_array('stored_target',$copy);}catch(TypeError $error){echo 'C';} \
        $reflection=new ReflectionFunction('stored_target'); \
        try{$reflection->invokeArgs($copy);}catch(TypeError $error){echo 'R';} \
        $known=[1];$knownCopy=$known;echo stored_target(...$knownCopy); \
        echo call_user_func_array('stored_target',$knownCopy),$reflection->invokeArgs($knownCopy);return 1;";
    for dispatch in [false,true] {
        for features in [vec![StrictFeature::Mixed],vec![StrictFeature::Mixed,StrictFeature::ArrayTypes]] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,"UCR111");assert_eq!(returned,FakeValue::Int(1));
        }
    }
}

/// Stored reference entries expire after alias writes while detached value entries remain valid.
#[test]
fn strict_mixed_stored_sources_do_not_refresh_reference_snapshots() {
    let source="function stored_origin():mixed{return 1;}function stored_ref_target(int &$value):int{return $value;} \
        function stored_value_target(int $value):int{return $value;} \
        $value=stored_origin();if(is_int($value)){$args=[&$value];$copy=$args;$detached=[$value]; \
        $value='2';try{stored_ref_target(...$copy);}catch(TypeError $error){echo 'R';} \
        echo stored_value_target(...$detached);}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"R1");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Binding updates retire entry evidence, and nested value copies keep optional proof roles.
#[test]
fn strict_mixed_stored_sources_follow_mutations_and_nested_copies() {
    let source="function stored_mutation_unknown(){return 1;}function stored_mutation_target(int $value):int{return $value;} \
        $args=[1];$copy=$args;$args[0]=stored_mutation_unknown(); \
        try{stored_mutation_target(...$args);}catch(TypeError $error){echo 'M';} \
        echo stored_mutation_target(...$copy); \
        $bad=[stored_mutation_unknown()];$tuple=['stored_mutation_target',$bad];$tupleCopy=$tuple; \
        try{call_user_func_array('call_user_func_array',$tupleCopy);}catch(TypeError $error){echo 'N';} \
        $good=[1];$tuple=['stored_mutation_target',$good];$tupleCopy=$tuple; \
        echo call_user_func_array('call_user_func_array',$tupleCopy);return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"M1N1");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Nested entry evidence cannot override an explicit mixed declaration of the container itself.
#[test]
fn strict_mixed_stored_sources_preserve_declared_container_alternatives() {
    let source="function stored_container_target(int $value):int{return $value;} \
        function stored_container_array(array $items):int{return 1;}mixed $args=[1]; \
        try{stored_container_array($args);}catch(TypeError $error){echo 'G';} \
        if(is_array($args)){echo call_user_func_array('stored_container_target',$args);}return 1;";
    for dispatch in [false,true] {
        for features in [vec![StrictFeature::Mixed],vec![StrictFeature::Mixed,StrictFeature::ArrayTypes]] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,"G1");assert_eq!(returned,FakeValue::Int(1));
        }
    }
}
