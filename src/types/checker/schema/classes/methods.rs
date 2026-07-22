//! Purpose:
//! Validates class schema methods rules.
//! Owns one slice of class metadata construction used by object inference and method checking.
//!
//! Called from:
//! - `crate::types::checker::schema::classes`
//!
//! Key details:
//! - Class metadata is shared globally after construction, so validation must reject inconsistent inheritance early.

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{ClassMethod, Visibility};
use crate::types::traits::FlattenedClass;

use super::super::super::Checker;
use super::super::validation::{
    build_method_sig, matches_global_builtin_attribute, validate_override_signature,
    visibility_rank,
};
use super::state::ClassBuildState;
use super::{collect_attribute_args, collect_attribute_names};

/// Validates and registers all methods of a flattened class into the build state.
/// Enforces abstract/final modifiers, method body presence, and delegates to
/// `apply_static_method` or `apply_instance_method` based on `method.is_static`.
pub(super) fn apply_methods(
    state: &mut ClassBuildState,
    class: &FlattenedClass,
    checker: &Checker,
) -> Result<(), CompileError> {
    for method in &class.methods {
        validate_method_shape(class, method)?;
        if method.is_static {
            apply_static_method(state, class, checker, method)?;
        } else {
            apply_instance_method(state, class, checker, method)?;
        }
    }
    Ok(())
}

/// Validates shape constraints on a single method: abstract+final conflict,
/// abstract method with a body, non-abstract method without a body, and
/// private abstract methods are all rejected.
fn validate_method_shape(
    class: &FlattenedClass,
    method: &ClassMethod,
) -> Result<(), CompileError> {
    if method.is_abstract && method.is_final {
        return Err(CompileError::new(
            method.span,
            &format!(
                "Cannot use the final modifier on an abstract method: {}::{}",
                class.name, method.name
            ),
        ));
    }
    if method.is_abstract && method.has_body {
        return Err(CompileError::new(
            method.span,
            &format!(
                "Abstract method cannot have a body: {}::{}",
                class.name, method.name
            ),
        ));
    }
    if !method.is_abstract && !method.has_body {
        return Err(CompileError::new(
            method.span,
            &format!(
                "Non-abstract method must have a body: {}::{}",
                class.name, method.name
            ),
        ));
    }
    if method.is_abstract && method.visibility == Visibility::Private {
        return Err(CompileError::new(
            method.span,
            &format!(
                "Private abstract methods are not supported: {}::{}",
                class.name, method.name
            ),
        ));
    }
    Ok(())
}

