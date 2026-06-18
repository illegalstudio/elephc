//! Purpose:
//! Lowers metadata-aware allocation for builtin Reflection owner objects in the
//! EIR backend.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::objects::lower_object_new()`.
//!
//! Key details:
//! - `ReflectionClass`, `ReflectionMethod`, `ReflectionProperty`,
//!   `ReflectionClassConstant`, and `ReflectionEnum*`
//!   constructors are compile-time metadata lookups that populate private
//!   `__name`/`__attrs` slots instead of running their public empty bodies.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::{Immediate, Instruction, Op, ValueDef, ValueId};
use crate::names::php_symbol_key;
use crate::parser::ast::Visibility;
use crate::types::{AttrArgValue, PhpType};

use super::super::super::context::FunctionContext;

/// Compile-time metadata used to populate one Reflection owner object.
struct ReflectionOwnerMetadata {
    reflected_name: Option<String>,
    attr_names: Vec<String>,
    attr_args: Vec<Option<Vec<AttrArgValue>>>,
    interface_names: Vec<String>,
    trait_names: Vec<String>,
    method_names: Vec<String>,
    property_names: Vec<String>,
    method_members: Vec<ReflectionListedMember>,
    property_members: Vec<ReflectionListedMember>,
    is_final: bool,
    is_abstract: bool,
    is_interface: bool,
    is_trait: bool,
    is_enum: bool,
    is_readonly: bool,
    modifiers: i64,
    member_flags: ReflectionMemberFlags,
}

/// Metadata for one member object returned by `ReflectionClass::getMethods()` or `getProperties()`.
struct ReflectionListedMember {
    name: String,
    attr_names: Vec<String>,
    attr_args: Vec<Option<Vec<AttrArgValue>>>,
    flags: ReflectionMemberFlags,
}

/// Boolean metadata exposed by ReflectionMethod and ReflectionProperty predicates.
#[derive(Clone, Copy, Default)]
struct ReflectionMemberFlags {
    is_static: bool,
    is_public: bool,
    is_protected: bool,
    is_private: bool,
    is_final: bool,
    is_abstract: bool,
}

/// Returns true for reflection owner classes that need metadata-aware construction.
pub(super) fn is_reflection_owner_class(class_name: &str) -> bool {
    matches!(
        class_name,
        "ReflectionClass"
            | "ReflectionMethod"
            | "ReflectionProperty"
            | "ReflectionClassConstant"
            | "ReflectionEnumUnitCase"
            | "ReflectionEnumBackedCase"
    )
}

/// Lowers builtin Reflection owner allocation by populating compile-time metadata slots.
pub(super) fn lower_reflection_owner_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
) -> Result<()> {
    let metadata = reflection_owner_metadata(ctx, class_name, inst)?;
    let (class_id, property_count, uninitialized_marker_offsets) = {
        let class_info =
            ctx.module.class_infos.get(class_name).ok_or_else(|| {
                CodegenIrError::unsupported(format!("unknown class {}", class_name))
            })?;
        (
            class_info.class_id,
            class_info.properties.len(),
            super::uninitialized_property_marker_offsets(class_info),
        )
    };
    super::emit_object_allocation(
        ctx,
        class_id,
        property_count,
        false,
        &uninitialized_marker_offsets,
    )?;
    if let Some(reflected_name) = metadata.reflected_name.as_deref() {
        emit_reflection_string_property(ctx, reflected_name, 8, 16);
        if class_name == "ReflectionClass" {
            emit_reflection_class_name_parts(ctx, reflected_name)?;
            emit_reflection_string_array_property_by_name(
                ctx,
                "__interface_names",
                &metadata.interface_names,
            )?;
            emit_reflection_string_array_property_by_name(
                ctx,
                "__trait_names",
                &metadata.trait_names,
            )?;
            emit_reflection_string_array_property_by_name(
                ctx,
                "__method_names",
                &metadata.method_names,
            )?;
            emit_reflection_string_array_property_by_name(
                ctx,
                "__property_names",
                &metadata.property_names,
            )?;
            emit_reflection_member_array_property_by_name(
                ctx,
                "__methods",
                "ReflectionMethod",
                &metadata.method_members,
            )?;
            emit_reflection_member_array_property_by_name(
                ctx,
                "__properties",
                "ReflectionProperty",
                &metadata.property_members,
            )?;
        }
    }
    emit_reflection_attrs_property(ctx, class_name, &metadata.attr_names, &metadata.attr_args)?;
    if class_name == "ReflectionClass" {
        emit_reflection_bool_property(ctx, "__is_final", metadata.is_final)?;
        emit_reflection_bool_property(ctx, "__is_abstract", metadata.is_abstract)?;
        emit_reflection_bool_property(ctx, "__is_interface", metadata.is_interface)?;
        emit_reflection_bool_property(ctx, "__is_trait", metadata.is_trait)?;
        emit_reflection_bool_property(ctx, "__is_enum", metadata.is_enum)?;
        emit_reflection_bool_property(ctx, "__is_readonly", metadata.is_readonly)?;
        emit_reflection_int_property(ctx, "__modifiers", metadata.modifiers)?;
    }
    emit_reflection_member_flag_properties(ctx, class_name, metadata.member_flags)?;
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("reflection object_new missing result"))?;
    ctx.store_result_value(result)
}

