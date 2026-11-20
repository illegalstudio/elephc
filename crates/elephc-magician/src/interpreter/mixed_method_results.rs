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
    matches!(expr,EvalExpr::DynamicStaticMethodCall {..})
}

/// Accepts object source domains while leaving method existence and access to ordinary dispatch.
pub(super) fn object_source(ty: &Type, context: &ElephcEvalContext) -> bool {
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
    match expr {
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

/// Retains the ordinary value-only static dispatcher after source validation.
pub(super) fn discarded(expr: &EvalExpr, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle,EvalStatus> {
    evaluate(expr,context,scope,values).map(|result|result.0)
}

/// Invokes a selected owned receiver while carrying its declaration through argument and warning cleanup.
pub(super) fn selected_object(object: RuntimeCellHandle, method: &str, args: &[EvalCallArg],
    discarded: bool, context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle,Option<Type>),EvalStatus> {
    let class=mixed_callable_sources::object_class(object,context,values)?;
    let mut source=local_binding_origins::declared_method_result(&class,&class,method,false,context);
    with_eval_object_method_arguments(args,object,method,context,scope,values,|arguments,context,_,values| {
        if let Some(reflected)=eval_reflection_invocation_source(object,method,&arguments,context,values)? {
            source=reflected;
        }
        let result=eval_method_call_result_with_evaluated_args(object,method,arguments,context,values)?;
        if discarded { warn_eval_discarded_instance_result(object,method,result,context,values) }
        else { Ok(result) }
    }).map(|value|(value,source))
}
