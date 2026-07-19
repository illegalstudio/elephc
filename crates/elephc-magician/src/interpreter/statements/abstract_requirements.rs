//! Purpose:
//! Collects, merges, and applies inherited abstract method and property requirements.
//!
//! Called from:
//! - Concrete class validation after direct member compatibility checks.
//!
//! Key details:
//! - Eval and AOT parent contracts retain visibility, staticness, and property hook modes.

use super::*;

/// Validates that a concrete class has satisfied inherited abstract and interface requirements.
pub(super) fn validate_concrete_class_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    if !pending_class_abstract_method_requirements(class, context).is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    if !pending_class_abstract_property_requirements(class, context)?.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    for interface in pending_class_interface_names(class, context) {
        if context.has_interface(&interface) {
            validate_class_implements_eval_interface(class, &interface, context)?;
        }
    }
    Ok(())
}

/// Validates concrete class methods required by PHP builtin runtime interfaces.
pub(super) fn validate_concrete_class_builtin_interface_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Result<(), EvalStatus> {
    for (requirement_owner, requirement) in
        pending_class_builtin_interface_method_requirements(class, context)
    {
        if !class_has_builtin_interface_method(class, &requirement_owner, &requirement, context) {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Validates concrete class methods required by generated/AOT abstract parents.
pub(super) fn validate_concrete_class_aot_parent_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !pending_class_aot_parent_abstract_method_requirements(class, context, values)?.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    if !pending_class_aot_parent_abstract_property_requirements(class, context, values)?.is_empty()
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(())
}

/// Validates concrete class methods required by generated/AOT runtime interfaces.
pub(super) fn validate_concrete_class_aot_interface_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for requirement in pending_class_aot_interface_method_requirements(class, context, values)? {
        if !class_has_aot_interface_method(class, &requirement, context) {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    for (requirement_owner, requirement) in
        pending_class_aot_interface_property_requirements(class, context, values)?
    {
        if !class_has_interface_property(class, &requirement_owner, &requirement, context) {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Returns inherited abstract methods that the pending class has not concretized.
pub(super) fn pending_class_abstract_method_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Vec<EvalClassMethod> {
    let mut requirements = std::collections::HashMap::new();
    if let Some(parent) = class.parent() {
        collect_class_abstract_method_requirements(parent, context, &mut requirements);
    }
    apply_class_abstract_method_requirements(class, &mut requirements);
    requirements.into_values().collect()
}

/// Returns inherited abstract properties that the pending class has not concretized.
pub(super) fn pending_class_abstract_property_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
) -> Result<Vec<EvalClassProperty>, EvalStatus> {
    let mut requirements = std::collections::HashMap::new();
    if let Some(parent) = class.parent() {
        collect_class_abstract_property_requirements(parent, context, &mut requirements)?;
    }
    apply_class_abstract_property_requirements(class, &mut requirements)?;
    Ok(requirements.into_values().collect())
}

/// Returns generated/AOT abstract parent methods the pending class has not concretized.
pub(super) fn pending_class_aot_parent_abstract_method_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvalAotAbstractMethodRequirement>, EvalStatus> {
    let mut requirements = std::collections::HashMap::new();
    if let Some(parent) = class.parent() {
        collect_aot_parent_abstract_method_requirements(
            parent,
            context,
            values,
            &mut requirements,
        )?;
    }
    apply_class_aot_parent_abstract_method_requirements(class, context, &mut requirements)?;
    Ok(requirements.into_values().collect())
}

/// Returns generated/AOT abstract parent properties the pending class has not concretized.
pub(super) fn pending_class_aot_parent_abstract_property_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvalAotAbstractPropertyRequirement>, EvalStatus> {
    let mut requirements = std::collections::HashMap::new();
    if let Some(parent) = class.parent() {
        collect_aot_parent_abstract_property_requirements(
            parent,
            context,
            values,
            &mut requirements,
        )?;
    }
    apply_class_aot_parent_abstract_property_requirements(class, context, &mut requirements)?;
    Ok(requirements.into_values().collect())
}

/// Collects abstract method requirements from one declared eval class ancestry chain.
pub(super) fn collect_class_abstract_method_requirements(
    class_name: &str,
    context: &ElephcEvalContext,
    requirements: &mut std::collections::HashMap<String, EvalClassMethod>,
) {
    let Some(class) = context.class(class_name) else {
        return;
    };
    if let Some(parent) = class.parent() {
        collect_class_abstract_method_requirements(parent, context, requirements);
    }
    apply_class_abstract_method_requirements(class, requirements);
}

/// Collects generated/AOT abstract method requirements through eval and AOT parents.
pub(super) fn collect_aot_parent_abstract_method_requirements(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    requirements: &mut std::collections::HashMap<String, EvalAotAbstractMethodRequirement>,
) -> Result<(), EvalStatus> {
    let class_name = class_name.trim_start_matches('\\');
    if let Some(class) = context.class(class_name) {
        if let Some(parent) = class.parent() {
            collect_aot_parent_abstract_method_requirements(
                parent,
                context,
                values,
                requirements,
            )?;
        }
        apply_class_aot_parent_abstract_method_requirements(class, context, requirements)?;
        return Ok(());
    }
    if values.class_exists(class_name)? {
        collect_native_aot_abstract_method_requirements(
            class_name,
            context,
            values,
            requirements,
        )?;
    }
    Ok(())
}

/// Collects abstract methods exposed by one generated/AOT class reflection row.
pub(super) fn collect_native_aot_abstract_method_requirements(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    requirements: &mut std::collections::HashMap<String, EvalAotAbstractMethodRequirement>,
) -> Result<(), EvalStatus> {
    for method_name in eval_aot_method_names(class_name, values)? {
        let Some(flags) = values.reflection_method_flags(class_name, &method_name)? else {
            continue;
        };
        let key = method_name.to_ascii_lowercase();
        if flags & EVAL_REFLECTION_MEMBER_FLAG_ABSTRACT == 0 {
            requirements.remove(&key);
            continue;
        }
        if flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE != 0 {
            continue;
        }
        let requirement =
            eval_aot_abstract_method_requirement(class_name, &method_name, flags, context, values)?;
        requirements.insert(key, requirement);
    }
    Ok(())
}

/// Collects generated/AOT abstract property requirements through eval and AOT parents.
pub(super) fn collect_aot_parent_abstract_property_requirements(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    requirements: &mut std::collections::HashMap<String, EvalAotAbstractPropertyRequirement>,
) -> Result<(), EvalStatus> {
    let class_name = class_name.trim_start_matches('\\');
    if let Some(class) = context.class(class_name) {
        if let Some(parent) = class.parent() {
            collect_aot_parent_abstract_property_requirements(
                parent,
                context,
                values,
                requirements,
            )?;
        }
        apply_class_aot_parent_abstract_property_requirements(class, context, requirements)?;
        return Ok(());
    }
    if values.class_exists(class_name)? {
        collect_native_aot_abstract_property_requirements(
            class_name,
            context,
            values,
            requirements,
        )?;
    }
    Ok(())
}

/// Collects abstract properties exposed by one generated/AOT class metadata row.
pub(super) fn collect_native_aot_abstract_property_requirements(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    requirements: &mut std::collections::HashMap<String, EvalAotAbstractPropertyRequirement>,
) -> Result<(), EvalStatus> {
    for (owner, property) in context.native_abstract_property_requirements(class_name) {
        let Some(flags) = values.reflection_property_flags(class_name, property.name())? else {
            continue;
        };
        if flags & EVAL_REFLECTION_MEMBER_FLAG_ABSTRACT == 0
            || flags & EVAL_REFLECTION_MEMBER_FLAG_PRIVATE != 0
        {
            continue;
        }
        let visibility = eval_aot_property_visibility(flags);
        let write_visibility = eval_aot_property_write_visibility(flags, visibility);
        let set_visibility = (write_visibility != visibility).then_some(write_visibility);
        let requirement = EvalClassProperty::with_visibility(property.name(), visibility, None)
            .with_type(property.property_type().cloned())
            .with_set_visibility(set_visibility)
            .with_abstract_hook_contract(property.requires_get(), property.requires_set());
        requirements.insert(
            property.name().to_string(),
            EvalAotAbstractPropertyRequirement {
                owner,
                property: requirement,
            },
        );
    }
    Ok(())
}

/// Collects abstract property requirements from one declared eval class ancestry chain.
pub(super) fn collect_class_abstract_property_requirements(
    class_name: &str,
    context: &ElephcEvalContext,
    requirements: &mut std::collections::HashMap<String, EvalClassProperty>,
) -> Result<(), EvalStatus> {
    let Some(class) = context.class(class_name) else {
        return Ok(());
    };
    if let Some(parent) = class.parent() {
        collect_class_abstract_property_requirements(parent, context, requirements)?;
    }
    apply_class_abstract_property_requirements(class, requirements)
}

/// Applies one class's methods to the open abstract-method requirement set.
pub(super) fn apply_class_abstract_method_requirements(
    class: &EvalClass,
    requirements: &mut std::collections::HashMap<String, EvalClassMethod>,
) {
    for method in class.methods() {
        let key = method.name().to_ascii_lowercase();
        if method.is_abstract() {
            requirements.insert(key, method.clone());
        } else {
            requirements.remove(&key);
        }
    }
}

/// Applies one eval class's methods to the open AOT abstract-method requirement set.
pub(super) fn apply_class_aot_parent_abstract_method_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
    requirements: &mut std::collections::HashMap<String, EvalAotAbstractMethodRequirement>,
) -> Result<(), EvalStatus> {
    for method in class.methods() {
        let key = method.name().to_ascii_lowercase();
        let Some(requirement) = requirements.get(&key) else {
            continue;
        };
        if !class_method_satisfies_aot_abstract_parent_requirement(
            method,
            class.name(),
            requirement,
            Some(class),
            context,
            false,
        ) {
            return Err(EvalStatus::RuntimeFatal);
        }
        if !method.is_abstract() {
            requirements.remove(&key);
        }
    }
    Ok(())
}

/// Applies one eval class's properties to the open AOT abstract-property requirement set.
pub(super) fn apply_class_aot_parent_abstract_property_requirements(
    class: &EvalClass,
    context: &ElephcEvalContext,
    requirements: &mut std::collections::HashMap<String, EvalAotAbstractPropertyRequirement>,
) -> Result<(), EvalStatus> {
    for property in class.properties() {
        let key = property.name().to_string();
        let Some(requirement) = requirements.get(&key).map(|requirement| {
            EvalAotAbstractPropertyRequirement {
                owner: requirement.owner.clone(),
                property: requirement.property.clone(),
            }
        }) else {
            continue;
        };
        if !class_property_satisfies_aot_abstract_parent_requirement(
            property,
            class.name(),
            &requirement,
            Some(class),
            context,
        ) {
            return Err(EvalStatus::RuntimeFatal);
        }
        if property.is_abstract() {
            requirements.insert(
                key,
                EvalAotAbstractPropertyRequirement {
                    owner: class.name().to_string(),
                    property: merge_abstract_property_contracts(
                        &requirement.property,
                        property,
                    ),
                },
            );
        } else {
            requirements.remove(&key);
        }
    }
    Ok(())
}

/// Applies one class's properties to the open abstract-property requirement set.
pub(super) fn apply_class_abstract_property_requirements(
    class: &EvalClass,
    requirements: &mut std::collections::HashMap<String, EvalClassProperty>,
) -> Result<(), EvalStatus> {
    for property in class.properties() {
        let key = property.name().to_string();
        if property.is_abstract() {
            if let Some(existing) = requirements.get(&key) {
                (property_contract_visibility_allows(existing, property)
                    && property_contract_write_visibility_allows(existing, property))
                .then_some(())
                .ok_or(EvalStatus::RuntimeFatal)?;
                requirements.insert(key, merge_abstract_property_contracts(existing, property));
            } else {
                requirements.insert(key, property.clone());
            }
        } else if let Some(requirement) = requirements.get(&key) {
            class_property_satisfies_abstract_contract(property, requirement)
                .then_some(())
                .ok_or(EvalStatus::RuntimeFatal)?;
            requirements.remove(&key);
        }
    }
    Ok(())
}

/// Merges inherited and redeclared abstract property hook requirements.
pub(super) fn merge_abstract_property_contracts(
    inherited: &EvalClassProperty,
    redeclared: &EvalClassProperty,
) -> EvalClassProperty {
    redeclared.clone().with_abstract_hook_contract(
        inherited.requires_get_hook() || redeclared.requires_get_hook(),
        inherited.requires_set_hook() || redeclared.requires_set_hook(),
    )
}

/// Returns whether a redeclared property keeps compatible visibility.
pub(super) fn property_contract_visibility_allows(
    inherited: &EvalClassProperty,
    redeclared: &EvalClassProperty,
) -> bool {
    property_visibility_rank(redeclared.visibility())
        >= property_visibility_rank(inherited.visibility())
}

/// Returns whether a redeclared property keeps compatible write visibility.
pub(super) fn property_contract_write_visibility_allows(
    inherited: &EvalClassProperty,
    redeclared: &EvalClassProperty,
) -> bool {
    !inherited.requires_set_hook()
        || property_visibility_rank(redeclared.write_visibility())
            >= property_visibility_rank(inherited.write_visibility())
}

/// Returns whether a concrete property satisfies an abstract hook contract.
pub(super) fn class_property_satisfies_abstract_contract(
    property: &EvalClassProperty,
    requirement: &EvalClassProperty,
) -> bool {
    if property.is_abstract()
        || property.is_static()
        || property.property_type() != requirement.property_type()
        || !property_contract_visibility_allows(requirement, property)
    {
        return false;
    }
    if requirement.requires_set_hook() {
        return requirement.set_visibility() != Some(EvalVisibility::Private)
            && property_contract_write_visibility_allows(requirement, property)
            && (property.has_set_hook() || (!property.has_get_hook() && !property.is_readonly()));
    }
    requirement.requires_get_hook()
}

/// Returns whether one property satisfies a generated/AOT abstract parent contract.
pub(super) fn class_property_satisfies_aot_abstract_parent_requirement(
    property: &EvalClassProperty,
    property_owner: &str,
    requirement: &EvalAotAbstractPropertyRequirement,
    pending_class: Option<&EvalClass>,
    context: &ElephcEvalContext,
) -> bool {
    let required = &requirement.property;
    if property.is_static() != required.is_static()
        || !property_contract_visibility_allows(required, property)
        || !property_type_signature_matches(
            property.property_type(),
            property_owner,
            required.property_type(),
            &requirement.owner,
            pending_class,
            context,
        )
    {
        return false;
    }
    if property.is_abstract() {
        return (!required.requires_get_hook() || property.requires_get_hook())
            && (!required.requires_set_hook()
                || (property.requires_set_hook()
                    && property_contract_write_visibility_allows(required, property)));
    }
    if required.requires_get_hook() && !class_property_supports_interface_get(property) {
        return false;
    }
    if required.requires_set_hook() {
        return required.set_visibility() != Some(EvalVisibility::Private)
            && property_contract_write_visibility_allows(required, property)
            && class_property_supports_interface_set(property);
    }
    required.requires_get_hook()
}

/// Returns a comparable rank where larger means less restrictive property visibility.
pub(super) fn property_visibility_rank(visibility: EvalVisibility) -> u8 {
    match visibility {
        EvalVisibility::Private => 1,
        EvalVisibility::Protected => 2,
        EvalVisibility::Public => 3,
    }
}