/// Stores namespace-aware name parts for a statically materialized ReflectionClass.
fn emit_reflection_class_name_parts(
    ctx: &mut FunctionContext<'_>,
    reflected_name: &str,
) -> Result<()> {
    let (namespace_name, short_name) = reflection_name_parts(reflected_name);
    emit_reflection_string_property_by_name(ctx, "__short_name", short_name)?;
    emit_reflection_string_property_by_name(ctx, "__namespace_name", namespace_name)?;
    emit_reflection_bool_property(ctx, "__in_namespace", !namespace_name.is_empty())?;
    Ok(())
}

/// Splits a canonical PHP class-like name into namespace and short-name parts.
fn reflection_name_parts(reflected_name: &str) -> (&str, &str) {
    match reflected_name.rfind('\\') {
        Some(separator) => (
            &reflected_name[..separator],
            &reflected_name[separator + 1..],
        ),
        None => ("", reflected_name),
    }
}

/// Resolves Reflection constructor operands to captured class/member metadata.
fn reflection_owner_metadata(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    match class_name {
        "ReflectionClass" => reflection_class_metadata(ctx, inst),
        "ReflectionMethod" => reflection_method_metadata(ctx, inst),
        "ReflectionProperty" => reflection_property_metadata(ctx, inst),
        "ReflectionClassConstant" => reflection_class_constant_metadata(ctx, inst),
        "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase" => {
            reflection_enum_case_metadata(ctx, class_name, inst)
        }
        _ => Ok(empty_reflection_metadata()),
    }
}

/// Resolves `ReflectionClass(class)` metadata.
fn reflection_class_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    let Some(class_operand) = inst.operands.first().copied() else {
        return Ok(empty_reflection_metadata());
    };
    let reflected_class = const_string_or_class_operand(ctx, class_operand, "ReflectionClass")?;
    if let Some((class_name, info)) = resolve_reflection_class(ctx, &reflected_class) {
        let is_enum = is_reflection_enum(ctx, class_name);
        let method_names = reflection_class_method_names(ctx, class_name);
        let property_names = reflection_class_property_names(ctx, class_name, info);
        let method_members = reflection_class_method_members(info, &method_names);
        let property_members =
            reflection_class_property_members(ctx, class_name, info, &property_names);
        return Ok(ReflectionOwnerMetadata {
            reflected_name: Some(class_name.to_string()),
            attr_names: info.attribute_names.clone(),
            attr_args: info.attribute_args.clone(),
            interface_names: info.interfaces.clone(),
            trait_names: info.used_traits.clone(),
            method_names,
            property_names,
            method_members,
            property_members,
            is_final: info.is_final,
            is_abstract: info.is_abstract,
            is_interface: false,
            is_trait: false,
            is_enum,
            is_readonly: info.is_readonly_class && !is_enum,
            modifiers: reflection_class_modifiers(
                info.is_final,
                info.is_abstract,
                info.is_readonly_class,
                is_enum,
            ),
            member_flags: ReflectionMemberFlags::default(),
        });
    }
    if let Some(interface_name) = resolve_reflection_interface(ctx, &reflected_class) {
        return Ok(class_like_reflection_metadata(
            interface_name,
            reflection_interface_parent_names(ctx, interface_name),
            Vec::new(),
            reflection_interface_method_names(ctx, interface_name),
            reflection_interface_property_names(ctx, interface_name),
            true,
            false,
            false,
        ));
    }
    if let Some(trait_name) = resolve_reflection_trait(ctx, &reflected_class) {
        let trait_names = ctx
            .module
            .declared_trait_uses
            .get(trait_name)
            .cloned()
            .unwrap_or_default();
        return Ok(class_like_reflection_metadata(
            trait_name,
            Vec::new(),
            trait_names,
            reflection_trait_method_names(ctx, trait_name),
            reflection_trait_property_names(ctx, trait_name),
            false,
            true,
            false,
        ));
    }
    Ok(empty_reflection_metadata())
}

/// Resolves `ReflectionMethod(class, method)` metadata.
fn reflection_method_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    let Some(class_operand) = inst.operands.first().copied() else {
        return Ok(empty_reflection_metadata());
    };
    let Some(method_operand) = inst.operands.get(1).copied() else {
        return Ok(empty_reflection_metadata());
    };
    let reflected_class = const_string_or_class_operand(ctx, class_operand, "ReflectionMethod")?;
    let method_name = const_required_string_operand(ctx, method_operand, "ReflectionMethod")?;
    let method_key = php_symbol_key(&method_name);
    Ok(resolve_reflection_class(ctx, &reflected_class)
        .and_then(|(_, info)| {
            Some(ReflectionOwnerMetadata {
                reflected_name: Some(method_name.clone()),
                attr_names: info.method_attribute_names.get(&method_key)?.clone(),
                attr_args: info.method_attribute_args.get(&method_key)?.clone(),
                interface_names: Vec::new(),
                trait_names: Vec::new(),
                method_names: Vec::new(),
                property_names: Vec::new(),
                method_members: Vec::new(),
                property_members: Vec::new(),
                is_final: false,
                is_abstract: false,
                is_interface: false,
                is_trait: false,
                is_enum: false,
                is_readonly: false,
                modifiers: 0,
                member_flags: reflection_method_member_flags(info, &method_key)?,
            })
        })
        .unwrap_or_else(empty_reflection_metadata))
}

