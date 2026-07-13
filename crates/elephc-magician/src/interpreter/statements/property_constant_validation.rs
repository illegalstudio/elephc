//! Purpose:
//! Validates property and class-constant declarations against inherited contracts.
//!
//! Called from:
//! - Eval class and interface declaration validation.
//!
//! Key details:
//! - Readonly, asymmetric visibility, hook parameter types, and AOT redeclarations are checked here.

use super::*;

/// Validates property declarations that can be checked before class registration.
pub(super) fn validate_eval_declared_properties(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    let mut names = std::collections::HashSet::new();
    for property in class.properties() {
        validate_eval_non_method_attribute_targets(property.attributes())?;
        if !names.insert(property.name().to_string()) {
            return Err(EvalStatus::RuntimeFatal);
        }
        if property.is_abstract()
            && (!class.is_abstract()
                || property.is_static()
                || property.is_final()
                || property.is_readonly()
                || property.default().is_some()
                || (!property.requires_get_hook() && !property.requires_set_hook()))
        {
            return Err(EvalStatus::RuntimeFatal);
        }
        if property.is_static() && property.is_readonly() {
            return Err(EvalStatus::RuntimeFatal);
        }
        if let Some(set_visibility) = property.set_visibility() {
            if property.is_static() || property.property_type().is_none() {
                return Err(EvalStatus::RuntimeFatal);
            }
            if property_visibility_rank(set_visibility)
                > property_visibility_rank(property.visibility())
            {
                return Err(EvalStatus::RuntimeFatal);
            }
        }
        if property.is_final() && property.visibility() == EvalVisibility::Private {
            return Err(EvalStatus::RuntimeFatal);
        }
        if property.is_readonly() && property.property_type().is_none() {
            return Err(EvalStatus::RuntimeFatal);
        }
        if property.is_readonly() && property.default().is_some() {
            return Err(EvalStatus::RuntimeFatal);
        }
        if (property.has_get_hook() || property.has_set_hook())
            && (property.is_static() || property.is_readonly() || property.default().is_some())
        {
            return Err(EvalStatus::RuntimeFatal);
        }
        validate_eval_property_set_hook_parameter_type(class, property, context)?;
    }
    Ok(())
}

/// Validates that an explicit set-hook parameter type can accept every property value.
pub(super) fn validate_eval_property_set_hook_parameter_type(
    class: &EvalClass,
    property: &EvalClassProperty,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    let Some(set_hook_type) = property.set_hook_type() else {
        return Ok(());
    };
    let Some(property_type) = property.property_type() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let set_hook_types = vec![Some(set_hook_type.clone())];
    let property_types = vec![Some(property_type.clone())];
    method_parameter_type_signature_accepts(
        &set_hook_types,
        &[],
        class.name(),
        &property_types,
        &[],
        class.name(),
        1,
        Some(class),
        context,
    )
    .then_some(())
    .ok_or(EvalStatus::RuntimeFatal)
}

/// Validates one property declaration against inherited eval property metadata.
pub(super) fn validate_property_parent_redeclaration(
    class: &EvalClass,
    property: &EvalClassProperty,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(parent) = class.parent() else {
        return Ok(());
    };
    if let Some((parent_declaring_class, parent_property)) =
        context.class_property(parent, property.name())
    {
        if parent_property.visibility() == EvalVisibility::Private {
            return Ok(());
        }
        if parent_property.is_final()
            || parent_property.set_visibility() == Some(EvalVisibility::Private)
        {
            return Err(EvalStatus::RuntimeFatal);
        }
        if parent_property.is_static() != property.is_static()
            || (parent_property.is_readonly() && !property.is_readonly())
            || property_visibility_rank(property.visibility())
                < property_visibility_rank(parent_property.visibility())
            || property_visibility_rank(property.write_visibility())
                < property_visibility_rank(parent_property.write_visibility())
            || !property_type_signature_matches(
                property.property_type(),
                class.name(),
                parent_property.property_type(),
                &parent_declaring_class,
                Some(class),
                context,
            )
        {
            return Err(EvalStatus::RuntimeFatal);
        }
        return Ok(());
    }
    validate_property_aot_parent_redeclaration(parent, class, property, context, values)
}

