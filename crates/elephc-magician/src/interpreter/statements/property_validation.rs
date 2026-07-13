//! Purpose:
//! Validates property-hook context, readonly writes, visibility, and property failures.
//!
//! Called from:
//! - Instance and static property access paths.
//!
//! Key details:
//! - Synthetic hook names and PHP-visible access errors stay centralized.

use super::*;

/// Returns the synthetic get-hook method name for one property.
pub(in crate::interpreter) fn property_hook_get_method(property_name: &str) -> String {
    format!("__propget_{property_name}")
}

/// Returns the synthetic set-hook method name for one property.
pub(in crate::interpreter) fn property_hook_set_method(property_name: &str) -> String {
    format!("__propset_{property_name}")
}

/// Rejects writes to readonly eval-declared properties outside their declaring constructor.
pub(super) fn validate_eval_readonly_property_write(
    declaring_class: &str,
    property: &EvalClassProperty,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    if !property.is_readonly() {
        return Ok(());
    }
    current_eval_method_is_declaring_constructor(declaring_class, context)
        .then_some(())
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Returns true while executing `__construct` for the property declaring class.
pub(super) fn current_eval_method_is_declaring_constructor(
    declaring_class: &str,
    context: &ElephcEvalContext,
) -> bool {
    let Some(current_class) = context.current_class_scope() else {
        return false;
    };
    if !same_eval_class_name(current_class, declaring_class) {
        return false;
    }
    context
        .current_function()
        .and_then(|function| function.rsplit_once("::"))
        .is_some_and(|(_, method)| method.eq_ignore_ascii_case("__construct"))
}

/// Resolves the property metadata visible from the current class scope, if any.
pub(super) fn eval_dynamic_property_for_access(
    object_class_name: &str,
    property_name: &str,
    context: &ElephcEvalContext,
) -> Option<(String, EvalClassProperty)> {
    if let Some(current_class) = context.current_class_scope() {
        if context.class_is_a(object_class_name, current_class, false) {
            if let Some((declaring_class, property)) =
                context.class_own_property(current_class, property_name)
            {
                if property.visibility() == EvalVisibility::Private {
                    return Some((declaring_class, property));
                }
            }
        }
    }
    context.class_property(object_class_name, property_name)
}

/// Returns the physical storage name for an eval object property slot.
pub(in crate::interpreter) fn eval_instance_property_storage_name(
    declaring_class: &str,
    property: &EvalClassProperty,
) -> String {
    if property.visibility() == EvalVisibility::Private {
        format!(
            "\0{}\0{}",
            declaring_class.trim_start_matches('\\'),
            property.name()
        )
    } else {
        property.name().to_string()
    }
}

/// Validates the visibility that applies to property writes, including asymmetric `set` visibility.
pub(super) fn validate_eval_property_write_access(
    declaring_class: &str,
    property: &EvalClassProperty,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    validate_eval_member_access(declaring_class, property.write_visibility(), context)
}

/// Throws PHP's inaccessible property error for eval-declared properties.
pub(super) fn eval_throw_property_access_error<T>(
    declaring_class: &str,
    property_name: &str,
    visibility: EvalVisibility,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Cannot access {} property {}::${}",
            eval_visibility_label(visibility),
            declaring_class.trim_start_matches('\\'),
            property_name
        ),
        context,
        values,
    )
}

/// Throws PHP's write access error for eval-declared properties.
pub(super) fn eval_throw_property_write_access_error<T>(
    declaring_class: &str,
    property: &EvalClassProperty,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    if let Some(set_visibility) = property.set_visibility() {
        return eval_throw_error(
            &format!(
                "Cannot modify {}(set) property {}::${} from {}",
                eval_visibility_label(set_visibility),
                declaring_class.trim_start_matches('\\'),
                property.name(),
                eval_native_constructor_scope_label(context)
            ),
            context,
            values,
        );
    }
    eval_throw_property_access_error(
        declaring_class,
        property.name(),
        property.write_visibility(),
        context,
        values,
    )
}

/// Throws PHP's unset access error for asymmetric eval-declared properties.
pub(super) fn eval_throw_property_unset_access_error<T>(
    declaring_class: &str,
    property: &EvalClassProperty,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    if let Some(set_visibility) = property.set_visibility() {
        return eval_throw_error(
            &format!(
                "Cannot unset {}(set) property {}::${} from {}",
                eval_visibility_label(set_visibility),
                declaring_class.trim_start_matches('\\'),
                property.name(),
                eval_native_constructor_scope_label(context)
            ),
            context,
            values,
        );
    }
    eval_throw_property_access_error(
        declaring_class,
        property.name(),
        property.write_visibility(),
        context,
        values,
    )
}