/// Resolves `ReflectionProperty(class, property)` metadata.
fn reflection_property_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    let Some(class_operand) = inst.operands.first().copied() else {
        return Ok(empty_reflection_metadata());
    };
    let Some(property_operand) = inst.operands.get(1).copied() else {
        return Ok(empty_reflection_metadata());
    };
    let reflected_class = const_string_or_class_operand(ctx, class_operand, "ReflectionProperty")?;
    let property_name = const_required_string_operand(ctx, property_operand, "ReflectionProperty")?;
    Ok(resolve_reflection_class(ctx, &reflected_class)
        .and_then(|(_, info)| {
            Some(ReflectionOwnerMetadata {
                reflected_name: Some(property_name.clone()),
                attr_names: info.property_attribute_names.get(&property_name)?.clone(),
                attr_args: info.property_attribute_args.get(&property_name)?.clone(),
                interface_names: Vec::new(),
                trait_names: Vec::new(),
                method_names: Vec::new(),
                property_names: Vec::new(),
                method_members: Vec::new(),
                property_members: Vec::new(),
                is_final: false,
                is_abstract: false,
                is_interface: false,
                is_trait: false,
                is_enum: false,
                is_readonly: false,
                modifiers: 0,
                member_flags: reflection_property_member_flags(info, &property_name)?,
            })
        })
        .unwrap_or_else(empty_reflection_metadata))
}

/// Resolves `ReflectionClassConstant(class, constant)` metadata.
fn reflection_class_constant_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    let Some(class_operand) = inst.operands.first().copied() else {
        return Ok(empty_reflection_metadata());
    };
    let Some(constant_operand) = inst.operands.get(1).copied() else {
        return Ok(empty_reflection_metadata());
    };
    let reflected_class =
        const_string_or_class_operand(ctx, class_operand, "ReflectionClassConstant")?;
    let constant_name =
        const_required_string_operand(ctx, constant_operand, "ReflectionClassConstant")?;
    if let Some(case) = resolve_reflection_enum_case(ctx, &reflected_class, &constant_name) {
        return Ok(ReflectionOwnerMetadata {
            reflected_name: Some(constant_name.clone()),
            attr_names: case.attribute_names.clone(),
            attr_args: case.attribute_args.clone(),
            interface_names: Vec::new(),
            trait_names: Vec::new(),
            method_names: Vec::new(),
            property_names: Vec::new(),
            method_members: Vec::new(),
            property_members: Vec::new(),
            is_final: false,
            is_abstract: false,
            is_interface: false,
            is_trait: false,
            is_enum: false,
            is_readonly: false,
            modifiers: 0,
            member_flags: ReflectionMemberFlags::default(),
        });
    }
    Ok(
        resolve_reflection_class_constant(ctx, &reflected_class, &constant_name)
            .map(|(_, info)| {
                let attr_names = info
                    .constant_attribute_names
                    .get(&constant_name)
                    .cloned()
                    .unwrap_or_default();
                let attr_args = info
                    .constant_attribute_args
                    .get(&constant_name)
                    .cloned()
                    .unwrap_or_default();
                ReflectionOwnerMetadata {
                    reflected_name: Some(constant_name),
                    attr_names,
                    attr_args,
                    interface_names: Vec::new(),
                    trait_names: Vec::new(),
                    method_names: Vec::new(),
                    property_names: Vec::new(),
                    method_members: Vec::new(),
                    property_members: Vec::new(),
                    is_final: false,
                    is_abstract: false,
                    is_interface: false,
                    is_trait: false,
                    is_enum: false,
                    is_readonly: false,
                    modifiers: 0,
                    member_flags: ReflectionMemberFlags::default(),
                }
            })
            .unwrap_or_else(empty_reflection_metadata),
    )
}

/// Resolves `ReflectionEnumUnitCase/BackedCase(enum, case)` metadata.
fn reflection_enum_case_metadata(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    let Some(enum_operand) = inst.operands.first().copied() else {
        return Ok(empty_reflection_metadata());
    };
    let Some(case_operand) = inst.operands.get(1).copied() else {
        return Ok(empty_reflection_metadata());
    };
    let reflected_enum = const_string_or_class_operand(ctx, enum_operand, class_name)?;
    let case_name = const_required_string_operand(ctx, case_operand, class_name)?;
    Ok(
        resolve_reflection_enum_case(ctx, &reflected_enum, &case_name)
            .map(|case| ReflectionOwnerMetadata {
                reflected_name: Some(case_name.clone()),
                attr_names: case.attribute_names.clone(),
                attr_args: case.attribute_args.clone(),
                interface_names: Vec::new(),
                trait_names: Vec::new(),
                method_names: Vec::new(),
                property_names: Vec::new(),
                method_members: Vec::new(),
                property_members: Vec::new(),
                is_final: false,
                is_abstract: false,
                is_interface: false,
                is_trait: false,
                is_enum: false,
                is_readonly: false,
                modifiers: 0,
                member_flags: ReflectionMemberFlags::default(),
            })
            .unwrap_or_else(empty_reflection_metadata),
    )
}

/// Looks up class metadata by PHP-style case-insensitive name.
fn resolve_reflection_class<'a>(
    ctx: &'a FunctionContext<'_>,
    class_name: &str,
) -> Option<(&'a str, &'a crate::types::ClassInfo)> {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    ctx.module
        .class_infos
        .iter()
        .find(|(candidate, _)| php_symbol_key(candidate.trim_start_matches('\\')) == class_key)
        .map(|(name, info)| (name.as_str(), info))
}

/// Looks up interface metadata by PHP-style case-insensitive name.
fn resolve_reflection_interface<'a>(
    ctx: &'a FunctionContext<'_>,
    interface_name: &str,
) -> Option<&'a str> {
    let interface_key = php_symbol_key(interface_name.trim_start_matches('\\'));
    ctx.module
        .interface_infos
        .keys()
        .find(|candidate| php_symbol_key(candidate.trim_start_matches('\\')) == interface_key)
        .map(String::as_str)
}