/// Validates one property declaration against inherited generated/AOT property metadata.
pub(super) fn validate_property_aot_parent_redeclaration(
    parent: &str,
    class: &EvalClass,
    property: &EvalClassProperty,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if context.has_class(parent) || !values.class_exists(parent)? {
        return Ok(());
    }
    let parent = parent.trim_start_matches('\\');
    let Some(flags) = values.reflection_property_flags(parent, property.name())? else {
        return Ok(());
    };
    if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE != 0 {
        return Ok(());
    }
    if flags & EVAL_REFLECTION_MEMBER_FLAG_FINAL != 0
        || flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE_SET != 0
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    let parent_is_static = flags & EVAL_REFLECTION_MEMBER_FLAG_STATIC != 0;
    let parent_visibility = eval_aot_property_visibility(flags);
    let parent_write_visibility = eval_aot_property_write_visibility(flags, parent_visibility);
    let parent_is_readonly = flags & EVAL_REFLECTION_MEMBER_FLAG_READONLY != 0;
    let parent_declaring_class =
        eval_aot_property_declaring_class(parent, property.name(), values)?;
    let parent_property_type = context
        .native_property_type(&parent_declaring_class, property.name())
        .or_else(|| context.native_property_type(parent, property.name()));
    if parent_is_static != property.is_static()
        || (parent_is_readonly && !property.is_readonly())
        || property_visibility_rank(property.visibility())
            < property_visibility_rank(parent_visibility)
        || property_visibility_rank(property.write_visibility())
            < property_visibility_rank(parent_write_visibility)
        || !property_type_signature_matches(
            property.property_type(),
            class.name(),
            parent_property_type.as_ref(),
            &parent_declaring_class,
            Some(class),
            context,
        )
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(())
}

/// Returns the eval visibility represented by generated/AOT property reflection flags.
pub(super) fn eval_aot_property_visibility(flags: u64) -> EvalVisibility {
    if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE != 0 {
        EvalVisibility::Private
    } else if flags & EVAL_REFLECTION_MEMBER_FLAG_PROTECTED != 0 {
        EvalVisibility::Protected
    } else {
        EvalVisibility::Public
    }
}

/// Returns the eval write visibility represented by generated/AOT property flags.
pub(super) fn eval_aot_property_write_visibility(
    flags: u64,
    read_visibility: EvalVisibility,
) -> EvalVisibility {
    if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE_SET != 0 {
        EvalVisibility::Private
    } else if flags & EVAL_REFLECTION_MEMBER_FLAG_PROTECTED_SET != 0 {
        EvalVisibility::Protected
    } else {
        read_visibility
    }
}

/// Returns the generated/AOT declaring class for one reflected property.
pub(super) fn eval_aot_property_declaring_class(
    class_name: &str,
    property_name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    values
        .reflection_property_declaring_class(class_name, property_name)
        .map(|declaring_class| declaring_class.unwrap_or_else(|| class_name.to_string()))
}

