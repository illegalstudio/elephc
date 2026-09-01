//! Purpose:
//! Visible methods, properties, constants, defaults, and enum members.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;

/// Returns PHP case-insensitive method names visible to `ReflectionClass::hasMethod()`.
pub(super) fn reflection_class_method_names(ctx: &FunctionContext<'_>, class_name: &str) -> Vec<String> {
    if let Some(method_names) = crate::types::php_src_date_method_names(class_name) {
        return method_names
            .iter()
            .map(|method_name| (*method_name).to_string())
            .collect();
    }
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut current = Some(class_name.to_string());
    while let Some(current_name) = current {
        let Some((resolved_name, info)) = resolve_reflection_class(ctx, &current_name) else {
            break;
        };
        push_unique_method_names(info.methods.keys(), &mut names, &mut seen);
        push_unique_method_names(info.static_methods.keys(), &mut names, &mut seen);
        current = info.parent.clone();
        if current.as_deref() == Some(resolved_name) {
            break;
        }
    }
    names
}

/// Returns PHP case-sensitive property names visible to `ReflectionClass::hasProperty()`.
pub(super) fn reflection_class_property_names(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    info: &crate::types::ClassInfo,
) -> Vec<String> {
    if let Some(property_names) = crate::types::php_src_date_property_names(class_name) {
        return property_names
            .iter()
            .map(|name| (*name).to_string())
            .collect();
    }
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if is_reflection_enum(ctx, class_name) {
        push_unique_property_name("name", &mut names, &mut seen);
    }
    for (name, _) in &info.properties {
        if reflection_property_visible_from_class(info, class_name, name, false) {
            push_unique_property_name(name, &mut names, &mut seen);
        }
    }
    for (name, _) in &info.static_properties {
        if reflection_property_visible_from_class(info, class_name, name, true) {
            push_unique_property_name(name, &mut names, &mut seen);
        }
    }
    names
}

/// Returns PHP case-sensitive class constant names visible to `ReflectionClass::hasConstant()`.
pub(super) fn reflection_class_constant_names(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    _info: &crate::types::ClassInfo,
) -> Vec<String> {
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(enum_info) = ctx.module.enum_infos.get(class_name) {
        for case in &enum_info.cases {
            push_unique_constant_name(&case.name, &mut names, &mut seen);
        }
    }
    let mut current = Some(class_name.to_string());
    while let Some(current_name) = current {
        let Some((resolved_name, current_info)) = resolve_reflection_class(ctx, &current_name)
        else {
            break;
        };
        for constant in current_info.constants.keys() {
            push_unique_constant_name(constant, &mut names, &mut seen);
        }
        for interface_name in &current_info.interfaces {
            for constant in reflection_interface_constant_names(ctx, interface_name) {
                push_unique_constant_name(&constant, &mut names, &mut seen);
            }
        }
        current = current_info.parent.clone();
        if current.as_deref() == Some(resolved_name) {
            break;
        }
    }
    names
}

/// Returns materializable class constant values for `ReflectionClass::getConstants()`.
pub(super) fn reflection_class_constant_members(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    _info: &crate::types::ClassInfo,
) -> Result<Vec<ReflectionConstantMember>> {
    let mut members = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(enum_info) = ctx.module.enum_infos.get(class_name) {
        for case in &enum_info.cases {
            push_unique_constant_member(
                &case.name,
                ReflectionConstantValue::EnumCase {
                    enum_name: class_name.to_string(),
                    case_name: case.name.clone(),
                },
                &mut members,
                &mut seen,
            );
        }
    }
    let mut current = Some(class_name.to_string());
    while let Some(current_name) = current {
        let Some((resolved_name, current_info)) = resolve_reflection_class(ctx, &current_name)
        else {
            break;
        };
        for (constant_name, value_expr) in &current_info.constants {
            if seen.contains(constant_name) {
                continue;
            }
            let value =
                reflection_constant_value(ctx, resolved_name, Some(current_info), value_expr, 0)?;
            push_unique_constant_member(constant_name, value, &mut members, &mut seen);
        }
        for interface_name in &current_info.interfaces {
            for member in reflection_interface_constant_members(ctx, interface_name)? {
                push_unique_constant_member(&member.name, member.value, &mut members, &mut seen);
            }
        }
        current = current_info.parent.clone();
        if current.as_deref() == Some(resolved_name) {
            break;
        }
    }
    Ok(members)
}

