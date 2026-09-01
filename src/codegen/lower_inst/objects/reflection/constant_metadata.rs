//! Purpose:
//! Reflection class constant, enum case, and class-like lookup metadata.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;

/// Resolves `ReflectionClassConstant(class, constant)` metadata.
pub(super) fn reflection_class_constant_metadata(
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
    if let Some((enum_name, case)) =
        resolve_reflection_enum_case(ctx, &reflected_class, &constant_name)
    {
        return Ok(ReflectionOwnerMetadata {
            reflected_name: Some(constant_name.clone()),
            attr_names: case.attribute_names.clone(),
            attr_args: case.attribute_args.clone(),
            interface_names: Vec::new(),
            trait_names: Vec::new(),
            trait_aliases: Vec::new(),
            parent_names: Vec::new(),
            method_names: Vec::new(),
            property_names: Vec::new(),
            constant_names: Vec::new(),
            constant_members: Vec::new(),
            default_property_members: Vec::new(),
            static_property_members: Vec::new(),
            constant_reflection_members: Vec::new(),
            enum_case_members: Vec::new(),
            method_members: Vec::new(),
            property_members: Vec::new(),
            property_hook_members: Vec::new(),
            constructor_member: None,
            parent_class_name: Some(enum_name.to_string()),
            constant_value: Some(ReflectionConstantValue::EnumCase {
                enum_name: enum_name.to_string(),
                case_name: constant_name.clone(),
            }),
            backing_value: None,
            is_enum_case: true,
            parameter_members: Vec::new(),
            type_metadata: None,
            property_default_value: None,
            required_parameter_count: 0,
            is_deprecated: false,
            is_generator: false,
            prototype_member: None,
            is_final: false,
            is_abstract: false,
            is_interface: false,
            is_trait: false,
            is_enum: false,
            is_readonly: false,
            is_anonymous: false,
            is_instantiable: false,
            is_cloneable: false,
            is_iterable: false,
            modifiers: reflection_class_constant_modifiers(&Visibility::Public, false),
            member_flags: reflection_member_flags(
                false,
                &Visibility::Public,
                false,
                false,
                false,
                false,
            ),
        });
    }
    Ok(
        reflection_class_constant_lookup(ctx, &reflected_class, &constant_name)?
            .map(|metadata| reflection_class_constant_owner_metadata(constant_name, metadata))
            .unwrap_or_else(empty_reflection_metadata),
    )
}

/// Resolves `ReflectionEnumUnitCase/BackedCase(enum, case)` metadata.
pub(super) fn reflection_enum_case_metadata(
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
            .map(|(enum_name, case)| ReflectionOwnerMetadata {
                reflected_name: Some(case_name.clone()),
                attr_names: case.attribute_names.clone(),
                attr_args: case.attribute_args.clone(),
                interface_names: Vec::new(),
                trait_names: Vec::new(),
                trait_aliases: Vec::new(),
                parent_names: Vec::new(),
                method_names: Vec::new(),
                property_names: Vec::new(),
                constant_names: Vec::new(),
                constant_members: Vec::new(),
                default_property_members: Vec::new(),
                static_property_members: Vec::new(),
                constant_reflection_members: Vec::new(),
                enum_case_members: Vec::new(),
                method_members: Vec::new(),
                property_members: Vec::new(),
                property_hook_members: Vec::new(),
                constructor_member: None,
                parent_class_name: Some(enum_name.to_string()),
                constant_value: Some(ReflectionConstantValue::EnumCase {
                    enum_name: enum_name.to_string(),
                    case_name: case_name.clone(),
                }),
                backing_value: reflection_enum_case_backing_value(case),
                is_enum_case: true,
                parameter_members: Vec::new(),
                type_metadata: None,
                property_default_value: None,
                required_parameter_count: 0,
                is_deprecated: false,
                is_generator: false,
                prototype_member: None,
                is_final: false,
                is_abstract: false,
                is_interface: false,
                is_trait: false,
                is_enum: false,
                is_readonly: false,
                is_anonymous: false,
                is_instantiable: false,
                is_cloneable: false,
                is_iterable: false,
                modifiers: reflection_class_constant_modifiers(&Visibility::Public, false),
                member_flags: reflection_member_flags(
                    false,
                    &Visibility::Public,
                    false,
                    false,
                    false,
                    false,
                ),
            })
            .unwrap_or_else(empty_reflection_metadata),
    )
}