/// Looks up a declared trait by PHP-style case-insensitive name.
fn resolve_reflection_trait<'a>(ctx: &'a FunctionContext<'_>, trait_name: &str) -> Option<&'a str> {
    let trait_key = php_symbol_key(trait_name.trim_start_matches('\\'));
    ctx.module
        .trait_table
        .names
        .iter()
        .find(|candidate| php_symbol_key(candidate.trim_start_matches('\\')) == trait_key)
        .map(String::as_str)
}

/// Looks up enum metadata by PHP-style case-insensitive name.
fn is_reflection_enum(ctx: &FunctionContext<'_>, enum_name: &str) -> bool {
    let enum_key = php_symbol_key(enum_name.trim_start_matches('\\'));
    ctx.module
        .enum_infos
        .keys()
        .any(|candidate| php_symbol_key(candidate.trim_start_matches('\\')) == enum_key)
}

/// Collects direct and inherited parent interfaces for a reflected interface.
fn reflection_interface_parent_names(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
) -> Vec<String> {
    let mut names = Vec::new();
    collect_reflection_interface_parent_names(ctx, interface_name, &mut names);
    names
}

/// Recursively collects interface parents without duplicating case-insensitive names.
fn collect_reflection_interface_parent_names(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    names: &mut Vec<String>,
) {
    let Some(interface) = ctx.module.interface_infos.get(interface_name) else {
        return;
    };
    for parent in &interface.parents {
        let parent_name = resolve_reflection_interface(ctx, parent)
            .map(str::to_string)
            .unwrap_or_else(|| parent.clone());
        if !names
            .iter()
            .any(|name| php_symbol_key(name) == php_symbol_key(&parent_name))
        {
            names.push(parent_name.clone());
            collect_reflection_interface_parent_names(ctx, &parent_name, names);
        }
    }
}

