//! Purpose:
//! Reads logical declarations for actual eval-backed reflected invocation targets.
//!
//! Called from:
//! - Source-aware method expression dispatch before invoking a reflected callable.
//!
//! Key details:
//! - The outer option distinguishes ordinary methods from unknown reflected returns.
//! - Binding inspections borrow evaluated arguments and acquire no PHP owners.

use super::*;
use elephc_generics::Type;

/// Resolves a reflected target's declared return without inspecting its future returned value.
pub(in crate::interpreter) fn eval_reflection_invocation_source(
    object: RuntimeCellHandle, method: &str, arguments: &[EvaluatedCallArg],
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Option<Option<Type>>,EvalStatus> {
    if !method.eq_ignore_ascii_case("invoke") && !method.eq_ignore_ascii_case("invokeArgs") { return Ok(None); }
    let Ok(identity)=values.object_identity(object) else { return Ok(None); };
    if let Some(function)=context.eval_reflection_function_name(identity) {
        return crate::interpreter::mixed_callable_sources::named(function,context).map(Some);
    }
    let Some((owner,target))=context.eval_reflection_method(identity)
        .map(|(owner,target)|(owner.to_string(),target.to_string())) else { return Ok(None); };
    let is_static=context.class_method(&owner,&target).is_some_and(|(_,method)|method.is_static())
        || context.native_static_method_signature(&owner,&target).is_some();
    let called=if is_static {owner.clone()} else {
        let object=if method.eq_ignore_ascii_case("invokeArgs") {
            eval_reflection_method_invoke_args_array(arguments.to_vec())?.0
        } else {eval_reflection_method_invoke_args(arguments.to_vec())?.0};
        crate::interpreter::mixed_callable_sources::object_class(object,context,values)?
    };
    Ok(Some(crate::interpreter::local_binding_origins::declared_method_result(&owner,&called,&target,is_static,context)))
}
