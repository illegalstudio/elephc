//! Purpose:
//! Captures method result sources after one receiver and member selection.
//!
//! Called from:
//! - Mixed expression evaluation and ordinary method expression entry points.
//!
//! Key details:
//! - Selected declarations supply evidence; runtime return tags never do.
//! - Argument and receiver cleanup retain the existing scalar owner adapters.

use super::*;
use elephc_generics::Type;

/// Recognizes method expressions whose target selection supplies a logical result declaration.
pub(super) fn handles(expr: &EvalExpr) -> bool {
    !nullsafe_chains::has_nullsafe(expr) && matches!(expr,
        EvalExpr::MethodCall {..} | EvalExpr::DynamicMethodCall {..} | EvalExpr::DynamicStaticMethodCall {..})
}

/// Accepts object source domains while leaving method existence and access to ordinary dispatch.
fn object_source(ty: &Type, context: &ElephcEvalContext) -> bool {
    match ty {
        Type::Atom(name) => name == "object" || name == "Closure" || context.resolve_class_like_name(name).is_some(),
        Type::Apply(name,_) => name != "array" && context.resolve_class_like_name(&ty.to_string()).is_some(),
        Type::Callable(_,_) => true,
        _ => false,
    }
}

/// Evaluates selected object and static method targets once with source-order argument effects.
pub(super) fn evaluate(expr: &EvalExpr, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<Type>), EvalStatus> {
    evaluate_selected(expr,false,context,scope,values)
}

/// Preserves discarded-result warnings inside the same selected receiver owner scope.
pub(super) fn discarded(expr: &EvalExpr, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle,EvalStatus> {
    evaluate_selected(expr,true,context,scope,values).map(|result|result.0)
}

/// Shares target selection between value consumers and discarded-result diagnostics.
fn evaluate_selected(expr: &EvalExpr, discarded: bool, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<Type>), EvalStatus> {
    match expr {
        EvalExpr::MethodCall {object,method,args} | EvalExpr::NullsafeMethodCall {object,method,args} =>
            object_call(expr,object,Some(method),None,args,discarded,context,scope,values),
        EvalExpr::DynamicMethodCall {object,method,args} | EvalExpr::NullsafeDynamicMethodCall {object,method,args} =>
            object_call(expr,object,None,Some(method),args,discarded,context,scope,values),
        EvalExpr::DynamicStaticMethodCall {class_name,method,args} =>
            mixed_eval::with_sources(&[class_name],context,scope,values,|receivers,sources,context,scope,values| {
                let source=sources[0].clone().unwrap_or_else(||Type::atom("mixed"));
                mixed_source_proofs::require_source(&source,"class selection",context,values,|ty,context|
                    *ty==Type::atom("string") || object_source(ty,context))?;
                let class=eval_dynamic_class_name(receivers[0],context,values)?;
                let method=eval_dynamic_member_name(method,context,scope,values)?;
                let receiver=resolve_eval_static_method_receiver(&class,context)?;
                let source=local_binding_origins::declared_method_result(&receiver.dispatch_class,&receiver.called_class,&method,true,context);
                let modes=CallArgumentModes::for_class_method(&class,&method,true,context);
                with_eval_call_arguments_with_modes(args,&modes,context,scope,values,|arguments,context,scope,values|
                    eval_static_method_call_result_from_scope(&class,&method,arguments,scope,context,values))
                    .map(|value|(value,source))
            }),
        _=>Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Retains a receiver through selector evaluation, reflected forwarding and ordinary method invocation.
fn object_call(expr: &EvalExpr, object: &EvalExpr, method: Option<&str>, dynamic: Option<&EvalExpr>,
    args: &[EvalCallArg], discarded: bool, context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle,Option<Type>),EvalStatus> {
    let nullsafe=matches!(expr,EvalExpr::NullsafeMethodCall {..}|EvalExpr::NullsafeDynamicMethodCall {..});
    mixed_eval::with_sources(&[object],context,scope,values,|receivers,sources,context,scope,values| {
        let source=sources[0].clone().map(mixed_source_proofs::value_source).unwrap_or_else(||Type::atom("mixed"));
        mixed_source_proofs::require_source(&source,"member access",context,values,|ty,context|
            object_source(ty,context) || nullsafe && *ty==Type::atom("null"))?;
        if nullsafe && values.is_null(receivers[0])? { return values.null().map(|value|(value,Some(Type::atom("null")))); }
        let method=match method {Some(method)=>method.to_string(),None=>
            eval_dynamic_member_name(dynamic.ok_or(EvalStatus::RuntimeFatal)?,context,scope,values)?};
        let class=mixed_callable_sources::object_class(receivers[0],context,values)?;
        let mut source=local_binding_origins::declared_method_result(&class,&class,&method,false,context);
        with_eval_object_method_arguments(args,receivers[0],&method,context,scope,values,|arguments,context,_,values| {
            if let Some(reflected)=eval_reflection_invocation_source(receivers[0],&method,&arguments,context,values)? {
                source=reflected;
            }
            let result=eval_method_call_result_with_evaluated_args(receivers[0],&method,arguments,context,values)?;
            if discarded { warn_eval_discarded_instance_result(receivers[0],&method,result,context,values) }
            else { Ok(result) }
        }).map(|value|(value,source))
    })
}