/// Returns materializable property defaults for `ReflectionClass::getDefaultProperties()`.
pub(super) fn reflection_class_default_property_members(
    info: &crate::types::ClassInfo,
    property_names: &[String],
) -> Vec<ReflectionDefaultPropertyMember> {
    property_names
        .iter()
        .filter_map(|property_name| {
            reflection_property_default_value(info, property_name).map(|value| {
                ReflectionDefaultPropertyMember {
                    name: property_name.clone(),
                    value,
                }
            })
        })
        .collect()
}

/// Returns static-property storage slots for `ReflectionClass::getStaticProperties()`.
pub(super) fn reflection_class_static_property_members(
    class_name: &str,
    info: &crate::types::ClassInfo,
) -> Vec<ReflectionStaticPropertyMember> {
    info.static_properties
        .iter()
        .map(|(property_name, php_type)| {
            let declaring_class_name = info
                .static_property_declaring_classes
                .get(property_name)
                .cloned()
                .unwrap_or_else(|| class_name.to_string());
            ReflectionStaticPropertyMember {
                name: property_name.clone(),
                declaring_class_name,
                php_type: php_type.clone(),
                is_declared: info.declared_static_properties.contains(property_name),
            }
        })
        .collect()
}

/// Returns materializable interface constant values for ReflectionClass metadata.
pub(super) fn reflection_interface_constant_members(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
) -> Result<Vec<ReflectionConstantMember>> {
    let mut members = Vec::new();
    let mut seen = std::collections::HashSet::new();
    collect_interface_constant_members(ctx, interface_name, &mut members, &mut seen)?;
    Ok(members)
}

/// Appends flattened interface constants while preserving their declaring interface.
pub(super) fn collect_interface_constant_members(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    members: &mut Vec<ReflectionConstantMember>,
    seen: &mut std::collections::HashSet<String>,
) -> Result<()> {
    let Some(interface_info) = ctx.module.interface_infos.get(interface_name) else {
        return Ok(());
    };
    for (constant_name, value_expr) in &interface_info.constants {
        if seen.contains(constant_name) {
            continue;
        }
        let declaring_interface =
            interface_constant_declaring_interface(interface_info, interface_name, constant_name);
        let value = reflection_constant_value(ctx, declaring_interface, None, value_expr, 0)?;
        push_unique_constant_member(constant_name, value, members, seen);
    }
    Ok(())
}

/// Returns materializable direct trait constant values for ReflectionClass metadata.
pub(super) fn reflection_trait_constant_members(
    ctx: &FunctionContext<'_>,
    trait_name: &str,
) -> Result<Vec<ReflectionConstantMember>> {
    let mut members = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(constants) = ctx.module.declared_trait_constants.get(trait_name) {
        for (constant_name, value_expr) in constants {
            if seen.contains(constant_name) {
                continue;
            }
            let value = reflection_constant_value(ctx, trait_name, None, value_expr, 0)?;
            push_unique_constant_member(constant_name, value, &mut members, &mut seen);
        }
    }
    Ok(members)
}

