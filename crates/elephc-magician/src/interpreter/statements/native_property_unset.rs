//! Purpose:
//! Authorizes eval unsets of physical native properties, including inherited slots.
//!
//! Called from:
//! - Instance property unset dispatch for native objects and eval subclasses.
//!
//! Key details:
//! - Visibility is checked in the caller's scope before selecting a native storage scope.
//! - Readonly and hooked properties must not reach the storage-clearing helper.

use super::*;
use std::cell::RefCell;
use std::collections::HashSet;

thread_local! {
    /// Active magic unsets share the runtime thread, including reentrant native-to-eval contexts.
    static ACTIVE_PROPERTY_UNSETS: RefCell<HashSet<(u64, String)>> = RefCell::new(HashSet::new());
}

/// Clears an object/property recursion guard on both ordinary return and Rust error unwinding.
pub(super) struct EvalPropertyUnsetGuard((u64, String));

impl EvalPropertyUnsetGuard {
    /// Enters a magic unset unless the same object/property is already being handled.
    pub(super) fn enter(identity: u64, property: &str) -> Option<Self> {
        let key = (identity, property.to_string());
        ACTIVE_PROPERTY_UNSETS.with(|active| active.borrow_mut().insert(key.clone()))
            .then(|| Self(key))
    }
}

impl Drop for EvalPropertyUnsetGuard {
    /// Removes only this invocation's guard so later independent unsets may call the hook again.
    fn drop(&mut self) {
        ACTIVE_PROPERTY_UNSETS.with(|active| active.borrow_mut().remove(&self.0));
    }
}

/// Invokes an inherited or direct native magic unsetter, balancing its temporary name and return.
pub(super) fn eval_native_magic_property_unset(
    object: RuntimeCellHandle,
    object_class: &str,
    property: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let native_class = context.class_native_parent_name(object_class)
        .unwrap_or_else(|| object_class.to_string());
    if values.reflection_method_flags(&native_class, "__unset")?.is_none() {
        return Ok(false);
    }
    let name = values.string(property)?;
    let result = eval_native_method_with_evaluated_args(
        object, &native_class, "__unset", positional_args(vec![name.borrowed()]), context, values,
    );
    let result = result.and_then(|result| release_expr_result(result, context, values));
    let released = release_expr_result(name, context, values);
    result.and(released).map(|()| true)
}

/// Unsets a native property after validating its PHP metadata, returning false for absent slots.
pub(super) fn eval_native_property_unset_result(
    object: RuntimeCellHandle,
    object_class: &str,
    property: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let native_class = eval_native_private_property_unset_scope(object_class, property, context, values)?
        .or_else(|| context.class_native_parent_name(object_class))
        .unwrap_or_else(|| object_class.to_string());
    let Some((owner, visibility, write_visibility, is_static)) =
        eval_reflection_aot_property_access_metadata(&native_class, property, values)?
    else {
        return Ok(false);
    };
    if is_static {
        return Ok(false);
    }
    if validate_eval_member_access(&owner, visibility, context).is_err() {
        if eval_magic_property_unset(object, object_class, property, context, values)? {
            return Ok(true);
        }
        return eval_throw_property_access_error(&owner, property, visibility, context, values);
    }
    let flags = values.reflection_property_flags(&native_class, property)?
        .ok_or(EvalStatus::RuntimeFatal)?;
    let has_hook = if flags & EVAL_REFLECTION_MEMBER_FLAG_VIRTUAL != 0 {
        true
    } else {
        let get = values.reflection_method_flags(&owner, &property_hook_get_method(property))?;
        let set = values.reflection_method_flags(&owner, &property_hook_set_method(property))?;
        get.into_iter().chain(set)
            .any(|flags| flags & EVAL_REFLECTION_METHOD_FLAG_PROPERTY_HOOK != 0)
    };
    if has_hook {
        return eval_throw_hooked_property_unset_error(object_class, property, context, values);
    }
    if flags & EVAL_REFLECTION_MEMBER_FLAG_READONLY != 0 {
        let initialized = eval_with_native_bridge_scope(&owner, context, || {
            values.property_is_initialized(object, property)
        })?;
        if initialized {
            return eval_throw_readonly_property_unset_error(&owner, property, context, values);
        }
        eval_validate_readonly_unset_scope(&owner, property, write_visibility, context, values)?;
    }
    if validate_eval_member_access(&owner, write_visibility, context).is_err() {
        if flags & (EVAL_REFLECTION_MEMBER_FLAG_PRIVATE_SET | EVAL_REFLECTION_MEMBER_FLAG_PROTECTED_SET) != 0 {
            return eval_throw_error(
                &format!("Cannot unset {}(set) property {}::${property} from {}",
                    eval_visibility_label(write_visibility), owner.trim_start_matches('\\'),
                    eval_native_constructor_scope_label(context)),
                context, values,
            );
        }
        return eval_throw_property_access_error(&owner, property, write_visibility, context, values);
    }
    eval_with_native_bridge_scope(&owner, context, || {
        values.unset_native_typed_property(object, property)
    })
}