/// Validates and registers a static method into `ClassBuildState`. Checks for
/// final method conflicts, static/instance kind conflicts, visibility reduction,
/// signature override compatibility, `#[Override]` attribute targets, and registers
/// the method in the static vtable when visibility is non-private.
fn apply_static_method(
    state: &mut ClassBuildState,
    class: &FlattenedClass,
    checker: &Checker,
    method: &ClassMethod,
) -> Result<(), CompileError> {
    let method_key = php_symbol_key(&method.name);
    let sig = build_method_sig(checker, method, &class.name)?;
    if state.final_methods.contains(&method_key) {
        return Err(final_method_error(
            state
                .method_declaring_classes
                .get(&method_key)
                .cloned()
                .unwrap_or_else(|| class.name.clone()),
            method,
        ));
    }
    if state.method_sigs.contains_key(&method_key) {
        return Err(method_kind_error(class, method));
    }
    if state.final_static_methods.contains(&method_key) {
        return Err(final_method_error(
            state
                .static_method_declaring_classes
                .get(&method_key)
                .cloned()
                .unwrap_or_else(|| class.name.clone()),
            method,
        ));
    }
    if let Some(parent_visibility) = state.static_method_visibilities.get(&method_key) {
        if visibility_rank(&method.visibility) < visibility_rank(parent_visibility) {
            return Err(CompileError::new(
                method.span,
                &format!(
                    "Cannot reduce visibility when overriding static method: {}::{}",
                    class.name, method.name
                ),
            ));
        }
    }
    if let Some(parent_sig) = state.static_sigs.get(&method_key) {
        validate_override_signature(
            checker,
            class,
            method,
            parent_sig,
            state
                .late_static_static_method_returns
                .get(&method_key),
            true,
        )?;
    } else if has_override_attribute(method)
        && !interface_declares_method(checker, state, class, &method_key, true)
    {
        return Err(missing_override_target(class, method));
    }
    if method.is_abstract && state.static_method_impl_classes.contains_key(&method_key) {
        return Err(CompileError::new(
            method.span,
            &format!(
                "Cannot make concrete static method abstract: {}::{}",
                class.name, method.name
            ),
        ));
    }
    state.static_sigs.insert(method_key.clone(), sig);
    if let Some(return_type) = method
        .return_type
        .as_ref()
        .filter(|return_type| return_type.contains_late_static())
    {
        state
            .late_static_static_method_returns
            .insert(method_key.clone(), return_type.clone());
    } else {
        state
            .late_static_static_method_returns
            .remove(&method_key);
    }
    state
        .static_method_visibilities
        .insert(method_key.clone(), method.visibility.clone());
    if method.is_final {
        state.final_static_methods.insert(method_key.clone());
    } else {
        state.final_static_methods.remove(&method_key);
    }
    state
        .static_method_declaring_classes
        .insert(method_key.clone(), class.name.clone());
    state
        .method_attribute_names
        .insert(method_key.clone(), collect_attribute_names(&method.attributes));
    state
        .method_attribute_args
        .insert(method_key.clone(), collect_attribute_args(&method.attributes));
    if method.is_abstract {
        state.static_method_impl_classes.remove(&method_key);
    } else {
        state
            .static_method_impl_classes
            .insert(method_key.clone(), class.name.clone());
    }
    if method.visibility != Visibility::Private
        && !state.static_vtable_slots.contains_key(&method_key)
    {
        let slot = state.static_vtable_methods.len();
        state.static_vtable_slots.insert(method_key.clone(), slot);
        state.static_vtable_methods.push(method_key);
    }
    Ok(())
}

/// Validates and registers an instance method into `ClassBuildState`. Checks for
/// final method conflicts, static/instance kind conflicts, visibility reduction,
/// signature override compatibility, `#[Override]` attribute targets, and registers
/// the method in the instance vtable when visibility is non-private.
fn apply_instance_method(
    state: &mut ClassBuildState,
    class: &FlattenedClass,
    checker: &Checker,
    method: &ClassMethod,
) -> Result<(), CompileError> {
    let method_key = php_symbol_key(&method.name);
    let sig = build_method_sig(checker, method, &class.name)?;
    if state.final_static_methods.contains(&method_key) {
        return Err(final_method_error(
            state
                .static_method_declaring_classes
                .get(&method_key)
                .cloned()
                .unwrap_or_else(|| class.name.clone()),
            method,
        ));
    }
    if state.static_sigs.contains_key(&method_key) {
        return Err(method_kind_error(class, method));
    }
    if state.final_methods.contains(&method_key) {
        return Err(final_method_error(
            state
                .method_declaring_classes
                .get(&method_key)
                .cloned()
                .unwrap_or_else(|| class.name.clone()),
            method,
        ));
    }
    if let Some(parent_visibility) = state.method_visibilities.get(&method_key) {
        if visibility_rank(&method.visibility) < visibility_rank(parent_visibility) {
            return Err(CompileError::new(
                method.span,
                &format!(
                    "Cannot reduce visibility when overriding method: {}::{}",
                    class.name, method.name
                ),
            ));
        }
    }
    if let Some(parent_sig) = state.method_sigs.get(&method_key) {
        validate_override_signature(
            checker,
            class,
            method,
            parent_sig,
            state.late_static_method_returns.get(&method_key),
            false,
        )?;
    } else if has_override_attribute(method)
        && !interface_declares_method(checker, state, class, &method_key, false)
    {
        return Err(missing_override_target(class, method));
    }
    if method.is_abstract && state.method_impl_classes.contains_key(&method_key) {
        return Err(CompileError::new(
            method.span,
            &format!(
                "Cannot make concrete method abstract: {}::{}",
                class.name, method.name
            ),
        ));
    }
    state.method_sigs.insert(method_key.clone(), sig);
    if let Some(return_type) = method
        .return_type
        .as_ref()
        .filter(|return_type| return_type.contains_late_static())
    {
        state
            .late_static_method_returns
            .insert(method_key.clone(), return_type.clone());
    } else {
        state.late_static_method_returns.remove(&method_key);
    }
    state
        .method_visibilities
        .insert(method_key.clone(), method.visibility.clone());
    if method.is_final {
        state.final_methods.insert(method_key.clone());
    } else {
        state.final_methods.remove(&method_key);
    }
    state
        .method_declaring_classes
        .insert(method_key.clone(), class.name.clone());
    state
        .method_attribute_names
        .insert(method_key.clone(), collect_attribute_names(&method.attributes));
    state
        .method_attribute_args
        .insert(method_key.clone(), collect_attribute_args(&method.attributes));
    if method.is_abstract {
        state.method_impl_classes.remove(&method_key);
    } else {
        state
            .method_impl_classes
            .insert(method_key.clone(), class.name.clone());
    }
    if method.visibility != Visibility::Private && !state.vtable_slots.contains_key(&method_key) {
        let slot = state.vtable_methods.len();
        state.vtable_slots.insert(method_key.clone(), slot);
        state.vtable_methods.push(method_key);
    }
    Ok(())
}