/// Returns PHP case-insensitive method names visible to `ReflectionClass::hasMethod()`.
fn reflection_class_method_names(ctx: &FunctionContext<'_>, class_name: &str) -> Vec<String> {
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
fn reflection_class_property_names(
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

/// Returns true when a property should be visible for `ReflectionClass::hasProperty()`.
fn reflection_property_visible_from_class(
    info: &crate::types::ClassInfo,
    reflected_class: &str,
    property_name: &str,
    is_static: bool,
) -> bool {
    let visibility = if is_static {
        info.static_property_visibilities.get(property_name)
    } else {
        info.property_visibilities.get(property_name)
    };
    if visibility != Some(&Visibility::Private) {
        return true;
    }
    let declaring_class = if is_static {
        info.static_property_declaring_classes.get(property_name)
    } else {
        info.property_declaring_classes.get(property_name)
    };
    declaring_class
        .map(|declaring_class| php_symbol_key(declaring_class) == php_symbol_key(reflected_class))
        .unwrap_or(false)
}

/// Returns ReflectionMethod predicate flags for a method visible on one class.
fn reflection_method_member_flags(
    info: &crate::types::ClassInfo,
    method_key: &str,
) -> Option<ReflectionMemberFlags> {
    if info.methods.contains_key(method_key) {
        let visibility = info
            .method_visibilities
            .get(method_key)
            .unwrap_or(&Visibility::Public);
        return Some(reflection_member_flags(
            false,
            visibility,
            info.final_methods.contains(method_key),
            !info.method_impl_classes.contains_key(method_key),
        ));
    }
    if info.static_methods.contains_key(method_key) {
        let visibility = info
            .static_method_visibilities
            .get(method_key)
            .unwrap_or(&Visibility::Public);
        return Some(reflection_member_flags(
            true,
            visibility,
            info.final_static_methods.contains(method_key),
            !info.static_method_impl_classes.contains_key(method_key),
        ));
    }
    None
}

/// Returns ReflectionProperty predicate flags for a property visible on one class.
fn reflection_property_member_flags(
    info: &crate::types::ClassInfo,
    property_name: &str,
) -> Option<ReflectionMemberFlags> {
    if info
        .properties
        .iter()
        .any(|(name, _)| name == property_name)
    {
        let visibility = info
            .property_visibilities
            .get(property_name)
            .unwrap_or(&Visibility::Public);
        return Some(reflection_member_flags(false, visibility, false, false));
    }
    if info
        .static_properties
        .iter()
        .any(|(name, _)| name == property_name)
    {
        let visibility = info
            .static_property_visibilities
            .get(property_name)
            .unwrap_or(&Visibility::Public);
        return Some(reflection_member_flags(true, visibility, false, false));
    }
    None
}

/// Builds ReflectionMethod array entries for the methods visible on one class.
fn reflection_class_method_members(
    info: &crate::types::ClassInfo,
    method_names: &[String],
) -> Vec<ReflectionListedMember> {
    method_names
        .iter()
        .filter_map(|method_name| reflection_class_method_member(info, method_name))
        .collect()
}

/// Builds one ReflectionMethod array entry from class metadata.
fn reflection_class_method_member(
    info: &crate::types::ClassInfo,
    method_name: &str,
) -> Option<ReflectionListedMember> {
    let method_key = php_symbol_key(method_name);
    Some(ReflectionListedMember {
        name: method_key.clone(),
        attr_names: info
            .method_attribute_names
            .get(&method_key)
            .cloned()
            .unwrap_or_default(),
        attr_args: info
            .method_attribute_args
            .get(&method_key)
            .cloned()
            .unwrap_or_default(),
        flags: reflection_method_member_flags(info, &method_key)?,
    })
}

/// Builds ReflectionProperty array entries for the properties visible on one class.
fn reflection_class_property_members(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    info: &crate::types::ClassInfo,
    property_names: &[String],
) -> Vec<ReflectionListedMember> {
    property_names
        .iter()
        .filter_map(|property_name| reflection_class_property_member(ctx, class_name, info, property_name))
        .collect()
}

/// Builds one ReflectionProperty array entry from class or enum metadata.
fn reflection_class_property_member(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    info: &crate::types::ClassInfo,
    property_name: &str,
) -> Option<ReflectionListedMember> {
    let flags = reflection_property_member_flags(info, property_name).or_else(|| {
        (is_reflection_enum(ctx, class_name) && property_name == "name").then_some(
            reflection_member_flags(false, &Visibility::Public, false, false),
        )
    })?;
    Some(ReflectionListedMember {
        name: property_name.to_string(),
        attr_names: info
            .property_attribute_names
            .get(property_name)
            .cloned()
            .unwrap_or_default(),
        attr_args: info
            .property_attribute_args
            .get(property_name)
            .cloned()
            .unwrap_or_default(),
        flags,
    })
}

/// Builds placeholder ReflectionMethod entries for class-like metadata without full method schemas.
fn default_method_members(
    method_names: &[String],
    is_interface: bool,
    _is_trait: bool,
) -> Vec<ReflectionListedMember> {
    method_names
        .iter()
        .map(|name| ReflectionListedMember {
            name: name.clone(),
            attr_names: Vec::new(),
            attr_args: Vec::new(),
            flags: reflection_member_flags(
                false,
                &Visibility::Public,
                false,
                is_interface,
            ),
        })
        .collect()
}

/// Builds placeholder ReflectionProperty entries for class-like metadata without full property schemas.
fn default_property_members(
    property_names: &[String],
    is_interface: bool,
) -> Vec<ReflectionListedMember> {
    property_names
        .iter()
        .map(|name| ReflectionListedMember {
            name: name.clone(),
            attr_names: Vec::new(),
            attr_args: Vec::new(),
            flags: reflection_member_flags(
                false,
                &Visibility::Public,
                false,
                is_interface,
            ),
        })
        .collect()
}

/// Builds common ReflectionMethod/ReflectionProperty predicate flags.
fn reflection_member_flags(
    is_static: bool,
    visibility: &Visibility,
    is_final: bool,
    is_abstract: bool,
) -> ReflectionMemberFlags {
    ReflectionMemberFlags {
        is_static,
        is_public: visibility == &Visibility::Public,
        is_protected: visibility == &Visibility::Protected,
        is_private: visibility == &Visibility::Private,
        is_final,
        is_abstract,
    }
}

/// Returns PHP case-insensitive method names declared by an interface and its parents.
fn reflection_interface_method_names(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
) -> Vec<String> {
    let Some(interface_name) = resolve_reflection_interface(ctx, interface_name) else {
        return Vec::new();
    };
    let Some(info) = ctx.module.interface_infos.get(interface_name) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    push_unique_method_names(info.methods.keys(), &mut names, &mut seen);
    names
}

/// Returns PHP case-sensitive property names declared by an interface and its parents.
fn reflection_interface_property_names(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
) -> Vec<String> {
    let Some(interface_name) = resolve_reflection_interface(ctx, interface_name) else {
        return Vec::new();
    };
    let Some(info) = ctx.module.interface_infos.get(interface_name) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for property in info.properties.keys() {
        push_unique_property_name(property, &mut names, &mut seen);
    }
    names
}

/// Returns PHP case-insensitive direct method names declared by a trait.
fn reflection_trait_method_names(ctx: &FunctionContext<'_>, trait_name: &str) -> Vec<String> {
    ctx.module
        .declared_trait_method_names
        .get(trait_name)
        .cloned()
        .unwrap_or_default()
}

/// Returns PHP case-sensitive direct property names declared by a trait.
fn reflection_trait_property_names(ctx: &FunctionContext<'_>, trait_name: &str) -> Vec<String> {
    ctx.module
        .declared_trait_property_names
        .get(trait_name)
        .cloned()
        .unwrap_or_default()
}

/// Appends lower-case method names while preserving first-seen order.
fn push_unique_method_names<'a>(
    method_names: impl Iterator<Item = &'a String>,
    names: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    for method_name in method_names {
        let key = php_symbol_key(method_name);
        if seen.insert(key.clone()) {
            names.push(key);
        }
    }
}

/// Appends one case-sensitive property name while preserving first-seen order.
fn push_unique_property_name(
    property_name: &str,
    names: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    if seen.insert(property_name.to_string()) {
        names.push(property_name.to_string());
    }
}

/// Builds empty ReflectionClass metadata for class-like symbols without stored attributes.
fn class_like_reflection_metadata(
    class_like_name: &str,
    interface_names: Vec<String>,
    trait_names: Vec<String>,
    method_names: Vec<String>,
    property_names: Vec<String>,
    is_interface: bool,
    is_trait: bool,
    is_enum: bool,
) -> ReflectionOwnerMetadata {
    let method_members = default_method_members(method_names.as_slice(), is_interface, is_trait);
    let property_members = default_property_members(property_names.as_slice(), is_interface);
    ReflectionOwnerMetadata {
        reflected_name: Some(class_like_name.to_string()),
        attr_names: Vec::new(),
        attr_args: Vec::new(),
        interface_names,
        trait_names,
        method_names,
        property_names,
        method_members,
        property_members,
        is_final: false,
        is_abstract: false,
        is_interface,
        is_trait,
        is_enum,
        is_readonly: false,
        modifiers: if is_enum { 32 } else { 0 },
        member_flags: ReflectionMemberFlags::default(),
    }
}

/// Looks up class-constant metadata by PHP-style class name and case-sensitive constant name.
fn resolve_reflection_class_constant<'a>(
    ctx: &'a FunctionContext<'_>,
    class_name: &str,
    constant_name: &str,
) -> Option<(&'a str, &'a crate::types::ClassInfo)> {
    let (resolved_name, info) = resolve_reflection_class(ctx, class_name)?;
    if info.constants.contains_key(constant_name) {
        return Some((resolved_name, info));
    }
    let parent = info.parent.as_deref()?;
    resolve_reflection_class_constant(ctx, parent, constant_name)
}