/// Returns materializable constant-reflector objects for `ReflectionClass::getReflectionConstants()`.
pub(super) fn reflection_class_constant_reflection_members(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    _info: &crate::types::ClassInfo,
) -> Result<Vec<ReflectionListedMember>> {
    let mut members = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(enum_info) = ctx.module.enum_infos.get(class_name) {
        for case in &enum_info.cases {
            push_unique_constant_reflection_member(
                &case.name,
                class_name,
                case.attribute_names.clone(),
                case.attribute_args.clone(),
                ReflectionConstantValue::EnumCase {
                    enum_name: class_name.to_string(),
                    case_name: case.name.clone(),
                },
                None,
                Visibility::Public,
                false,
                true,
                &mut members,
                &mut seen,
            );
        }
    }
    let mut current = Some(class_name.to_string());
    while let Some(current_name) = current {
        let Some((resolved_name, current_info)) = resolve_reflection_class(ctx, &current_name)
        else {
            break;
        };
        for (constant_name, value_expr) in &current_info.constants {
            if seen.contains(constant_name) {
                continue;
            }
            let value =
                reflection_constant_value(ctx, resolved_name, Some(current_info), value_expr, 0)?;
            push_unique_constant_reflection_member(
                constant_name,
                resolved_name,
                current_info
                    .constant_attribute_names
                    .get(constant_name)
                    .cloned()
                    .unwrap_or_default(),
                current_info
                    .constant_attribute_args
                    .get(constant_name)
                    .cloned()
                    .unwrap_or_default(),
                value,
                current_info
                    .constant_types
                    .get(constant_name)
                    .and_then(reflection_declared_type_metadata),
                current_info
                    .constant_visibilities
                    .get(constant_name)
                    .cloned()
                    .unwrap_or(Visibility::Public),
                current_info.final_constants.contains(constant_name),
                false,
                &mut members,
                &mut seen,
            );
        }
        for interface_name in &current_info.interfaces {
            for member in reflection_interface_constant_reflection_members(ctx, interface_name)? {
                push_unique_listed_constant_member(member, &mut members, &mut seen);
            }
        }
        current = current_info.parent.clone();
        if current.as_deref() == Some(resolved_name) {
            break;
        }
    }
    Ok(members)
}

/// Returns enum-case reflector members for `ReflectionEnum::getCases()`.
pub(super) fn reflection_enum_case_members(
    ctx: &FunctionContext<'_>,
    enum_name: &str,
) -> Vec<ReflectionListedMember> {
    let Some(enum_info) = ctx.module.enum_infos.get(enum_name) else {
        return Vec::new();
    };
    enum_info
        .cases
        .iter()
        .map(|case| ReflectionListedMember {
            name: case.name.clone(),
            declaring_class_name: Some(enum_name.to_string()),
            attr_names: case.attribute_names.clone(),
            attr_args: case.attribute_args.clone(),
            constant_value: Some(ReflectionConstantValue::EnumCase {
                enum_name: enum_name.to_string(),
                case_name: case.name.clone(),
            }),
            backing_value: reflection_enum_case_backing_value(case),
            is_enum_case: true,
            flags: reflection_member_flags(
                false,
                &Visibility::Public,
                false,
                false,
                false,
                false,
            ),
            modifiers: reflection_class_constant_modifiers(&Visibility::Public, false),
            type_metadata: None,
            default_value: None,
            property_hook_members: Vec::new(),
            required_parameter_count: 0,
            is_deprecated: false,
            is_generator: false,
            prototype_member: None,
            parameters: Vec::new(),
        })
        .collect()
}

/// Returns constant-reflector objects for interface constants.
pub(super) fn reflection_interface_constant_reflection_members(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
) -> Result<Vec<ReflectionListedMember>> {
    let mut members = Vec::new();
    let mut seen = std::collections::HashSet::new();
    collect_interface_constant_reflection_members(ctx, interface_name, &mut members, &mut seen)?;
    Ok(members)
}

