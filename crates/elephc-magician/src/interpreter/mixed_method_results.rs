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

/// Accepts object source domains while leaving method existence and access to ordinary dispatch.
pub(super) fn object_source(ty: &Type, context: &ElephcEvalContext) -> bool {
    match ty {
        Type::Atom(name) => name == "object" || name == "Closure" || context.resolve_class_like_name(name).is_some(),
        Type::Apply(name,_) => name != "array" && context.resolve_class_like_name(&ty.to_string()).is_some(),
        Type::Callable(_,_) => true,
        _ => false,
    }
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