/// Builds owner metadata for one resolved class/interface/trait/enum constant reflector.
pub(super) fn reflection_class_constant_owner_metadata(
    reflected_name: String,
    metadata: ReflectionClassConstantMetadata,
) -> ReflectionOwnerMetadata {
    let is_final = metadata.is_final;
    let is_deprecated = metadata
        .attr_names
        .iter()
        .any(|name| php_symbol_key(name.trim_start_matches('\\')) == "deprecated");
    let modifiers = reflection_class_constant_modifiers(&metadata.visibility, is_final);
    let member_flags =
        reflection_member_flags(false, &metadata.visibility, is_final, false, false, false);
    ReflectionOwnerMetadata {
        reflected_name: Some(reflected_name),
        attr_names: metadata.attr_names,
        attr_args: metadata.attr_args,
        interface_names: Vec::new(),
        trait_names: Vec::new(),
        trait_aliases: Vec::new(),
        parent_names: Vec::new(),
        method_names: Vec::new(),
        property_names: Vec::new(),
        constant_names: Vec::new(),
        constant_members: Vec::new(),
        default_property_members: Vec::new(),
        static_property_members: Vec::new(),
        constant_reflection_members: Vec::new(),
        enum_case_members: Vec::new(),
        method_members: Vec::new(),
        property_members: Vec::new(),
        property_hook_members: Vec::new(),
        constructor_member: None,
        parent_class_name: Some(metadata.declaring_class_name),
        constant_value: Some(metadata.value),
        backing_value: None,
        is_enum_case: false,
        parameter_members: Vec::new(),
        type_metadata: metadata.type_metadata,
        property_default_value: None,
        required_parameter_count: 0,
        is_deprecated,
        is_generator: false,
        prototype_member: None,
        is_final,
        is_abstract: false,
        is_interface: false,
        is_trait: false,
        is_enum: false,
        is_readonly: false,
        is_anonymous: false,
        is_instantiable: false,
        is_cloneable: false,
        is_iterable: false,
        modifiers,
        member_flags,
    }
}

/// Resolves static metadata for a direct `ReflectionClassConstant` constructor call.
pub(super) fn reflection_class_constant_lookup(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    constant_name: &str,
) -> Result<Option<ReflectionClassConstantMetadata>> {
    if let Some((declaring_class_name, info)) =
        resolve_reflection_class_constant(ctx, class_name, constant_name)
    {
        let Some(value_expr) = info.constants.get(constant_name) else {
            return Ok(None);
        };
        let value =
            reflection_constant_value(ctx, declaring_class_name, Some(info), value_expr, 0)?;
        return Ok(Some(ReflectionClassConstantMetadata {
            declaring_class_name: declaring_class_name.to_string(),
            attr_names: info
                .constant_attribute_names
                .get(constant_name)
                .cloned()
                .unwrap_or_default(),
            attr_args: info
                .constant_attribute_args
                .get(constant_name)
                .cloned()
                .unwrap_or_default(),
            value,
            type_metadata: info
                .constant_types
                .get(constant_name)
                .and_then(reflection_declared_type_metadata),
            visibility: info
                .constant_visibilities
                .get(constant_name)
                .cloned()
                .unwrap_or(Visibility::Public),
            is_final: info.final_constants.contains(constant_name),
        }));
    }
    if let Some((_, class_info)) = resolve_reflection_class(ctx, class_name) {
        for interface_name in &class_info.interfaces {
            if let Some(metadata) =
                reflection_interface_class_constant_lookup(ctx, interface_name, constant_name)?
            {
                return Ok(Some(metadata));
            }
        }
    }
    if let Some(interface_name) = resolve_reflection_interface(ctx, class_name) {
        if let Some(metadata) =
            reflection_interface_class_constant_lookup(ctx, interface_name, constant_name)?
        {
            return Ok(Some(metadata));
        }
    }
    if let Some(trait_name) = resolve_reflection_trait(ctx, class_name) {
        if let Some(value_expr) = ctx
            .module
            .declared_trait_constants
            .get(trait_name)
            .and_then(|constants| constants.get(constant_name))
        {
            let is_final = ctx
                .module
                .declared_trait_final_constants
                .get(trait_name)
                .is_some_and(|constants| constants.contains(constant_name));
            let value = reflection_constant_value(ctx, trait_name, None, value_expr, 0)?;
            return Ok(Some(ReflectionClassConstantMetadata {
                declaring_class_name: trait_name.to_string(),
                attr_names: Vec::new(),
                attr_args: Vec::new(),
                value,
                type_metadata: ctx
                    .module
                    .declared_trait_constant_types
                    .get(trait_name)
                    .and_then(|types| types.get(constant_name))
                    .and_then(reflection_declared_type_metadata),
                visibility: ctx
                    .module
                    .declared_trait_constant_visibilities
                    .get(trait_name)
                    .and_then(|constants| constants.get(constant_name))
                    .cloned()
                    .unwrap_or(Visibility::Public),
                is_final,
            }));
        }
    }
    Ok(None)
}