/// Appends flattened interface constant-reflector objects with declaring-interface metadata.
pub(super) fn collect_interface_constant_reflection_members(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    members: &mut Vec<ReflectionListedMember>,
    seen: &mut std::collections::HashSet<String>,
) -> Result<()> {
    let Some(interface_info) = ctx.module.interface_infos.get(interface_name) else {
        return Ok(());
    };
    for (constant_name, value_expr) in &interface_info.constants {
        let declaring_interface =
            interface_constant_declaring_interface(interface_info, interface_name, constant_name);
        let declaring_info = ctx.module.interface_infos.get(declaring_interface);
        let is_final =
            declaring_info.is_some_and(|info| info.final_constants.contains(constant_name));
        let value = reflection_constant_value(ctx, declaring_interface, None, value_expr, 0)?;
        push_unique_constant_reflection_member(
            constant_name,
            declaring_interface,
            declaring_info
                .and_then(|info| info.constant_attribute_names.get(constant_name))
                .cloned()
                .unwrap_or_default(),
            declaring_info
                .and_then(|info| info.constant_attribute_args.get(constant_name))
                .cloned()
                .unwrap_or_default(),
            value,
            declaring_info
                .and_then(|info| info.constant_types.get(constant_name))
                .and_then(reflection_declared_type_metadata),
            Visibility::Public,
            is_final,
            false,
            members,
            seen,
        );
    }
    Ok(())
}

/// Returns constant-reflector objects for direct trait constants.
pub(super) fn reflection_trait_constant_reflection_members(
    ctx: &FunctionContext<'_>,
    trait_name: &str,
) -> Result<Vec<ReflectionListedMember>> {
    let mut members = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let Some(constants) = ctx.module.declared_trait_constants.get(trait_name) else {
        return Ok(members);
    };
    let final_constants = ctx.module.declared_trait_final_constants.get(trait_name);
    for (constant_name, value_expr) in constants {
        let value = reflection_constant_value(ctx, trait_name, None, value_expr, 0)?;
        push_unique_constant_reflection_member(
            constant_name,
            trait_name,
            Vec::new(),
            Vec::new(),
            value,
            ctx.module
                .declared_trait_constant_types
                .get(trait_name)
                .and_then(|types| types.get(constant_name))
                .and_then(reflection_declared_type_metadata),
            ctx.module
                .declared_trait_constant_visibilities
                .get(trait_name)
                .and_then(|constants| constants.get(constant_name))
                .cloned()
                .unwrap_or(Visibility::Public),
            final_constants.is_some_and(|constants| constants.contains(constant_name)),
            false,
            &mut members,
            &mut seen,
        );
    }
    Ok(members)
}

/// Appends one constant-reflector member if a constant with this name was not already visible.
pub(super) fn push_unique_constant_reflection_member(
    name: &str,
    declaring_class_name: &str,
    attr_names: Vec<String>,
    attr_args: Vec<Option<Vec<AttrArgEntry>>>,
    value: ReflectionConstantValue,
    type_metadata: Option<ReflectionParameterTypeMetadata>,
    visibility: Visibility,
    is_final: bool,
    is_enum_case: bool,
    members: &mut Vec<ReflectionListedMember>,
    seen: &mut std::collections::HashSet<String>,
) {
    if !seen.insert(name.to_string()) {
        return;
    }
    members.push(ReflectionListedMember {
        name: name.to_string(),
        declaring_class_name: Some(declaring_class_name.to_string()),
        attr_names,
        attr_args,
        constant_value: Some(value),
        backing_value: None,
        is_enum_case,
        flags: reflection_member_flags(false, &visibility, is_final, false, false, false),
        modifiers: reflection_class_constant_modifiers(&visibility, is_final),
        type_metadata,
        default_value: None,
        property_hook_members: Vec::new(),
        required_parameter_count: 0,
        is_deprecated: false,
        is_generator: false,
        prototype_member: None,
        parameters: Vec::new(),
    });
}

/// Appends a prebuilt constant-reflector member if its name was not already visible.
pub(super) fn push_unique_listed_constant_member(
    member: ReflectionListedMember,
    members: &mut Vec<ReflectionListedMember>,
    seen: &mut std::collections::HashSet<String>,
) {
    if seen.insert(member.name.clone()) {
        members.push(member);
    }
}
