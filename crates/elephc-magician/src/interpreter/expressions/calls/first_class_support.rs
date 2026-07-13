//! Purpose:
//! Shared magic-call probes and TypeError helpers for first-class callables.
//!
//! Called from:
//! - `crate::interpreter::expressions::calls::first_class`.
//!
//! Key details:
//! - Error messages preserve PHP visibility labels and current eval class scope.
//! - AOT magic-call probes use generated method dispatch metadata.

use super::*;

/// Returns whether an eval class has an instance magic-call fallback for a callable.
pub(super) fn eval_instance_magic_callable_for_class(
    class_name: &str,
    context: &ElephcEvalContext,
) -> bool {
    context
        .class_method(class_name, "__call")
        .is_some_and(|(_, method)| !method.is_static() && !method.is_abstract())
}

/// Returns whether an eval class has a static magic-call fallback for a callable.
pub(super) fn eval_static_magic_callable_for_class(
    class_name: &str,
    context: &ElephcEvalContext,
) -> bool {
    context
        .class_method(class_name, "__callStatic")
        .is_some_and(|(_, method)| method.is_static() && !method.is_abstract())
}

/// Returns whether an AOT class has an instance magic-call fallback for a callable.
pub(super) fn eval_native_instance_magic_callable_for_class(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(eval_aot_method_dispatch_metadata_in_hierarchy(class_name, "__call", context, values)?
        .is_some_and(|(_, _, is_static, is_abstract)| !is_static && !is_abstract))
}

/// Returns whether an AOT class has a static magic-call fallback for a callable.
pub(super) fn eval_native_static_magic_callable_for_class(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(
        eval_aot_method_dispatch_metadata_in_hierarchy(
            class_name,
            "__callStatic",
            context,
            values,
        )?
        .is_some_and(|(_, _, is_static, is_abstract)| is_static && !is_abstract),
    )
}

/// Throws PHP's first-class callable error for an inaccessible method.
pub(super) fn eval_first_class_method_access_error<T>(
    declaring_class: &str,
    method_name: &str,
    visibility: EvalVisibility,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Call to {} method {}::{}() from {}",
            eval_callable_visibility_label(visibility),
            declaring_class.trim_start_matches('\\'),
            method_name,
            eval_callable_scope_label(context)
        ),
        context,
        values,
    )
}

/// Throws PHP's first-class callable error for an instance method used statically.
pub(super) fn eval_first_class_non_static_method_error<T>(
    declaring_class: &str,
    method_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Non-static method {}::{}() cannot be called statically",
            declaring_class.trim_start_matches('\\'),
            method_name
        ),
        context,
        values,
    )
}

/// Throws PHP's first-class callable error for an abstract method target.
pub(super) fn eval_first_class_abstract_method_error<T>(
    declaring_class: &str,
    method_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Cannot call abstract method {}::{}()",
            declaring_class.trim_start_matches('\\'),
            method_name
        ),
        context,
        values,
    )
}

/// Throws PHP's first-class callable error for a missing method target.
pub(super) fn eval_first_class_undefined_method_error<T>(
    class_name: &str,
    method_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Call to undefined method {}::{}()",
            class_name.trim_start_matches('\\'),
            method_name
        ),
        context,
        values,
    )
}

/// Returns the current PHP scope label used in callable access errors.
fn eval_callable_scope_label(context: &ElephcEvalContext) -> String {
    context.current_class_scope().map_or_else(
        || String::from("global scope"),
        |class_name| format!("scope {}", class_name.trim_start_matches('\\')),
    )
}

/// Returns PHP's lowercase visibility label for callable access errors.
fn eval_callable_visibility_label(visibility: EvalVisibility) -> &'static str {
    match visibility {
        EvalVisibility::Public => "public",
        EvalVisibility::Protected => "protected",
        EvalVisibility::Private => "private",
    }
}
