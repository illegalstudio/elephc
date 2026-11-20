//! Purpose:
//! Verifies declaration-backed results across callback forwarding wrappers.
//!
//! Called from:
//! - Focused Mixed callback source and owner regressions.
//!
//! Key details:
//! - Unknown declarations stay unknown and nested wrappers retain actual target metadata.

use super::strict_array_contents::run_source;
use super::support::FakeValue;
use elephc_semantics::StrictFeature;

/// Direct, namespaced, dynamic and nested wrappers preserve the actual callback return declaration.
#[test]
fn strict_mixed_forwarded_results_preserve_selected_declarations() {
    let source="namespace Forwarded;function target():int{return 2;} \
        echo call_user_func('Forwarded\\target')+1; \
        echo call_user_func_array('Forwarded\\target',[])+1; \
        $wrapper='call_user_func';echo $wrapper('Forwarded\\target')+1; \
        echo call_user_func('call_user_func_array','Forwarded\\target',[])+1; \
        $result=call_user_func('Forwarded\\target');echo $result+1;return 1;";
    for dispatch in [false,true] {
        for features in [vec![StrictFeature::Mixed],vec![StrictFeature::Mixed,StrictFeature::ArrayTypes]] {
            let (output,returned)=run_source(source,None,&features,dispatch);
            assert_eq!(output,"33333");assert_eq!(returned,FakeValue::Int(1));
        }
    }
}

/// Forwarding never turns actual integer occupants into a proof for unknown or partial signatures.
#[test]
fn strict_mixed_forwarded_results_preserve_unknown_alternatives() {
    let source="function forwarded_unknown(){return 1;}function forwarded_mixed():mixed{return 1;} \
        function forwarded_partial():int|stdClass{return 1;} \
        try{echo call_user_func('forwarded_unknown')+1;}catch(TypeError $error){echo 'U';} \
        try{echo call_user_func_array('forwarded_mixed',[])+1;}catch(TypeError $error){echo 'M';} \
        try{echo call_user_func('call_user_func','forwarded_partial')+1;}catch(TypeError $error){echo 'P';}return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"UMP");assert_eq!(returned,FakeValue::Int(1));
    }
}

/// The callback value is captured before later argument effects while its declaration follows that value.
#[test]
fn strict_mixed_forwarded_results_preserve_argument_order_and_callback_identity() {
    let source="function forwarded_selected(int $ignored):int{return 2;} \
        function forwarded_other(int $ignored):mixed{return 2;} \
        function forwarded_replace(&$callback):int{$callback='forwarded_other';echo 'E';return 0;} \
        $callback='forwarded_selected';echo call_user_func($callback,forwarded_replace($callback))+1; \
        echo ':',$callback;return 1;";
    for dispatch in [false,true] {
        let (output,returned)=run_source(source,None,&[StrictFeature::Mixed],dispatch);
        assert_eq!(output,"E3:forwarded_other");assert_eq!(returned,FakeValue::Int(1));
    }
}