/// Validates constant declarations that can be checked before registration.
pub(super) fn validate_eval_declared_constants(constants: &[EvalClassConstant]) -> Result<(), EvalStatus> {
    let mut names = std::collections::HashSet::new();
    for constant in constants {
        validate_eval_non_method_attribute_targets(constant.attributes())?;
        if !names.insert(constant.name().to_string()) {
            return Err(EvalStatus::RuntimeFatal);
        }
        if constant.is_final() && constant.visibility() == EvalVisibility::Private {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Validates declarations that are specific to PHP interface constants.
pub(super) fn validate_eval_interface_constants(constants: &[EvalClassConstant]) -> Result<(), EvalStatus> {
    for constant in constants {
        if constant.visibility() != EvalVisibility::Public {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Validates interface constants against inherited parent-interface constants.
pub(super) fn validate_interface_constant_parent_redeclarations(
    interface: &EvalInterface,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for constant in interface.constants() {
        for parent in interface.parents() {
            if let Some((_, parent_constant)) = context.interface_constant(parent, constant.name())
            {
                if parent_constant.is_final() {
                    return Err(EvalStatus::RuntimeFatal);
                }
            }
            validate_aot_interface_constant_redeclaration(parent, constant, values)?;
        }
    }
    Ok(())
}

/// Validates one constant declaration against inherited eval constant metadata.
pub(super) fn validate_constant_parent_redeclaration(
    class: &EvalClass,
    constant: &EvalClassConstant,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if let Some(parent) = class.parent() {
        if let Some((_, parent_constant)) = context.class_constant(parent, constant.name()) {
            if parent_constant.visibility() != EvalVisibility::Private
                && (parent_constant.is_final()
                    || constant_visibility_rank(constant.visibility())
                        < constant_visibility_rank(parent_constant.visibility()))
            {
                return Err(EvalStatus::RuntimeFatal);
            }
        }
        validate_aot_class_constant_redeclaration(parent, constant, values)?;
    }
    for interface in pending_class_interface_names(class, context) {
        if let Some((_, interface_constant)) =
            context.interface_constant(&interface, constant.name())
        {
            if interface_constant.is_final()
                || constant_visibility_rank(constant.visibility())
                    < constant_visibility_rank(interface_constant.visibility())
            {
                return Err(EvalStatus::RuntimeFatal);
            }
        }
        validate_aot_interface_constant_redeclaration(&interface, constant, values)?;
    }
    Ok(())
}

/// Validates a class constant redeclaration against a generated/AOT parent class constant.
pub(super) fn validate_aot_class_constant_redeclaration(
    parent: &str,
    constant: &EvalClassConstant,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !values.class_exists(parent)? {
        return Ok(());
    }
    validate_aot_constant_redeclaration(parent, constant, false, values)
}

/// Validates a class/interface constant redeclaration against a generated/AOT interface constant.
pub(super) fn validate_aot_interface_constant_redeclaration(
    interface: &str,
    constant: &EvalClassConstant,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !values.interface_exists(interface)? {
        return Ok(());
    }
    validate_aot_constant_redeclaration(interface, constant, true, values)
}

/// Applies PHP redeclaration rules to one generated/AOT constant metadata row.
pub(super) fn validate_aot_constant_redeclaration(
    class_like: &str,
    constant: &EvalClassConstant,
    interface_context: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let class_like = class_like.trim_start_matches('\\');
    let Some(flags) = values.reflection_constant_flags(class_like, constant.name())? else {
        return Ok(());
    };
    let inherited_visibility = eval_aot_constant_visibility(flags);
    if !interface_context && inherited_visibility == EvalVisibility::Private {
        return Ok(());
    }
    if flags & EVAL_REFLECTION_MEMBER_FLAG_FINAL != 0
        || constant_visibility_rank(constant.visibility())
            < constant_visibility_rank(inherited_visibility)
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(())
}

/// Returns the eval visibility represented by generated/AOT constant reflection flags.
pub(super) fn eval_aot_constant_visibility(flags: u64) -> EvalVisibility {
    if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE != 0 {
        EvalVisibility::Private
    } else if flags & EVAL_REFLECTION_MEMBER_FLAG_PROTECTED != 0 {
        EvalVisibility::Protected
    } else {
        EvalVisibility::Public
    }
}

/// Returns a comparable rank where larger means less restrictive constant visibility.
pub(super) fn constant_visibility_rank(visibility: EvalVisibility) -> u8 {
    match visibility {
        EvalVisibility::Private => 1,
        EvalVisibility::Protected => 2,
        EvalVisibility::Public => 3,
    }
}