/// Resolves interface constant metadata with the original declaring interface preserved.
pub(super) fn reflection_interface_class_constant_lookup(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    constant_name: &str,
) -> Result<Option<ReflectionClassConstantMetadata>> {
    let Some(interface_name) = resolve_reflection_interface(ctx, interface_name) else {
        return Ok(None);
    };
    let Some(info) = ctx.module.interface_infos.get(interface_name) else {
        return Ok(None);
    };
    let Some(value_expr) = info.constants.get(constant_name) else {
        return Ok(None);
    };
    let declaring_interface =
        interface_constant_declaring_interface(info, interface_name, constant_name);
    let declaring_info = ctx.module.interface_infos.get(declaring_interface);
    let is_final =
        declaring_info.is_some_and(|info| info.final_constants.contains(constant_name));
    let value = reflection_constant_value(ctx, declaring_interface, None, value_expr, 0)?;
    Ok(Some(ReflectionClassConstantMetadata {
        declaring_class_name: declaring_interface.to_string(),
        attr_names: declaring_info
            .and_then(|info| info.constant_attribute_names.get(constant_name))
            .cloned()
            .unwrap_or_default(),
        attr_args: declaring_info
            .and_then(|info| info.constant_attribute_args.get(constant_name))
            .cloned()
            .unwrap_or_default(),
        value,
        type_metadata: declaring_info
            .and_then(|info| info.constant_types.get(constant_name))
            .and_then(reflection_declared_type_metadata),
        visibility: Visibility::Public,
        is_final,
    }))
}

/// Returns the interface that originally declared a visible interface constant.
pub(super) fn interface_constant_declaring_interface<'a>(
    info: &'a InterfaceInfo,
    fallback_interface: &'a str,
    constant_name: &str,
) -> &'a str {
    info.constant_declaring_interfaces
        .get(constant_name)
        .map(String::as_str)
        .unwrap_or(fallback_interface)
}

/// Looks up class metadata by PHP-style case-insensitive name.
pub(super) fn resolve_reflection_class<'a>(
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

/// Returns true when a class name uses the parser's anonymous-class synthetic prefix.
pub(super) fn is_reflection_anonymous_class_name(class_name: &str) -> bool {
    class_name
        .trim_start_matches('\\')
        .starts_with("class@anonymous#")
}

/// Looks up interface metadata by PHP-style case-insensitive name.
pub(super) fn resolve_reflection_interface<'a>(
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
pub(super) fn resolve_reflection_trait<'a>(ctx: &'a FunctionContext<'_>, trait_name: &str) -> Option<&'a str> {
    let trait_key = php_symbol_key(trait_name.trim_start_matches('\\'));
    ctx.module
        .trait_table
        .names
        .iter()
        .find(|candidate| php_symbol_key(candidate.trim_start_matches('\\')) == trait_key)
        .map(String::as_str)
}

/// Looks up enum metadata by PHP-style case-insensitive name.
pub(super) fn is_reflection_enum(ctx: &FunctionContext<'_>, enum_name: &str) -> bool {
    let enum_key = php_symbol_key(enum_name.trim_start_matches('\\'));
    ctx.module
        .enum_infos
        .keys()
        .any(|candidate| php_symbol_key(candidate.trim_start_matches('\\')) == enum_key)
}
