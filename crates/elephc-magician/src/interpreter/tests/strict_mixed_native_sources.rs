//! Purpose:
//! Verifies logical source checks before native function and method argument coercion.
//!
//! Called from:
//! - Focused native binder, variadic, default and reference regression tests.
//!
//! Key details:
//! - Native invocation is unnecessary to exercise normalization and owned binding cleanup.
//! - A concrete integer occupant does not prove a declared mixed source.

use super::super::*;
use super::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Creates a borrowed source operand with independently captured logical alternatives.
fn native_source_argument(value: RuntimeCellHandle, name: Option<&str>,
    source: &str, context: &ElephcEvalContext,
) -> EvaluatedCallArg {
    EvaluatedCallArg { value: value.borrowed(), evaluation_owner: None,
        name: name.map(str::to_string), ref_target: None,
        source_type: Some(ArgumentSource::capture(crate::eval_ir::parameter_type(
            &crate::parser::generics::parse_type_string(source).unwrap()),context)) }
}

/// Registers a native integer function signature without calling its descriptor.
fn native_source_function(variadic: bool) -> NativeFunction {
    let mut function=NativeFunction::new(std::ptr::null_mut(),fake_native_return_descriptor,1);
    assert!(function.set_param_name(0,"value"));
    assert!(function.set_param_type(0,EvalParameterType::new(vec![EvalParameterTypeVariant::Int],false)));
    if variadic { assert!(function.set_variadic_index(0)); }
    function
}

/// Retires a rejected TypeError and verifies no native binding acquired an extra value owner.
fn finish_native_source_case(context: &mut ElephcEvalContext, values: &mut FakeOps,
    value: RuntimeCellHandle, rejected: bool,
) {
    if rejected {
        let error=context.take_pending_throw().expect("native source TypeError");
        assert!(values.object_is_a(error,"TypeError",false).unwrap());values.release(error).unwrap();
    } else { assert!(context.take_pending_throw().is_none()); }
    values.release(value).unwrap();
    for value in context.take_persistent_cell_owners(){values.release(value).unwrap();}
    assert_eq!(values.cell_owners.values().sum::<usize>(),0);
}

/// Named, positional and variadic function slots validate their original source before conversion.
#[test]
fn strict_mixed_native_sources_function_binders_require_logical_inputs() {
    for variadic in [false,true] { for name in [None,Some("value"),Some("tail")] {
        if !variadic && name==Some("tail"){continue;}
        for source in ["mixed","int"] {
            let mut context=ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Mixed,true)).unwrap();
            let mut values=FakeOps::default();let value=values.int(1).unwrap();
            let function=native_source_function(variadic);
            let argument=native_source_argument(value,name,source,&context);
            let result=bind_evaluated_native_function_args(&function,vec![argument],&mut context,&mut values);
            let rejected=source=="mixed";
            if rejected { assert!(matches!(result,Err(EvalStatus::UncaughtThrowable)),"variadic={variadic}, name={name:?}"); }
            else { for value in result.unwrap().values {values.release(value).unwrap();} }
            finish_native_source_case(&mut context,&mut values,value,rejected);
        }
    }}
}

/// Native method collectors preserve each entry proof instead of validating their occupant alone.
#[test]
fn strict_mixed_native_sources_method_binders_require_logical_inputs() {
    for variadic in [false,true] { for name in [None,Some("value"),Some("tail")] {
        if !variadic && name==Some("tail"){continue;}
        for source in ["mixed","int"] {
            let mut context=ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Mixed,true)).unwrap();
            let mut values=FakeOps::default();let value=values.int(1).unwrap();
            let mut signature=NativeCallableSignature::new(1);
            assert!(signature.set_param_name(0,"value"));
            assert!(signature.set_param_type(0,EvalParameterType::new(vec![EvalParameterTypeVariant::Int],false)));
            if variadic {assert!(signature.set_variadic_index(0));}
            let argument=native_source_argument(value,name,source,&context);
            let result=bind_native_callable_bound_args_with_mode(Some(signature),vec![argument],None,
                EvalByRefBindingMode::RequireTarget,&mut context,&mut values);
            let rejected=source=="mixed";
            if rejected {assert!(matches!(result,Err(EvalStatus::UncaughtThrowable)),"variadic={variadic}, name={name:?}");}
            else {release_native_bound_args(&result.unwrap(),&mut context,&mut values).unwrap();}
            finish_native_source_case(&mut context,&mut values,value,rejected);
        }
    }}
}