/// Looks up enum-case metadata by PHP-style enum name and case-sensitive case name.
fn resolve_reflection_enum_case<'a>(
    ctx: &'a FunctionContext<'_>,
    enum_name: &str,
    case_name: &str,
) -> Option<&'a crate::types::EnumCaseInfo> {
    let enum_key = php_symbol_key(enum_name.trim_start_matches('\\'));
    ctx.module
        .enum_infos
        .iter()
        .find(|(candidate, _)| php_symbol_key(candidate.trim_start_matches('\\')) == enum_key)
        .and_then(|(_, info)| info.cases.iter().find(|case| case.name == case_name))
}

/// Returns empty Reflection metadata for unsupported dynamic constructor operands.
fn empty_reflection_metadata() -> ReflectionOwnerMetadata {
    ReflectionOwnerMetadata {
        reflected_name: None,
        attr_names: Vec::new(),
        attr_args: Vec::new(),
        interface_names: Vec::new(),
        trait_names: Vec::new(),
        method_names: Vec::new(),
        property_names: Vec::new(),
        method_members: Vec::new(),
        property_members: Vec::new(),
        is_final: false,
        is_abstract: false,
        is_interface: false,
        is_trait: false,
        is_enum: false,
        is_readonly: false,
        modifiers: 0,
        member_flags: ReflectionMemberFlags::default(),
    }
}

/// Extracts a constant string or class-name operand from an EIR value.
fn const_string_or_class_operand(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    owner: &str,
) -> Result<String> {
    const_data_operand(ctx, value, owner, true)
}

/// Extracts a constant string operand from an EIR value.
fn const_required_string_operand(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    owner: &str,
) -> Result<String> {
    const_data_operand(ctx, value, owner, false)
}

/// Reads a `ConstStr` or optional `ConstClassName` value from the module data pool.
fn const_data_operand(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    owner: &str,
    allow_class_name: bool,
) -> Result<String> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Err(CodegenIrError::unsupported(format!(
            "{} constructor with non-literal reflection argument",
            owner
        )));
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    let Some(Immediate::Data(data)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(format!(
            "{} reflection literal missing data id",
            owner
        )));
    };
    match inst_ref.op {
        Op::ConstStr => ctx
            .module
            .data
            .strings
            .get(data.as_raw() as usize)
            .cloned()
            .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw())),
        Op::ConstClassName if allow_class_name => ctx
            .module
            .data
            .class_names
            .get(data.as_raw() as usize)
            .cloned()
            .ok_or_else(|| CodegenIrError::missing_entry("class data", data.as_raw())),
        _ => Err(CodegenIrError::unsupported(format!(
            "{} constructor with non-literal reflection argument",
            owner
        ))),
    }
}

/// Writes a heap-persisted string into the current Reflection object result slot.
fn emit_reflection_string_property(
    ctx: &mut FunctionContext<'_>,
    value: &str,
    low_offset: usize,
    high_offset: usize,
) {
    let (label, len) = ctx.data.add_string(value.as_bytes());
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            abi::emit_pop_reg(ctx.emitter, object_reg);
            abi::emit_store_to_address(ctx.emitter, "x1", object_reg, low_offset);
            abi::emit_store_to_address(ctx.emitter, "x2", object_reg, high_offset);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rax", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            abi::emit_pop_reg(ctx.emitter, object_reg);
            abi::emit_store_to_address(ctx.emitter, "rax", object_reg, low_offset);
            abi::emit_store_to_address(ctx.emitter, "rdx", object_reg, high_offset);
        }
    }
    abi::emit_push_reg(ctx.emitter, object_reg);
    abi::emit_pop_reg(ctx.emitter, result_reg);
}

/// Writes a heap-persisted string into a named ReflectionClass property slot.
fn emit_reflection_string_property_by_name(
    ctx: &mut FunctionContext<'_>,
    property_name: &str,
    value: &str,
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get("ReflectionClass")
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, property_name)?;
    emit_reflection_string_property(ctx, value, low_offset, low_offset + 8);
    Ok(())
}