/// Enforces implicit readonly set visibility even when old metadata omits an asymmetric flag.
pub(super) fn eval_validate_readonly_unset_scope(
    owner: &str,
    property: &str,
    write_visibility: EvalVisibility,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let modern = crate::eval_php_profile::eval_php_version_id() >= 80400;
    let visibility = match write_visibility {
        EvalVisibility::Public if modern => EvalVisibility::Protected,
        EvalVisibility::Public => EvalVisibility::Private,
        visibility => visibility,
    };
    if validate_eval_member_access(owner, visibility, context).is_ok() {
        return Ok(());
    }
    if modern {
        return eval_throw_error(
            &format!("Cannot unset {}(set) readonly property {}::${property} from {}",
                eval_visibility_label(visibility), owner.trim_start_matches('\\'),
                eval_native_constructor_scope_label(context)),
            context, values,
        );
    }
    eval_throw_readonly_property_unset_error(owner, property, context, values)
}

/// Selects a lexical native private slot before a same-named child declaration can hide it.
pub(super) fn eval_native_private_property_unset_scope(
    object_class: &str,
    property: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    let Some(scope) = context.current_class_scope() else { return Ok(None); };
    let native_class = context.class_native_parent_name(object_class)
        .unwrap_or_else(|| object_class.to_string());
    if !native_class_is_a(&native_class, scope, context) {
        return Ok(None);
    }
    let Some((owner, EvalVisibility::Private, _, false)) =
        eval_reflection_aot_property_access_metadata(scope, property, values)?
    else { return Ok(None); };
    Ok(same_eval_class_name(&owner, scope).then_some(owner))
}

/// Reports PHP's prohibition on unsetting either a virtual or a backed hooked property.
pub(super) fn eval_throw_hooked_property_unset_error<T>(
    object_class: &str,
    property: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!("Cannot unset hooked property {}::${property}", object_class.trim_start_matches('\\')),
        context, values,
    )
}

#[cfg(test)]
mod tests {
    use super::EvalPropertyUnsetGuard;

    /// Denied reentry leaves the outer guard active until its actual owner exits.
    #[test]
    fn denied_magic_unset_reentry_preserves_outer_guard() {
        let outer = EvalPropertyUnsetGuard::enter(1, "value").unwrap();
        assert!(EvalPropertyUnsetGuard::enter(1, "value").is_none());
        assert!(EvalPropertyUnsetGuard::enter(1, "value").is_none());
        assert!(EvalPropertyUnsetGuard::enter(1, "other").is_some());
        assert!(EvalPropertyUnsetGuard::enter(2, "value").is_some());
        drop(outer);
        assert!(EvalPropertyUnsetGuard::enter(1, "value").is_some());
    }
}