/// Default metadata supplies its formal source without requiring a fabricated caller operand.
#[test]
fn strict_mixed_native_sources_defaults_retain_formal_contracts() {
    for features in [vec![StrictFeature::Mixed],vec![StrictFeature::Mixed,StrictFeature::Coercions]] {
        let profile=features.iter().fold(SemanticProfile::PERMISSIVE,|profile,feature|profile.with(*feature,true));
        let mut context=ElephcEvalContext::new();context.install_semantic_profile(profile).unwrap();
        let mut values=FakeOps::default();
        let mut function=NativeFunction::new(std::ptr::null_mut(),fake_native_return_descriptor,2);
        for (index,name) in ["first","second"].iter().enumerate() {
            assert!(function.set_param_name(index,*name));
            assert!(function.set_param_type(index,EvalParameterType::new(vec![EvalParameterTypeVariant::Int],false)));
            assert!(function.set_param_default(index,NativeCallableDefault::Int(7)));
        }
        let operand=values.int(1).unwrap();
        let argument=native_source_argument(operand,Some("second"),"int",&context);
        let bound=bind_evaluated_native_function_args(&function,vec![argument],&mut context,&mut values).unwrap();
        assert_eq!(values.get(bound.values[0]),FakeValue::Int(7));
        for value in bound.values {values.release(value).unwrap();}
        values.release(operand).unwrap();
        let mut signature=NativeCallableSignature::new(1);
        assert!(signature.set_param_name(0,"value"));
        assert!(signature.set_param_type(0,EvalParameterType::new(vec![EvalParameterTypeVariant::Int],false)));
        assert!(signature.set_param_default(0,NativeCallableDefault::Int(7)));
        let bound=bind_native_callable_bound_args_with_mode(Some(signature),Vec::new(),None,
            EvalByRefBindingMode::RequireTarget,&mut context,&mut values).unwrap();
        assert_eq!(values.get(bound[0].value),FakeValue::Int(7));
        release_native_bound_args(&bound,&mut context,&mut values).unwrap();
        assert_eq!(values.cell_owners.values().sum::<usize>(),0);
    }
}

/// Payload materialization cannot hide a stale canonical reference source from a native binder.
#[test]
fn strict_mixed_native_sources_check_canonical_reference_stamps() {
    for method in [false,true] {
        let mut context=ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Mixed,true)).unwrap();
        let mut values=FakeOps::default();let value=values.int(1).unwrap();
        let reference=values.reference_new(value).unwrap();
        let mut argument=native_source_argument(value,None,"int",&context);
        argument.ref_target=Some(EvalReferenceTarget::Cell{cell:reference.borrowed()});
        context.invalidate_shared_source_views(reference);
        if method {
            let mut signature=NativeCallableSignature::new(1);
            assert!(signature.set_param_name(0,"value"));
            assert!(signature.set_param_type(0,EvalParameterType::new(vec![EvalParameterTypeVariant::Int],false)));
            assert!(signature.set_param_by_ref(0,true));
            let result=bind_native_callable_bound_args_with_mode(Some(signature),vec![argument],None,
                EvalByRefBindingMode::RequireTarget,&mut context,&mut values);
            assert!(matches!(result,Err(EvalStatus::UncaughtThrowable)));
        } else {
            let mut function=native_source_function(false);assert!(function.set_param_by_ref(0,true));
            let result=bind_evaluated_native_function_args(&function,vec![argument],&mut context,&mut values);
            assert!(matches!(result,Err(EvalStatus::UncaughtThrowable)));
        }
        values.release(reference).unwrap();
        finish_native_source_case(&mut context,&mut values,value,true);
    }
}

/// Coercions selects exact source alternatives before a native scalar binder observes the occupant.
#[test]
fn strict_mixed_native_sources_compose_with_noncoercive_signatures() {
    let mut context=ElephcEvalContext::new();
    context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Mixed,true)
        .with(StrictFeature::Coercions,true)).unwrap();
    let mut values=FakeOps::default();let value=values.int(1).unwrap();
    let function=native_source_function(false);
    let argument=native_source_argument(value,None,"int|string",&context);
    let result=bind_evaluated_native_function_args(&function,vec![argument],&mut context,&mut values);
    assert!(matches!(result,Err(EvalStatus::UncaughtThrowable)));
    finish_native_source_case(&mut context,&mut values,value,true);
}