/// Replaces the Reflection object's default `__attrs` array with populated metadata.
fn emit_reflection_attrs_property(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    attr_names: &[String],
    attr_args: &[Option<Vec<AttrArgValue>>],
) -> Result<()> {
    let (attrs_low_offset, attrs_high_offset) = reflection_attrs_offsets(ctx, class_name)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, 0);
    abi::emit_load_from_address(ctx.emitter, result_reg, object_reg, attrs_low_offset);
    abi::emit_call_label(ctx.emitter, "__rt_decref_array");
    super::super::builtins::attributes::emit_reflection_attribute_array(
        ctx, attr_names, attr_args,
    )?;
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, attrs_low_offset);
    abi::emit_load_int_immediate(ctx.emitter, abi::secondary_scratch_reg(ctx.emitter), 4);
    abi::emit_store_to_address(
        ctx.emitter,
        abi::secondary_scratch_reg(ctx.emitter),
        object_reg,
        attrs_high_offset,
    );
    abi::emit_push_reg(ctx.emitter, object_reg);
    abi::emit_pop_reg(ctx.emitter, result_reg);
    Ok(())
}

/// Replaces a ReflectionClass private array slot with an indexed string array.
fn emit_reflection_string_array_property_by_name(
    ctx: &mut FunctionContext<'_>,
    property_name: &str,
    names: &[String],
) -> Result<()> {
    if names.is_empty() {
        return Ok(());
    }
    let class_info = ctx
        .module
        .class_infos
        .get("ReflectionClass")
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, property_name)?;
    let high_offset = low_offset + 8;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, 0);
    abi::emit_load_from_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_call_label(ctx.emitter, "__rt_decref_array");
    emit_reflection_string_array(ctx, names)?;
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_load_int_immediate(ctx.emitter, abi::secondary_scratch_reg(ctx.emitter), 4);
    abi::emit_store_to_address(
        ctx.emitter,
        abi::secondary_scratch_reg(ctx.emitter),
        object_reg,
        high_offset,
    );
    abi::emit_push_reg(ctx.emitter, object_reg);
    abi::emit_pop_reg(ctx.emitter, result_reg);
    Ok(())
}

/// Replaces a ReflectionClass private array slot with ReflectionMethod/Property objects.
fn emit_reflection_member_array_property_by_name(
    ctx: &mut FunctionContext<'_>,
    property_name: &str,
    member_class_name: &str,
    members: &[ReflectionListedMember],
) -> Result<()> {
    if members.is_empty() {
        return Ok(());
    }
    let class_info = ctx
        .module
        .class_infos
        .get("ReflectionClass")
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, property_name)?;
    let high_offset = low_offset + 8;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, 0);
    abi::emit_load_from_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_call_label(ctx.emitter, "__rt_decref_array");
    emit_reflection_member_array(ctx, member_class_name, members)?;
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_load_int_immediate(ctx.emitter, abi::secondary_scratch_reg(ctx.emitter), 4);
    abi::emit_store_to_address(
        ctx.emitter,
        abi::secondary_scratch_reg(ctx.emitter),
        object_reg,
        high_offset,
    );
    abi::emit_push_reg(ctx.emitter, object_reg);
    abi::emit_pop_reg(ctx.emitter, result_reg);
    Ok(())
}

/// Allocates an indexed array of populated ReflectionMethod/ReflectionProperty objects.
fn emit_reflection_member_array(
    ctx: &mut FunctionContext<'_>,
    member_class_name: &str,
    members: &[ReflectionListedMember],
) -> Result<()> {
    emit_reflection_indexed_array(ctx, members.len().max(1), 8);
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &PhpType::Object(member_class_name.to_string()),
    );

    for member in members {
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        emit_reflection_member_object(ctx, member_class_name, member)?;
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        emit_append_reflection_member_object(ctx);
    }

    Ok(())
}

/// Allocates and populates one ReflectionMethod/ReflectionProperty object.
fn emit_reflection_member_object(
    ctx: &mut FunctionContext<'_>,
    member_class_name: &str,
    member: &ReflectionListedMember,
) -> Result<()> {
    let (class_id, property_count, uninitialized_marker_offsets) = {
        let class_info =
            ctx.module.class_infos.get(member_class_name).ok_or_else(|| {
                CodegenIrError::unsupported(format!("unknown class {}", member_class_name))
            })?;
        (
            class_info.class_id,
            class_info.properties.len(),
            super::uninitialized_property_marker_offsets(class_info),
        )
    };
    super::emit_object_allocation(
        ctx,
        class_id,
        property_count,
        false,
        &uninitialized_marker_offsets,
    )?;
    let class_info = ctx
        .module
        .class_infos
        .get(member_class_name)
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let name_offset = reflection_property_offset(class_info, "__name")?;
    emit_reflection_string_property(ctx, &member.name, name_offset, name_offset + 8);
    emit_reflection_attrs_property(
        ctx,
        member_class_name,
        &member.attr_names,
        &member.attr_args,
    )?;
    emit_reflection_member_flag_properties(ctx, member_class_name, member.flags)?;
    Ok(())
}

/// Allocates an indexed array for static reflection metadata.
fn emit_reflection_indexed_array(
    ctx: &mut FunctionContext<'_>,
    capacity: usize,
    stride: i64,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", stride);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", stride);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
}

/// Appends the stacked member object to the stacked member array and leaves the array in result.
fn emit_append_reflection_member_object(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x1");
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "rsi");
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
    }
}

/// Allocates an indexed string array containing ReflectionClass metadata names.
fn emit_reflection_string_array(ctx: &mut FunctionContext<'_>, names: &[String]) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", names.len() as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", 16);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", names.len() as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", 16);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_reflection_string_array_fill_aarch64(ctx, names),
        Arch::X86_64 => emit_reflection_string_array_fill_x86_64(ctx, names),
    }
    Ok(())
}

