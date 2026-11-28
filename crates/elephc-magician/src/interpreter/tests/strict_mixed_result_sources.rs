//! Purpose:
//! Verifies logical result alternatives for arithmetic and lazy conditional expressions.
//!
//! Called from:
//! - Focused Mixed result metadata and independently selected profile tests.
//!
//! Key details:
//! - Successful expressions retain source-domain kinds instead of reading their result tag.
//! - Selected lazy branches evaluate once and keep their own producer evidence.

use super::strict_array_contents::run_source;
use super::support::FakeValue;
use elephc_semantics::StrictFeature;

/// Float arithmetic and unary signatures keep a float proof instead of an invented integer arm.
#[test]
fn strict_mixed_result_sources_keep_float_operator_domains() {
    let source="function result_float(float $value):float{return $value;} \
        echo result_float(1.5+0.5),':',result_float(-1.5),':',result_float(+1.5);return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed,StrictFeature::Coercions],dispatch);
        assert_eq!(output,"2:-1.5:1.5");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Checked integer operations retain their guaranteed integer kind through strict calls and returns.
#[test]
fn strict_mixed_result_sources_keep_checked_integer_domains() {
    let source="function result_integer(int $value):int{return $value;} \
        echo result_integer(1+1),':',result_integer(3*2),':',result_integer(-2),':',result_integer(+2),':',result_integer(2**3);return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed,StrictFeature::Coercions,
            StrictFeature::Overflow,StrictFeature::Arithmetic],dispatch);
        assert_eq!(output,"2:6:-2:2:8");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Elvis keeps only the reached branch source, and logical short circuits preserve effects and guards.
#[test]
fn strict_mixed_result_sources_follow_lazy_branch_selection() {
    let source="function result_lazy_int(int $value):int{return $value;} \
        function result_lazy_mixed():mixed{return 2;}$value=null;echo result_lazy_int($value?:1),':'; \
        $value=result_lazy_mixed();$guard=is_int($value)&&$value>0; \
        if($guard){echo 'G';}$side=0;$unused=true||($side=1);echo $side,':'; \
        $side=0;echo result_lazy_int((false?:($side=3))),':',$side;return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed,StrictFeature::Coercions],dispatch);
        assert_eq!(output,"1:G0:3:3");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// Selected boolean signatures check every source alternative at lazy and unary consumers.
#[test]
fn strict_mixed_result_sources_boolean_signatures_require_narrowing() {
    let source="function result_bool_origin():mixed{return true;}$value=result_bool_origin(); \
        try{if($value){echo 'bad';}}catch(TypeError $error){echo 'I';} \
        try{$unused=!$value;}catch(TypeError $error){echo 'N';} \
        try{$unused=true&&$value;}catch(TypeError $error){echo 'A';} \
        try{$unused=true xor $value;}catch(TypeError $error){echo 'X';} \
        try{$unused=$value?:false;}catch(TypeError $error){echo 'E';} \
        try{$value?:false;}catch(TypeError $error){echo 'T';} \
        if(is_bool($value)){if($value){echo 'G';}}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed,StrictFeature::Bools],dispatch);
        assert_eq!(output,"INAXETG");assert_eq!(returned,FakeValue::Int(1));
    }
}