/// Real source calls preserve proofs through native static, named and callback dispatch.
#[test]
fn strict_mixed_native_sources_runtime_dispatch_preserves_input_proofs() {
    let source="function native_dispatch_origin():mixed{return 1;}$value=native_dispatch_origin(); \
        try{KnownClass::sum($value,4);}catch(TypeError $error){echo 'D';} \
        try{KnownClass::sum(right:4,left:$value);}catch(TypeError $error){echo 'N';} \
        $call=KnownClass::sum(...);try{$call($value,4);}catch(TypeError $error){echo 'F';} \
        try{call_user_func('KnownClass::sum',$value,4);}catch(TypeError $error){echo 'C';} \
        try{call_user_func_array('KnownClass::sum',[$value,4]);}catch(TypeError $error){echo 'A';} \
        if(is_int($value)){echo KnownClass::sum($value,4);}return 1;";
    for dispatch in [false,true] {
        let profile=SemanticProfile::PERMISSIVE.with(StrictFeature::Mixed,true);
        let mut context=ElephcEvalContext::new();context.install_semantic_profile(profile).unwrap();
        let mut signature=NativeCallableSignature::new(2);
        for (index,name) in ["left","right"].iter().enumerate() {
            assert!(signature.set_param_name(index,*name));
            assert!(signature.set_param_type(index,EvalParameterType::new(vec![EvalParameterTypeVariant::Int],false)));
        }
        assert!(context.define_native_static_method_signature("KnownClass","sum",signature));
        let mut scope=ElephcEvalScope::new();let mut values=FakeOps::default();
        let program=crate::parser::parse_fragment_with_semantics(source.as_bytes(),profile).unwrap();
        let result=if dispatch {
            bytecode::try_execute(&mut context,&program,&mut scope,&mut values).and_then(|result|
                result.map(Ok).unwrap_or_else(||execute_statements(program.statements(),&mut context,&mut scope,&mut values)))
        } else {execute_statements(program.statements(),&mut context,&mut scope,&mut values)}.unwrap();
        let EvalControl::Return(result)=result else {panic!("native fixture returns its result")};
        assert_eq!(values.get(result),FakeValue::Int(1));values.release(result).unwrap();
        assert_eq!(values.output,"DNFCA5");
        for value in scope.drain_owned_cells(){values.release(value).unwrap();}
        for value in context.take_persistent_cell_owners(){values.release(value).unwrap();}
        assert!(context.take_pending_throw().is_none());
        assert_eq!(values.cell_owners.values().sum::<usize>(),0);
    }
}

/// Descriptor staging validates logical declarations before losing source packets to a value array.
#[test]
fn strict_descriptor_sources_validate_inputs_before_native_staging() {
    for declared in [false,true] {for source in ["int","mixed"] {
        let mut context=ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Mixed,true)).unwrap();
        let mut values=FakeOps::default();let value=values.int(1).unwrap();
        let mut argument=native_source_argument(value,None,source,&context);argument.value=value;
        let signature=NativeCallableSignatureMetadata {
            has_logical_sources:true,
            return_type:Some(elephc_generics::Type::atom("int")),display_name:"descriptor_source".into(),
            parameters:vec![NativeCallableParameterMetadata{name:"value".into(),type_tag:EVAL_TAG_INT,by_ref:false,
                logical_type:declared.then(||elephc_generics::Type::atom("int"))}],
            regular_param_count:1,variadic_index:None,
        };
        let result=stage_native_descriptor_args(&signature,vec![argument],&mut context,&mut values);
        if declared && source=="mixed" {
            assert!(matches!(result,Err(EvalStatus::UncaughtThrowable)));
            let error=context.take_pending_throw().unwrap();
            assert!(values.object_is_a(error,"TypeError",false).unwrap());values.release(error).unwrap();
        } else {for value in result.unwrap().values {values.release(value).unwrap();}}
        assert!(context.take_pending_throw().is_none());
        assert_eq!(values.cell_owners.values().sum::<usize>(),0);
    }}
}

/// Legacy descriptors retain permissive calls but cannot silently grant open strict parameter contracts.
#[test]
fn strict_descriptor_sources_reject_missing_metadata_only_in_mixed_mode() {
    for strict in [false,true] {
        let mut context=ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Mixed,strict)).unwrap();
        let mut values=FakeOps::default();let value=values.int(1).unwrap();
        let mut argument=native_source_argument(value,None,"int",&context);argument.value=value;
        let signature=NativeCallableSignatureMetadata {
            has_logical_sources:false,return_type:None,display_name:"legacy_descriptor".into(),
            parameters:vec![NativeCallableParameterMetadata{name:"value".into(),type_tag:EVAL_TAG_INT,
                by_ref:false,logical_type:None}],regular_param_count:1,variadic_index:None,
        };
        let result=stage_native_descriptor_args(&signature,vec![argument],&mut context,&mut values);
        if strict {
            assert!(matches!(result,Err(EvalStatus::UncaughtThrowable)));
            let error=context.take_pending_throw().unwrap();
            assert!(values.object_is_a(error,"TypeError",false).unwrap());values.release(error).unwrap();
        } else {for value in result.unwrap().values {values.release(value).unwrap();}}
        assert_eq!(values.cell_owners.values().sum::<usize>(),0);
    }
}