/// Appends ReflectionClass metadata names to the current ARM64 result array.
fn emit_reflection_string_array_fill_aarch64(ctx: &mut FunctionContext<'_>, names: &[String]) {
    ctx.emitter.instruction("str x0, [sp, #-16]!"); // park the metadata-name array while appending strings
    for name in names {
        let (label, len) = ctx.data.add_string(name.as_bytes());
        ctx.emitter.instruction("ldr x0, [sp]"); // reload the metadata-name array for this append
        abi::emit_symbol_address(ctx.emitter, "x1", &label);
        abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        ctx.emitter.instruction("str x0, [sp]"); // preserve the possibly-grown metadata-name array
    }
    ctx.emitter.instruction("ldr x0, [sp], #16"); // restore the final metadata-name array as the result
}

/// Appends ReflectionClass metadata names to the current x86_64 result array.
fn emit_reflection_string_array_fill_x86_64(ctx: &mut FunctionContext<'_>, names: &[String]) {
    ctx.emitter.instruction("push rax"); // park the metadata-name array while appending strings
    ctx.emitter.instruction("sub rsp, 8"); // keep stack alignment stable across append helper calls
    for name in names {
        let (label, len) = ctx.data.add_string(name.as_bytes());
        ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 8]"); // reload the metadata-name array for this append
        abi::emit_symbol_address(ctx.emitter, "rsi", &label);
        abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
        abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax"); // preserve the possibly-grown metadata-name array
    }
    ctx.emitter.instruction("add rsp, 8"); // drop the temporary alignment slot
    ctx.emitter.instruction("pop rax"); // restore the final metadata-name array as the result
}

/// Stores ReflectionMethod/ReflectionProperty boolean predicate slots when supported.
fn emit_reflection_member_flag_properties(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    flags: ReflectionMemberFlags,
) -> Result<()> {
    match class_name {
        "ReflectionMethod" => {
            emit_reflection_owner_bool_property(ctx, class_name, "__is_static", flags.is_static)?;
            emit_reflection_owner_bool_property(ctx, class_name, "__is_public", flags.is_public)?;
            emit_reflection_owner_bool_property(
                ctx,
                class_name,
                "__is_protected",
                flags.is_protected,
            )?;
            emit_reflection_owner_bool_property(ctx, class_name, "__is_private", flags.is_private)?;
            emit_reflection_owner_bool_property(ctx, class_name, "__is_final", flags.is_final)?;
            emit_reflection_owner_bool_property(
                ctx,
                class_name,
                "__is_abstract",
                flags.is_abstract,
            )?;
        }
        "ReflectionProperty" => {
            emit_reflection_owner_bool_property(ctx, class_name, "__is_static", flags.is_static)?;
            emit_reflection_owner_bool_property(ctx, class_name, "__is_public", flags.is_public)?;
            emit_reflection_owner_bool_property(
                ctx,
                class_name,
                "__is_protected",
                flags.is_protected,
            )?;
            emit_reflection_owner_bool_property(ctx, class_name, "__is_private", flags.is_private)?;
        }
        _ => {}
    }
    Ok(())
}

/// Stores one boolean property on the current Reflection owner object result.
fn emit_reflection_owner_bool_property(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    property_name: &str,
    value: bool,
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get(class_name)
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, property_name)?;
    let high_offset = low_offset + 8;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let value_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, value_reg, i64::from(value));
    abi::emit_store_to_address(ctx.emitter, value_reg, result_reg, low_offset);
    abi::emit_store_zero_to_address(ctx.emitter, result_reg, high_offset);
    Ok(())
}

/// Stores one boolean property on the current ReflectionClass object result.
fn emit_reflection_bool_property(
    ctx: &mut FunctionContext<'_>,
    property_name: &str,
    value: bool,
) -> Result<()> {
    emit_reflection_owner_bool_property(ctx, "ReflectionClass", property_name, value)
}

/// Stores one integer property on the current ReflectionClass object result.
fn emit_reflection_int_property(
    ctx: &mut FunctionContext<'_>,
    property_name: &str,
    value: i64,
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get("ReflectionClass")
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, property_name)?;
    let high_offset = low_offset + 8;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let value_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, value_reg, value);
    abi::emit_store_to_address(ctx.emitter, value_reg, result_reg, low_offset);
    abi::emit_store_zero_to_address(ctx.emitter, result_reg, high_offset);
    Ok(())
}

/// Computes PHP's `ReflectionClass::getModifiers()` bitmask for class metadata.
fn reflection_class_modifiers(
    is_final: bool,
    is_abstract: bool,
    is_readonly_class: bool,
    is_enum: bool,
) -> i64 {
    let mut modifiers = 0;
    if is_final {
        modifiers |= 32;
    }
    if is_abstract {
        modifiers |= 64;
    }
    if is_readonly_class && !is_enum {
        modifiers |= 65_536;
    }
    modifiers
}

/// Returns one declared property offset from a synthetic Reflection class layout.
fn reflection_property_offset(info: &crate::types::ClassInfo, property: &str) -> Result<usize> {
    info.property_offsets.get(property).copied().ok_or_else(|| {
        CodegenIrError::invalid_module(format!(
            "Reflection owner missing property offset for ${}",
            property
        ))
    })
}

/// Returns the low/high object offsets for the private `__attrs` slot.
fn reflection_attrs_offsets(ctx: &FunctionContext<'_>, class_name: &str) -> Result<(usize, usize)> {
    let class_info = ctx
        .module
        .class_infos
        .get(class_name)
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let attrs_low_offset = reflection_property_offset(class_info, "__attrs")?;
    Ok((attrs_low_offset, attrs_low_offset + 8))
}
