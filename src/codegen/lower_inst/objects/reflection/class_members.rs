//! Purpose:
//! Visible methods, properties, constants, defaults, and enum members.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;
use std::collections::HashMap;

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

/// Sorts members into the position their name has in `order`; a name missing from it goes last.
///
/// The positions are indexed once, so ordering stays linear in the number of constants rather
/// than scanning the whole name list for every member.
fn sort_by_name_order<T>(members: &mut [T], order: &[String], name: impl Fn(&T) -> &String) {
    let positions: std::collections::HashMap<&str, usize> = order
        .iter()
        .enumerate()
        .map(|(position, name)| (name.as_str(), position))
        .collect();
    members.sort_by_key(|member| positions.get(name(member).as_str()).copied().unwrap_or(usize::MAX));
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
    sort_by_name_order(&mut members, &order, |member| &member.name);
    Ok(members)
}

/// How each class on a chain orders the properties it declares: the class is depth 0, its parent
/// 1, and so on, and each depth carries that class's own declaration order.
type PropertyRankTable = HashMap<String, (usize, HashMap<String, usize>)>;

/// Builds the rank table for `class_name` and its ancestors, walking the chain once per listing
/// rather than once per property.
///
/// The class's property lists follow the inherited storage layout, which puts an ancestor's
/// properties first and keeps a redeclared property in its ancestor's slot; PHP lists a class's
/// own properties before its parent's, in the order the class declares them.
fn reflection_property_rank_table(ctx: &FunctionContext<'_>, class_name: &str) -> PropertyRankTable {
    let mut table = HashMap::new();
    let mut current = Some(class_name.to_string());
    let mut depth = 0;
    while let Some(name) = current {
        let resolved = resolve_reflection_class(ctx, &name);
        let order = resolved
            .map(|(_, info)| {
                info.property_order
                    .iter()
                    .enumerate()
                    .map(|(index, property)| (property.clone(), index))
                    .collect()
            })
            .unwrap_or_default();
        table.entry(php_symbol_key(&name)).or_insert((depth, order));
        current = resolved.and_then(|(_, info)| info.parent.clone());
        depth += 1;
    }
    table
}

/// Returns a property's rank: its declaring class's depth, then its position in that class's own
/// declarations. An unknown declaring class counts as the class itself, and a property missing
/// from the declaration order (a trait's) sorts after the declared ones.
fn reflection_property_rank(
    table: &PropertyRankTable,
    class_name: &str,
    declaring_class: Option<&String>,
    property_name: &str,
) -> (usize, usize) {
    let declaring = declaring_class.map(String::as_str).unwrap_or(class_name);
    table
        .get(&php_symbol_key(declaring))
        .map(|(depth, order)| (*depth, order.get(property_name).copied().unwrap_or(usize::MAX)))
        .unwrap_or((0, usize::MAX))
}

/// Returns materializable property defaults for `ReflectionClass::getDefaultProperties()`.
///
/// PHP lists the static properties first, then the instance ones; each group has the class's
/// own properties before its ancestors', in declaration order.
pub(super) fn reflection_class_default_property_members(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    info: &crate::types::ClassInfo,
    property_names: &[String],
) -> Vec<ReflectionDefaultPropertyMember> {
    let table = reflection_property_rank_table(ctx, class_name);
    let mut ranked: Vec<(bool, (usize, usize), usize, ReflectionDefaultPropertyMember)> = property_names
        .iter()
        .enumerate()
        .filter_map(|(position, property_name)| {
            let is_static = info.static_properties.iter().any(|(name, _)| name == property_name);
            let declaring_class = if is_static {
                info.static_property_declaring_classes.get(property_name)
            } else {
                info.property_declaring_classes.get(property_name)
            };
            let rank = reflection_property_rank(&table, class_name, declaring_class, property_name);
            reflection_property_default_value(info, property_name).map(|value| {
                let member = ReflectionDefaultPropertyMember {
                    name: property_name.clone(),
                    value,
                };
                (!is_static, rank, position, member)
            })
        })
        .collect();
    ranked.sort_by_key(|(instance, rank, position, _)| (*instance, *rank, *position));
    ranked.into_iter().map(|(_, _, _, member)| member).collect()
}

/// Returns static-property storage slots for `ReflectionClass::getStaticProperties()`.
///
/// An ancestor's private static is not part of the class, as PHP reports it; the class's own
/// statics come before its ancestors', in declaration order.
pub(super) fn reflection_class_static_property_members(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    info: &crate::types::ClassInfo,
) -> Vec<ReflectionStaticPropertyMember> {
    let table = reflection_property_rank_table(ctx, class_name);
    let mut visible: Vec<((usize, usize), usize, &(String, PhpType))> = info
        .static_properties
        .iter()
        .enumerate()
        .filter(|(_, (property_name, _))| {
            reflection_property_visible_from_class(info, class_name, property_name, true)
        })
        .map(|(position, entry)| {
            let declaring_class = info.static_property_declaring_classes.get(&entry.0);
            let rank = reflection_property_rank(&table, class_name, declaring_class, &entry.0);
            (rank, position, entry)
        })
        .collect();
    visible.sort_by_key(|(rank, position, _)| (*rank, *position));
    visible
        .into_iter()
        .map(|(_, _, (property_name, php_type))| {
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
    sort_by_name_order(&mut members, &order, |member| &member.name);
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
    sort_by_name_order(&mut members, &order, |member| &member.name);
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
    sort_by_name_order(&mut members, &order, |member| &member.name);
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