/// Throws PHP's read-only property-hook write error.
pub(super) fn eval_throw_property_hook_readonly_error<T>(
    declaring_class: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Property {}::${} is read-only",
            declaring_class.trim_start_matches('\\'),
            property_name
        ),
        context,
        values,
    )
}

/// Throws PHP's readonly property assignment error for eval-declared properties.
pub(super) fn eval_throw_readonly_property_modification_error<T>(
    declaring_class: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Cannot modify readonly property {}::${}",
            declaring_class.trim_start_matches('\\'),
            property_name
        ),
        context,
        values,
    )
}

/// Throws PHP's readonly property unset error for eval-declared properties.
pub(super) fn eval_throw_readonly_property_unset_error<T>(
    declaring_class: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Cannot unset readonly property {}::${}",
            declaring_class.trim_start_matches('\\'),
            property_name
        ),
        context,
        values,
    )
}

/// Throws PHP's dynamic property creation error for readonly eval-declared classes.
pub(super) fn eval_throw_dynamic_property_creation_error<T>(
    class_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Cannot create dynamic property {}::${}",
            class_name.trim_start_matches('\\'),
            property_name
        ),
        context,
        values,
    )
}

/// Throws PHP's undeclared static property error for static property access.
pub(super) fn eval_throw_undeclared_static_property_error<T>(
    class_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Access to undeclared static property {}::${}",
            class_name.trim_start_matches('\\'),
            property_name
        ),
        context,
        values,
    )
}

/// Throws PHP's uninitialized typed instance property error.
pub(super) fn eval_throw_uninitialized_property_error<T>(
    declaring_class: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Typed property {}::${} must not be accessed before initialization",
            declaring_class.trim_start_matches('\\'),
            property_name
        ),
        context,
        values,
    )
}

/// Throws PHP's uninitialized typed static property error.
pub(super) fn eval_throw_uninitialized_static_property_error<T>(
    declaring_class: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Typed static property {}::${} must not be accessed before initialization",
            declaring_class.trim_start_matches('\\'),
            property_name
        ),
        context,
        values,
    )
}

/// Throws PHP's class-not-found error for unresolved static member receivers.
pub(in crate::interpreter) fn eval_throw_class_not_found_error<T>(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!("Class \"{}\" not found", class_name.trim_start_matches('\\')),
        context,
        values,
    )
}

/// Throws PHP's inaccessible constant error for eval-declared class constants.
pub(super) fn eval_throw_constant_access_error<T>(
    declaring_class: &str,
    constant_name: &str,
    visibility: EvalVisibility,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Cannot access {} constant {}::{}",
            eval_visibility_label(visibility),
            declaring_class.trim_start_matches('\\'),
            constant_name
        ),
        context,
        values,
    )
}

/// Throws PHP's inaccessible method error for eval-declared methods.
pub(super) fn eval_throw_method_access_error<T>(
    declaring_class: &str,
    method_name: &str,
    visibility: EvalVisibility,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Call to {} method {}::{}() from {}",
            eval_visibility_label(visibility),
            declaring_class.trim_start_matches('\\'),
            method_name,
            eval_native_constructor_scope_label(context)
        ),
        context,
        values,
    )
}

/// Throws PHP's inaccessible clone-expression error for `__clone()` hooks.
pub(super) fn eval_throw_clone_access_error<T>(
    declaring_class: &str,
    visibility: EvalVisibility,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Call to {} {}::__clone() from {}",
            eval_visibility_label(visibility),
            declaring_class.trim_start_matches('\\'),
            eval_native_constructor_scope_label(context)
        ),
        context,
        values,
    )
}

/// Throws PHP's error for calling an instance method through static syntax.
pub(super) fn eval_throw_non_static_method_call_error<T>(
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

/// Throws PHP's error for calling an abstract method directly.
pub(super) fn eval_throw_abstract_method_call_error<T>(
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

/// Throws PHP's undefined method error after static magic fallback misses.
pub(super) fn eval_throw_undefined_method_call_error<T>(
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

/// Throws PHP's error for invoking an object without `__invoke()`.
pub(in crate::interpreter) fn eval_throw_object_not_callable_error<T>(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Object of type {} is not callable",
            class_name.trim_start_matches('\\')
        ),
        context,
        values,
    )
}
