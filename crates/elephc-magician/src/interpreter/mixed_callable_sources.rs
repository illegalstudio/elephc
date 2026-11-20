//! Purpose:
//! Captures declared return alternatives from one already normalized callable identity.
//!
//! Called from:
//! - Dynamic callable invocation and normalized receiver owner scopes.
//!
//! Key details:
//! - Return metadata is captured before argument effects and owns no PHP values.
//! - Missing declarations remain unknown; descriptor storage tags supply no logical proof.

use super::*;
use elephc_generics::Type;
use crate::eval_ir::generic_type;

/// Converts a declaration in its selected class and late-static scopes.
fn declared(ty: Option<&EvalParameterType>,owner: Option<&str>,called: Option<&str>,
    context: &ElephcEvalContext,
) -> Result<Option<Type>,EvalStatus> {
    ty.map(|ty|local_binding_origins::resolve_declared_source_type(&generic_type(ty),owner,called,context)).transpose()
}

/// Reads a named function, closure or builtin declaration without inferring a result from storage.
pub(super) fn named(name: &str,context: &ElephcEvalContext) -> Result<Option<Type>,EvalStatus> {
    if let Some(closure)=context.closure(name) {return declared(closure.function().return_type(),None,None,context);}
    if let Some(function)=context.function(name) {return declared(function.return_type(),None,None,context);}
    if let Some(function)=context.native_function(name) {return declared(function.return_type(),None,None,context);}
    Ok(elephc_builtin_contract::lookup(name.trim_start_matches('\\'))
        .map(|contract|local_binding_origins::builtin_producer_type(contract.returns)))
}

/// Identifies the selected receiver class while its normalized callback owner remains alive.
pub(super) fn object_class(object: RuntimeCellHandle,context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String,EvalStatus> {
    if let Some(class)=values.object_identity(object).ok().and_then(|identity|context.dynamic_object_class(identity)) {
        return Ok(class.name().to_string());
    }
    runtime_object_class_name(object,values)
}

/// Reads an invokable object's declared return without dispatching its method a second time.
pub(super) fn invokable(object: RuntimeCellHandle,context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<Type>,EvalStatus> {
    let class=object_class(object,context,values)?;
    Ok(local_binding_origins::declared_method_result(&class,&class,"__invoke",false,context))
}

/// Keeps structural return alternatives from the actual normalized callable, including bound scopes.
pub(super) fn resolved(callback: &EvaluatedCallable,context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<Type>,EvalStatus> {
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::Mixed) { return Ok(None); }
    match callback {
        EvaluatedCallable::ForeignContext{callback,owner}=>{
            let context=unsafe{owner.context_ptr().as_ref()}.ok_or(EvalStatus::RuntimeFatal)?;
            resolved(callback,context,values)
        },
        EvaluatedCallable::NativeDescriptor{callback,..}=>{
            let source=values.native_callable_signature(*callback)?.and_then(|signature|signature.return_type);
            Ok(source.and_then(|ty|local_binding_origins::resolve_declared_source_type(&ty,None,None,context).ok()))
        },
        EvaluatedCallable::Named{name,..}=>named(name,context),
        EvaluatedCallable::BoundClosure{name,bound_this,bound_scope}=>{
            let Some(closure)=context.closure(name) else {return named(name,context);};
            let called=bound_this.map(|object|object_class(object,context,values)).transpose()?;
            let called=called.as_deref().or(bound_scope.as_deref());
            declared(closure.function().return_type(),bound_scope.as_deref().or(called),called,context)
        },
        EvaluatedCallable::InvokableObject{object}=>invokable(*object,context,values),
        EvaluatedCallable::ObjectMethod{object,method,called_class,native_class,bridge_scope}=>{
            let class=object_class(*object,context,values)?;
            let owner=bridge_scope.as_deref().or(native_class.as_deref()).unwrap_or(&class);
            Ok(local_binding_origins::declared_method_result(owner,called_class.as_deref().unwrap_or(&class),method,false,context))
        },
        EvaluatedCallable::StaticMethod{class_name,method,called_class,native_class,bridge_scope}=>{
            let owner=bridge_scope.as_deref().or(native_class.as_deref()).unwrap_or(class_name);
            Ok(local_binding_origins::declared_method_result(owner,called_class.as_deref().unwrap_or(class_name),method,true,context))
        },
    }
}

/// Tests callable operand domains while leaving symbol existence and callable shapes to normal resolution.
pub(super) fn supports(ty: &Type,context: &ElephcEvalContext) -> bool {
    match ty {
        Type::Atom(name) if matches!(name.as_str(),"string"|"array"|"callable"|"Closure")=>true,
        Type::Apply(name,_) if name=="array"=>true,
        Type::Callable(_,_)=>true,
        Type::Atom(name)=>context.class_method(name,"__invoke").is_some()
            || context.native_method_signature(name,"__invoke").is_some(),
        _=>false,
    }
}

/// Invokes evaluated arguments while forwarding wrapper results independently from the wrapper signature.
pub(super) fn invoke(callback: &EvaluatedCallable, arguments: Vec<EvaluatedCallArg>, by_value: bool,
    source: Option<Type>, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<Type>), EvalStatus> {
    if let EvaluatedCallable::ForeignContext { callback, owner } = callback {
        let context = unsafe { owner.context_ptr().as_mut() }.ok_or(EvalStatus::RuntimeFatal)?;
        return invoke(callback, arguments, by_value, source, context, values);
    }
    if let EvaluatedCallable::Named { name, .. } = callback {
        if context.semantic_profile().contains(elephc_semantics::StrictFeature::Mixed)
            && matches!(name.as_str(), "call_user_func" | "call_user_func_array") {
            return eval_builtin_callback_with_arguments_and_source(name, arguments, !by_value, context, values);
        }
    }
    let mut source = source;
    if context.semantic_profile().contains(elephc_semantics::StrictFeature::Mixed) {
        if let EvaluatedCallable::ObjectMethod { object, method, .. } = callback {
            if let Some(reflected) = eval_reflection_invocation_source(*object, method, &arguments, context, values)? {
                source = reflected;
            }
        }
    }
    let result = if by_value {
        eval_evaluated_callable_with_by_value_call_args(callback, arguments, context, values)
    } else { eval_evaluated_callable_with_call_array_args(callback, arguments, context, values) };
    result.map(|value| (value, source))
}
