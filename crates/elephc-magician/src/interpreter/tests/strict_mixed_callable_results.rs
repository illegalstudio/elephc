//! Purpose:
//! Verifies declared result evidence captured from normalized dynamic callable identities.
//!
//! Called from:
//! - Focused Mixed closure, named and method callback regressions.
//!
//! Key details:
//! - Callee selection precedes argument effects and occurs only once.
//! - Unknown and explicitly mixed returns remain unproven.

use super::strict_array_contents::run_source;
use super::support::FakeValue;
use elephc_semantics::StrictFeature;

/// Dynamic functions, closures and method callbacks retain the selected declaration's return type.
#[test]
fn strict_mixed_callable_results_preserve_resolved_return_declarations() {
    let source="function callable_result_int():int{return 1;} \
        class CallableResultMethods{public function get():int{return 2;}public static function cached():int{return 3;}} \
        $function='callable_result_int';echo $function()+1; \
        $function=callable_result_int(...);echo $function()+1; \
        $closure=function():int{return 4;};echo $closure()+1; \
        $object=new CallableResultMethods();array<mixed> $method=[$object,'get'];echo $method()+1; \
        $static=['CallableResultMethods','cached'];echo $static()+1;return 1;";
    for dispatch in [false,true] {
        for features in [vec![StrictFeature::Mixed],vec![StrictFeature::Mixed,StrictFeature::ArrayTypes]] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,"22534");assert_eq!(returned,FakeValue::Int(1));
        }
    }
}

/// Unknown, mixed and partially supported results cannot acquire proofs from actual scalar returns.
#[test]
fn strict_mixed_callable_results_preserve_unknown_and_partial_alternatives() {
    let source="function callable_unknown(){return 1;}function callable_mixed():mixed{return 1;} \
        function callable_partial():int|stdClass{return 1;} \
        $function='callable_unknown';try{echo $function()+1;}catch(TypeError $error){echo 'U';} \
        $function='callable_mixed';try{echo $function()+1;}catch(TypeError $error){echo 'M';} \
        $function='callable_partial';try{echo $function()+1;}catch(TypeError $error){echo 'P';}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"UMP");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Later argument effects cannot select a different declaration for an already captured callback.
#[test]
fn strict_mixed_callable_results_capture_before_argument_effects() {
    let source="function callable_selected(int $ignored):int{return 1;}function callable_other(int $ignored):mixed{return 1;} \
        function callable_replace(&$callback):int{$callback='callable_other';return 0;} \
        $callback='callable_selected';echo $callback(callable_replace($callback))+1,':',$callback;return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"2:callable_other");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Dynamic invocation requires a callable source view even when its result is unused.
#[test]
fn strict_mixed_callable_results_require_callee_narrowing_at_all_entries() {
    let source="function callable_guard_target():int{return 1;} \
        function callable_guard_origin():mixed{return 'callable_guard_target';}$callback=callable_guard_origin(); \
        try{$callback();}catch(TypeError $error){echo 'B';} \
        try{echo $callback()+1;}catch(TypeError $error){echo 'R';} \
        if(is_callable($callback)){echo $callback()+1;}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"BR2");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Bound methods and invokable objects resolve self and late-static return declarations.
#[test]
fn strict_mixed_callable_results_resolve_nominal_bound_return_scopes() {
    let source="class CallableResultParent{public int $value=4;public static function make():static{return new static();}} \
        class CallableResultChild extends CallableResultParent{} \
        class CallableResultInvoker{public function __invoke():int{return 5;}} \
        $factory=CallableResultChild::make(...);$object=$factory();echo $object->value+1; \
        $invoke=new CallableResultInvoker();echo $invoke()+1;return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"56");assert_eq!(returned,FakeValue::Int(1));
    }
}
