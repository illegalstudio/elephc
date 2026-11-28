//! Purpose:
//! Verifies selected method and reflected target result declarations under Mixed.
//!
//! Called from:
//! - Focused receiver, selector and return-source regressions.
//!
//! Key details:
//! - Runtime target selection never infers a return declaration from the result tag.

use super::strict_array_contents::run_source;
use super::support::FakeValue;
use elephc_semantics::StrictFeature;

/// Runtime method and class selectors preserve the selected immutable return declaration.
#[test]
fn strict_mixed_method_results_preserve_dynamic_selections() {
    let source="class MixedSelectedMethods{public function read():int{return 2;} \
        public static function cached():int{return 3;}public function unknown(){return 2;}} \
        $object=new MixedSelectedMethods();$method='read';echo $object->$method()+1; \
        $class='MixedSelectedMethods';$method='cached';echo $class::$method()+1; \
        $method='unknown';try{echo $object->$method()+1;}catch(TypeError $error){echo 'U';}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"34U");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Receiver and member-name source proofs are required even when the result is unused.
#[test]
fn strict_mixed_method_results_require_receiver_and_selector_narrowing() {
    let source="class MixedGuardedMethods{public function read():int{return 2;}} \
        function mixed_method_receiver():mixed{return new MixedGuardedMethods();} \
        function mixed_method_selector():mixed{return 'read';} \
        $object=mixed_method_receiver();try{$object->read();}catch(TypeError $error){echo 'R';} \
        if(is_object($object)){echo $object->read()+1;} \
        $object=new MixedGuardedMethods();$method=mixed_method_selector(); \
        try{$object->$method();}catch(TypeError $error){echo 'S';} \
        if(is_string($method)){echo $object->$method()+1;}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"R3S3");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Reflection invocation obtains the selected target declaration and preserves unknown alternatives.
#[test]
fn strict_mixed_method_results_preserve_reflected_targets() {
    let source="function reflected_result_int():int{return 2;}function reflected_result_unknown(){return 2;} \
        class MixedReflectedMethods{public function read():int{return 3;}public static function cached():int{return 4;}} \
        $reflection=new ReflectionFunction('reflected_result_int');echo $reflection->invoke()+1; \
        echo $reflection->invokeArgs([])+1;echo call_user_func([$reflection,'invoke'])+1;$object=new MixedReflectedMethods(); \
        $reflection=new ReflectionMethod('MixedReflectedMethods','read');echo $reflection->invoke($object)+1; \
        echo $reflection->invokeArgs($object,[])+1; \
        $reflection=new ReflectionMethod('MixedReflectedMethods','cached');echo $reflection->invoke(null)+1; \
        $reflection=new ReflectionFunction('reflected_result_unknown'); \
        try{echo $reflection->invoke()+1;}catch(TypeError $error){echo 'U';}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"333445U");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Dynamic class selectors require narrowing and target selection precedes later argument effects.
#[test]
fn strict_mixed_method_results_preserve_class_guards_and_effect_order() {
    let source="class MixedMethodOrder{public static function cached(int $ignored):int{return 4;} \
        public static function other(int $ignored):mixed{return 4;}} \
        function mixed_method_class():mixed{return 'MixedMethodOrder';} \
        function mixed_method_replace(&$method):int{$method='other';echo 'E';return 0;} \
        $class=mixed_method_class();$method='cached'; \
        try{$class::$method(0);}catch(TypeError $error){echo 'C';} \
        if(is_string($class)){echo $class::$method(mixed_method_replace($method))+1;}echo ':',$method;return 1;";
    for dispatch in [false,true] {
        for features in [vec![StrictFeature::Mixed],vec![StrictFeature::Mixed,StrictFeature::ArrayTypes]] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,"CE5:other");assert_eq!(returned,FakeValue::Int(1));
        }
    }
}
