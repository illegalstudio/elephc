//! Purpose:
//! Registers eval class declarations and validates their direct parent.
//!
//! Called from:
//! - Statement dispatch for class and anonymous-class declarations.
//!
//! Key details:
//! - Registration occurs only after trait expansion and declaration validation succeed.

use super::*;

/// Registers an eval-declared class in the dynamic class table.
pub(in crate::interpreter) fn execute_class_decl_stmt(
    class: &EvalClass,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let name = class.name().trim_start_matches('\\');
    if context.has_class(name)
        || context.has_interface(name)
        || context.has_trait(name)
        || context.has_enum(name)
        || values.class_exists(name)?
        || eval_runtime_interface_exists(name, values)?
        || values.trait_exists(name)?
        || values.enum_exists(name)?
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    let class = expand_eval_class_traits(class, context)?.with_readonly_properties();
    let class = &class;
    validate_eval_class_modifiers(class, context, values)?;
    let native_parent = validate_eval_class_parent(class, context, values)?;
    for interface in class.interfaces() {
        if !context.has_interface(interface) && !eval_runtime_interface_exists(interface, values)? {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    validate_eval_class_does_not_implement_throwable_interfaces(class, context)?;
    validate_eval_class_does_not_implement_enum_interfaces(class, context)?;
    validate_declared_class_interface_members(class, context)?;
    validate_declared_class_builtin_interface_members(class, context)?;
    validate_declared_class_aot_interface_members(class, context, values)?;
    if !class.is_abstract() {
        validate_concrete_class_requirements(class, context)?;
        validate_concrete_class_builtin_interface_requirements(class, context)?;
        validate_concrete_class_aot_parent_requirements(class, context, values)?;
        validate_concrete_class_aot_interface_requirements(class, context, values)?;
    }
    if context.define_class(class.clone()) {
        if let Some(parent) = native_parent.as_deref() {
            if !context.define_native_class_parent(class.name(), parent) {
                return Err(EvalStatus::RuntimeFatal);
            }
        }
        initialize_eval_declared_constants(
            class.name(),
            class.constants(),
            context,
            scope,
            values,
        )?;
        initialize_eval_static_properties(class, context, scope, values)
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Validates an eval class parent and returns an AOT parent name when the parent is runtime-backed.
pub(super) fn validate_eval_class_parent(
    class: &EvalClass,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<String>, EvalStatus> {
    let Some(parent) = class.parent() else {
        return Ok(None);
    };
    let parent = context
        .resolve_class_name(parent)
        .unwrap_or_else(|| parent.trim_start_matches('\\').to_string());
    if let Some(parent_class) = context.class(&parent) {
        if parent_class.is_final()
            || parent_class.is_readonly_class() != class.is_readonly_class()
            || context.class_is_a(&parent, class.name(), false)
        {
            return Err(EvalStatus::RuntimeFatal);
        }
        return Ok(None);
    }
    let Some((parent_is_final, parent_is_readonly)) =
        eval_reflection_aot_class_inheritance_modifiers(&parent, values)?
    else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if parent_is_final
        || parent_is_readonly != class.is_readonly_class()
        || native_class_is_a(&parent, class.name(), context)
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(Some(parent))
}

/// Registers one eval anonymous class expression if this execution has not seen it yet.
pub(in crate::interpreter) fn ensure_eval_anonymous_class_decl(
    class: &EvalClass,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !class.is_anonymous() {
        return Err(EvalStatus::RuntimeFatal);
    }
    if let Some(existing) = context.class(class.name()) {
        return if existing.is_anonymous() {
            Ok(())
        } else {
            Err(EvalStatus::RuntimeFatal)
        };
    }
    execute_class_decl_stmt(class, context, scope, values)
}
