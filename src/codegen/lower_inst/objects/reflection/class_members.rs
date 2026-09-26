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
        // A user enum records its cases in `constant_order`, interleaved with its constants in
        // declaration order; only an enum without that record lists its cases first.
        let ordered = resolve_reflection_class(ctx, class_name).is_some_and(|(_, info)| {
            enum_info.cases.iter().all(|case| info.constant_order.contains(&case.name))
        });
        if !ordered {
            for case in &enum_info.cases {
                push_unique_constant_name(&case.name, &mut names, &mut seen);
            }
        }
    }
    push_class_constant_names_in_php_order(ctx, class_name, &mut names, &mut seen, 0);
    names
}

/// Appends a class's constant names in the order PHP's constants table holds them.
///
/// Measured on PHP 8.5.10: the class's own constants in declaration order (trait constants after
/// them), then its parent's whole list, then each implemented interface's list, keeping the first
/// occurrence of a name. `ClassInfo::constants` is a map, so iterating it gave a different order
/// on every build.
fn push_class_constant_names_in_php_order(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    names: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
    depth: usize,
) {
    if depth > ctx.module.class_infos.len() {
        return;
    }
    let Some((resolved_name, info)) = resolve_reflection_class(ctx, class_name) else {
        return;
    };
    for constant in declared_then_remaining(&info.constant_order, info.constants.keys()) {
        push_unique_constant_name(constant, names, seen);
    }
    if let Some(parent) = info.parent.as_deref() {
        if parent != resolved_name {
            push_class_constant_names_in_php_order(ctx, parent, names, seen, depth + 1);
        }
    }
    for interface_name in &info.interfaces {
        for constant in reflection_interface_constant_names(ctx, interface_name) {
            push_unique_constant_name(&constant, names, seen);
        }
    }
}

/// Yields the names of `order` first, then any other key of the map in sorted order, so a
/// constant missing from the recorded order still appears, deterministically.
pub(super) fn declared_then_remaining<'a>(
    order: &'a [String],
    keys: impl Iterator<Item = &'a String>,
) -> Vec<&'a String> {
    let mut remaining: Vec<&String> = keys.filter(|key| !order.contains(key)).collect();
    remaining.sort();
    order.iter().chain(remaining).collect()
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
    // The values are gathered walking up the hierarchy; the ORDER is PHP's, which puts a
    // parent's constants before the class's interfaces, so sort by the name order.
    let order = reflection_class_constant_names(ctx, class_name, _info);
    members.sort_by_key(|member| order.iter().position(|name| *name == member.name).unwrap_or(usize::MAX));
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
    for constant_name in declared_then_remaining(&interface_info.constant_order, interface_info.constants.keys()) {
        let value_expr = &interface_info.constants[constant_name];
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
    // `declared_trait_constants` is a map; the trait's declaration order is the name list.
    let order = reflection_trait_constant_names(ctx, trait_name);
    members.sort_by_key(|member| order.iter().position(|name| *name == member.name).unwrap_or(usize::MAX));
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
    // Same ordering as `reflection_class_constant_members`: PHP's, not the walk's.
    let order = reflection_class_constant_names(ctx, class_name, _info);
    members.sort_by_key(|member| order.iter().position(|name| *name == member.name).unwrap_or(usize::MAX));
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
        returns_reference: false,
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
    for constant_name in declared_then_remaining(&interface_info.constant_order, interface_info.constants.keys()) {
        let value_expr = &interface_info.constants[constant_name];
        let declaring_interface =
            interface_constant_declaring_interface(interface_info, interface_name, constant_name);
        let is_final = ctx
            .module
            .interface_infos
            .get(declaring_interface)
            .is_some_and(|info| info.final_constants.contains(constant_name));
        let value = reflection_constant_value(ctx, declaring_interface, None, value_expr, 0)?;
        push_unique_constant_reflection_member(
            constant_name,
            declaring_interface,
            Vec::new(),
            Vec::new(),
            value,
            interface_info
                .constant_types
                .get(constant_name)
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
    // `declared_trait_constants` is a map; the trait's declaration order is the name list.
    let order = reflection_trait_constant_names(ctx, trait_name);
    members.sort_by_key(|member| order.iter().position(|name| *name == member.name).unwrap_or(usize::MAX));
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
        returns_reference: false,
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

