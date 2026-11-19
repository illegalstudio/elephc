//! Purpose:
//! Shares ordinary missing-variable reads across the walker and bytecode VM.
//!
//! Called from:
//! - Expression LoadVar and VM load_local after visible binding lookup.
//!
//! Key details:
//! - Quiet language probes inspect presence directly and do not enter this path.
//! - Initialized null has a visible binding and never reaches this helper.

use super::*;

/// Coalescing can ask a getter directly when no existence hook is declared.
pub(super) fn coalesce_uses_getter(object: RuntimeCellHandle, property: &str,
    context: &ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> bool {
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::Initialization) { return false; }
    let Ok(identity) = values.object_identity(object) else { return false; };
    let Some(class) = context.dynamic_object_class(identity) else { return false; };
    if context.class_method(class.name(), "__get").is_none()
        || context.class_method(class.name(), "__isset").is_some() { return false; }
    match context.class_property(class.name(), property) {
        None => true,
        Some((owner, member)) if member.property_type().is_none() => {
            let storage = eval_instance_property_storage_name(&owner, &member);
            !context.dynamic_property_is_initialized(identity, &storage)
        }
        _ => false,
    }
}

/// Publishes an untyped missing-property error after getter and visibility dispatch.
pub(super) fn missing_property<T>(name: &str, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    throwables::eval_throw_error(&elephc_semantics::initialization::property_message(name), context, values)
}

/// Throws the selected initialization error or preserves PHP's warning and null result.
pub(super) fn missing_variable(name: &str, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if context.semantic_profile().contains(elephc_semantics::StrictFeature::Initialization) {
        return throwables::eval_throw_error(
            &elephc_semantics::initialization::variable_message(name), context, values);
    }
    values.warning(&format!("Warning: Undefined variable ${name}\n"))?;
    values.null()
}