/// Constructs a `CompileError` for overriding a `final` method.
fn final_method_error(declaring_class: String, method: &ClassMethod) -> CompileError {
    CompileError::new(
        method.span,
        &format!(
            "Cannot override final method {}::{}",
            declaring_class, method.name
        ),
    )
}

/// Constructs a `CompileError` for attempting to change a static method to
/// instance or vice versa when overriding a parent method.
fn method_kind_error(class: &FlattenedClass, method: &ClassMethod) -> CompileError {
    CompileError::new(
        method.span,
        &format!(
            "Cannot change method kind when overriding {}::{}",
            class.name, method.name
        ),
    )
}

/// Returns `true` if the method carries the PHP 8.3 `#[\Override]` marker
/// attribute (in any group, in either qualified form). Match is case-insensitive
/// to mirror PHP's class-name lookup rules.
fn has_override_attribute(method: &ClassMethod) -> bool {
    method.attributes.iter().any(|group| {
        group
            .attributes
            .iter()
            .any(|attr| matches_global_builtin_attribute(attr, "Override"))
    })
}

/// Returns `true` if any interface implemented by the class (directly or
/// transitively via parent interfaces or inherited parent-class contracts)
/// declares the method with the requested static/instance kind.
///
/// Seeds from `class.implements` because `apply_methods` runs before
/// `collect_interfaces` has added the class's own clause to `state.interfaces`;
/// also scans `state.interfaces`, which already carries interfaces inherited
/// from the parent class chain.
fn interface_declares_method(
    checker: &Checker,
    state: &ClassBuildState,
    class: &FlattenedClass,
    method_key: &str,
    is_static: bool,
) -> bool {
    let mut visited = std::collections::HashSet::new();
    let mut queue: Vec<String> = class.implements.clone();
    queue.extend(state.interfaces.iter().cloned());
    while let Some(name) = queue.pop() {
        if !visited.insert(name.clone()) {
            continue;
        }
        let Some(info) = checker.interfaces.get(&name) else {
            continue;
        };
        let declares_method = if is_static {
            info.static_methods.contains_key(method_key)
        } else {
            info.methods.contains_key(method_key)
        };
        if declares_method {
            return true;
        }
        queue.extend(info.parents.iter().cloned());
    }
    false
}

/// Constructs a `CompileError` when a method carries `#[\Override]` but no
/// matching parent method exists to override.
fn missing_override_target(class: &FlattenedClass, method: &ClassMethod) -> CompileError {
    CompileError::new(
        method.span,
        &format!(
            "{}::{}() has #[\\Override] attribute, but no matching parent method was found",
            class.name, method.name
        ),
    )
}
