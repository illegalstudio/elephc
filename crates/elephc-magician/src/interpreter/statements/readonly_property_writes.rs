//! Purpose:
//! Enforces readonly initialization and overwrite precedence for native properties in eval.
//!
//! Called from:
//! - Instance property writes and readonly setter-access diagnostics.
//!
//! Key details:
//! - Initialized readonly errors precede restricted setters when read visibility permits access.
//! - Access checks never consume a clone rewrite lease; authorized writes consume it once.

use super::*;

/// Checks initialized readonly state before effective setter access, without consuming clone leases.
pub(super) fn validate_native_readonly_property_access(
    object: RuntimeCellHandle,
    identity: u64,
    declaring_class: &str,
    property_name: &str,
    write_visibility: EvalVisibility,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let flags = values.reflection_property_flags(declaring_class, property_name)?
        .unwrap_or_default();
    if flags & EVAL_REFLECTION_MEMBER_FLAG_READONLY == 0 {
        return Ok(());
    }
    if values.property_is_initialized(object, property_name)?
        && !context.clone_property_can_be_reinitialized(identity, property_name)
    {
        return eval_throw_readonly_property_modification_error(
            declaring_class, property_name, context, values,
        );
    }
    if validate_eval_member_access(declaring_class, write_visibility, context).is_err() {
        return eval_throw_readonly_initialization_access_error(
            declaring_class, property_name, write_visibility, context, values,
        );
    }
    Ok(())
}

/// Enforces readonly one-shot initialization for an authorized generated-class property write.
pub(super) fn validate_native_readonly_property_write(
    object: RuntimeCellHandle,
    identity: u64,
    declaring_class: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let flags = values.reflection_property_flags(declaring_class, property_name)?
        .unwrap_or_default();
    if flags & EVAL_REFLECTION_MEMBER_FLAG_READONLY == 0 {
        return Ok(());
    }
    if context.clone_initialization_is_active(identity) {
        if context.consume_clone_reinitialization(identity, property_name) {
            return Ok(());
        }
    } else if !values.property_is_initialized(object, property_name)? {
        return Ok(());
    }
    eval_throw_readonly_property_modification_error(
        declaring_class, property_name, context, values,
    )
}

/// Reports first-write readonly setter access with PHP's protected versus private spelling.
pub(super) fn eval_throw_readonly_initialization_access_error<T>(
    declaring_class: &str,
    property_name: &str,
    write_visibility: EvalVisibility,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    let readonly = if write_visibility == EvalVisibility::Protected { "readonly " } else { "" };
    eval_throw_error(
        &format!("Cannot modify {}(set) {readonly}property {}::${property_name} from {}",
            eval_visibility_label(write_visibility), declaring_class.trim_start_matches('\\'),
            eval_native_constructor_scope_label(context)),
        context,
        values,
    )
}
