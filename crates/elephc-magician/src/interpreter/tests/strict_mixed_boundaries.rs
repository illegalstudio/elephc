//! Purpose:
//! Verifies logical Mixed proofs at normalized callable argument and return boundaries.
//!
//! Called from:
//! - Focused direct, callable, reflected and shared-reference source regressions.
//!
//! Key details:
//! - Argument effects precede proof rejection, and defaults retain their declaration context.
//! - Detached value proofs survive later effects; shared proofs require their original validity.

use super::strict_array_contents::run_source;
use super::support::FakeValue;
use elephc_semantics::StrictFeature;

/// All normalized user-call forms require a proof while defaults and guarded arguments remain valid.
#[test]
fn strict_mixed_boundaries_normalized_calls_require_source_proofs() {
    let source = "function boundary_unknown():mixed{return 1;} \
        function boundary_int(int $value):int{return $value;} \
        function boundary_default(int $value=5):int{return $value;} \
        $value=boundary_unknown(); \
        try{boundary_int($value);}catch(TypeError $error){echo 'D';} \
        try{boundary_int(value:$value);}catch(TypeError $error){echo 'N';} \
        $call=boundary_int(...);try{$call($value);}catch(TypeError $error){echo 'F';} \
        try{call_user_func('boundary_int',$value);}catch(TypeError $error){echo 'C';} \
        if(is_int($value)){echo boundary_int($value);}echo boundary_default();return 1;";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::Mixed], dispatch);
        assert_eq!(output, "DNFC15"); assert_eq!(returned, FakeValue::Int(1));
    }
}

/// Reflection forwards logical argument proofs instead of deriving them from its value container.
#[test]
fn strict_mixed_boundaries_reflection_preserves_argument_proofs() {
    let source = "function reflected_unknown():mixed{return 1;} \
        function reflected_int(int $value):int{return $value;} \
        $reflection=new ReflectionFunction('reflected_int');$value=reflected_unknown(); \
        try{$reflection->invoke($value);}catch(TypeError $error){echo 'R';} \
        if(is_int($value)){echo $reflection->invoke($value);}return 1;";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::Mixed], dispatch);
        assert_eq!(output, "R1"); assert_eq!(returned, FakeValue::Int(1));
    }
}

/// A return proof fails at its source statement so the function's own handler can catch it.
#[test]
fn strict_mixed_boundaries_return_proofs_use_the_return_statement() {
    let source = "function returned_int(mixed $value):int{ \
        try{return $value;}catch(TypeError $error){echo 'T';return 1;}} \
        function guarded_returned_int(mixed $value):int{if(is_int($value)){return $value;}return 0;} \
        echo returned_int(1);return guarded_returned_int(1);";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::Mixed], dispatch);
        assert_eq!(output, "T1"); assert_eq!(returned, FakeValue::Int(1));
    }
}

/// All argument effects complete before rejection, including effects after a stale reference proof.
#[test]
fn strict_mixed_boundaries_keep_argument_effects_and_reference_validity() {
    let source = "function effect_unknown():mixed{return 1;} \
        function effect_int(int $value,$later):int{return $value;} \
        function effect_ref(int &$value,$later):int{return $value;} \
        function effect_replace(&$value):bool{$value='2';return true;} \
        $value=effect_unknown();try{effect_int($value,($side='E'));}catch(TypeError $error){echo 'T';} \
        echo $side;$value=effect_unknown();if(is_int($value)){ \
        echo effect_int($value,effect_replace($value)),':',gettype($value),':';} \
        $value=effect_unknown();if(is_int($value)){ \
        try{effect_ref($value,effect_replace($value));}catch(TypeError $error){echo 'R';} \
        echo ':',gettype($value);}return 1;";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::Mixed], dispatch);
        assert_eq!(output, "TE1:string:R:string"); assert_eq!(returned, FakeValue::Int(1));
    }
}

/// Nested untyped functions and closures shadow their caller while finally observes return errors.
#[test]
fn strict_mixed_boundaries_return_scopes_shadow_and_restore() {
    let source = "function scoped_unknown():mixed{return 1;} \
        function scoped_untyped($value){return $value;} \
        function scoped_int(mixed $value):int{scoped_untyped($value); \
        $call=function($item){return $item;};$call($value); \
        try{return $value;}catch(TypeError $error){echo 'T';return 1;}} \
        function scoped_finally(mixed $value):int{try{return $value;}finally{echo 'F';return 2;}} \
        $closure=function(mixed $value):int{try{return $value;}catch(TypeError $error){echo 'C';return 3;}}; \
        echo scoped_int(scoped_unknown()),':',scoped_finally(scoped_unknown()),':',$closure(scoped_unknown());return 1;";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::Mixed], dispatch);
        assert_eq!(output, "T1:F2:C3"); assert_eq!(returned, FakeValue::Int(1));
    }
}

/// Instance and static methods validate the reached return before their own exception handlers exit.
#[test]
fn strict_mixed_boundaries_method_return_proofs() {
    let source = "class ReturnBoundary { \
        public function instance(mixed $value):int{try{return $value;}catch(TypeError $error){echo 'I';return 1;}} \
        public static function selected(mixed $value):int{try{return $value;}catch(TypeError $error){echo 'S';return 2;}}} \
        $object=new ReturnBoundary();echo $object->instance(1),':',ReturnBoundary::selected(1);return 1;";
    for dispatch in [false, true] {
        let (output, returned) = run_source(source, None, &[StrictFeature::Mixed], dispatch);
        assert_eq!(output, "I1:S2"); assert_eq!(returned, FakeValue::Int(1));
    }
}
