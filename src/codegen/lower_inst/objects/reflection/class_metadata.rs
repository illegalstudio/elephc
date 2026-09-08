//! Purpose:
//! ReflectionClass and ReflectionEnum metadata resolution.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;

/// Resolves Reflection constructor operands to captured class/member metadata.
pub(super) fn reflection_owner_metadata(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    match class_name {
        "ReflectionClass" => reflection_class_metadata(ctx, inst),
        "ReflectionEnum" => reflection_enum_metadata(ctx, inst),
        "ReflectionFunction" => reflection_function_metadata(ctx, inst),
        "ReflectionMethod" => reflection_method_metadata(ctx, inst),
        "ReflectionProperty" => reflection_property_metadata(ctx, inst),
        "ReflectionParameter" => reflection_parameter_metadata(ctx, inst),
        "ReflectionClassConstant" => reflection_class_constant_metadata(ctx, inst),
        "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase" => {
            reflection_enum_case_metadata(ctx, class_name, inst)
        }
        _ => Ok(empty_reflection_metadata()),
    }
}

/// Resolves `ReflectionClass(class)` metadata.
pub(super) fn reflection_class_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    let Some(class_operand) = inst.operands.first().copied() else {
        return Ok(empty_reflection_metadata());
    };
    let reflected_class = const_string_or_class_operand(ctx, class_operand, "ReflectionClass")?;
    reflection_class_metadata_for_name(ctx, &reflected_class)
}

/// Resolves `ReflectionEnum(enum)` metadata for a known enum name.
pub(super) fn reflection_enum_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<ReflectionOwnerMetadata> {
    let Some(enum_operand) = inst.operands.first().copied() else {
        return Ok(empty_reflection_metadata());
    };
    let reflected_enum = const_string_or_class_operand(ctx, enum_operand, "ReflectionEnum")?;
    let mut metadata = reflection_class_metadata_for_name(ctx, &reflected_enum)?;
    let Some(enum_name) = metadata.reflected_name.as_deref() else {
        return Ok(empty_reflection_metadata());
    };
    let Some(enum_info) = ctx.module.enum_infos.get(enum_name) else {
        return Ok(empty_reflection_metadata());
    };
    metadata.type_metadata = enum_info
        .backing_type
        .as_ref()
        .and_then(reflection_named_type_metadata)
        .map(ReflectionParameterTypeMetadata::Named);
    Ok(metadata)
}

/// Resolves `ReflectionClass(name)` metadata for a known class-like name.
pub(super) fn reflection_class_metadata_for_name(
    ctx: &FunctionContext<'_>,
    reflected_class: &str,
) -> Result<ReflectionOwnerMetadata> {
    if let Some((class_name, info)) = resolve_reflection_class(ctx, &reflected_class) {
        let is_enum = is_reflection_enum(ctx, class_name);
        let method_names = reflection_class_method_names(ctx, class_name);
        let property_names = reflection_class_property_names(ctx, class_name, info);
        let constant_names = reflection_class_constant_names(ctx, class_name, info);
        let constant_members = reflection_class_constant_members(ctx, class_name, info)?;
        let default_property_members =
            reflection_class_default_property_members(info, &property_names);
        let static_property_members = reflection_class_static_property_members(class_name, info);
        let constant_reflection_members =
            reflection_class_constant_reflection_members(ctx, class_name, info)?;
        let enum_case_members = if is_enum {
            reflection_enum_case_members(ctx, class_name)
        } else {
            Vec::new()
        };
        let method_members = reflection_class_method_members(ctx, class_name, info, &method_names)?;
        let property_members =
            reflection_class_property_members(ctx, class_name, info, &property_names);
        let constructor_member = reflection_constructor_member(&method_members);
        let is_instantiable =
            reflection_class_is_instantiable(info, is_enum, constructor_member.as_ref());
        let is_cloneable = reflection_class_is_cloneable(class_name, info, is_enum);
        let is_iterable = reflection_class_is_iterable(info, is_enum);
        return Ok(ReflectionOwnerMetadata {
            reflected_name: Some(class_name.to_string()),
            attr_names: info.attribute_names.clone(),
            attr_args: info.attribute_args.clone(),
            interface_names: info.interfaces.clone(),
            trait_names: info.used_traits.clone(),
            trait_aliases: info.trait_aliases.clone(),
            parent_names: reflection_parent_class_names(ctx, info),
            method_names,
            property_names,
            constant_names,
            constant_members,
            default_property_members,
            static_property_members,
            constant_reflection_members,
            enum_case_members,
            method_members,
            property_members,
            property_hook_members: Vec::new(),
            constructor_member,
            parent_class_name: reflection_parent_class_name(ctx, info),
            constant_value: None,
            backing_value: None,
            is_enum_case: false,
            parameter_members: Vec::new(),
            type_metadata: None,
            property_default_value: None,
            required_parameter_count: 0,
            is_deprecated: false,
            is_generator: false,
            prototype_member: None,
            is_final: info.is_final,
            is_abstract: info.is_abstract,
            is_interface: false,
            is_trait: false,
            is_enum,
            is_readonly: info.is_readonly_class && !is_enum,
            is_anonymous: is_reflection_anonymous_class_name(class_name),
            is_instantiable,
            is_cloneable,
            is_iterable,
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
        let method_names = reflection_interface_method_names(ctx, interface_name);
        let property_names = reflection_interface_property_names(ctx, interface_name);
        let constant_names = reflection_interface_constant_names(ctx, interface_name);
        let constant_members = reflection_interface_constant_members(ctx, interface_name)?;
        let constant_reflection_members =
            reflection_interface_constant_reflection_members(ctx, interface_name)?;
        let method_members = ctx
            .module
            .interface_infos
            .get(interface_name)
            .map(|info| {
                reflection_interface_method_members(ctx, info, interface_name, &method_names)
            })
            .transpose()?
            .unwrap_or_else(|| default_method_members(&method_names, true, interface_name));
        let property_members = default_property_members(&property_names, true, interface_name);
        let constructor_member = reflection_constructor_member(&method_members);
        return Ok(ReflectionOwnerMetadata {
            reflected_name: Some(interface_name.to_string()),
            attr_names: Vec::new(),
            attr_args: Vec::new(),
            interface_names: reflection_interface_parent_names(ctx, interface_name),
            trait_names: Vec::new(),
            trait_aliases: Vec::new(),
            parent_names: Vec::new(),
            method_names,
            property_names,
            constant_names,
            constant_members,
            default_property_members: Vec::new(),
            static_property_members: Vec::new(),
            constant_reflection_members,
            enum_case_members: Vec::new(),
            method_members,
            property_members,
            property_hook_members: Vec::new(),
            constructor_member,
            parent_class_name: None,
            constant_value: None,
            backing_value: None,
            is_enum_case: false,
            parameter_members: Vec::new(),
            type_metadata: None,
            property_default_value: None,
            required_parameter_count: 0,
            is_deprecated: false,
            is_generator: false,
            prototype_member: None,
            is_final: false,
            is_abstract: false,
            is_interface: true,
            is_trait: false,
            is_enum: false,
            is_readonly: false,
            is_anonymous: false,
            is_instantiable: false,
            is_cloneable: false,
            is_iterable: false,
            modifiers: 0,
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
    if let Some(trait_name) = resolve_reflection_trait(ctx, &reflected_class) {
        let trait_names = ctx
            .module
            .declared_trait_uses
            .get(trait_name)
            .cloned()
            .unwrap_or_default();
        let method_names = reflection_trait_method_names(ctx, trait_name);
        let property_names = reflection_trait_property_names(ctx, trait_name);
        let constant_names = reflection_trait_constant_names(ctx, trait_name);
        let constant_members = reflection_trait_constant_members(ctx, trait_name)?;
        let constant_reflection_members =
            reflection_trait_constant_reflection_members(ctx, trait_name)?;
        let method_members = ctx
            .module
            .declared_trait_methods
            .get(trait_name)
            .map(|methods| reflection_trait_method_members(ctx, methods, trait_name, &method_names))
            .transpose()?
            .unwrap_or_else(|| default_method_members(&method_names, false, trait_name));
        let property_members = default_property_members(&property_names, false, trait_name);
        let constructor_member = reflection_constructor_member(&method_members);
        return Ok(ReflectionOwnerMetadata {
            reflected_name: Some(trait_name.to_string()),
            attr_names: Vec::new(),
            attr_args: Vec::new(),
            interface_names: Vec::new(),
            trait_names,
            trait_aliases: Vec::new(),
            parent_names: Vec::new(),
            method_names,
            property_names,
            constant_names,
            constant_members,
            default_property_members: Vec::new(),
            static_property_members: Vec::new(),
            constant_reflection_members,
            enum_case_members: Vec::new(),
            method_members,
            property_members,
            property_hook_members: Vec::new(),
            constructor_member,
            parent_class_name: None,
            constant_value: None,
            backing_value: None,
            is_enum_case: false,
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
            is_trait: true,
            is_enum: false,
            is_readonly: false,
            is_anonymous: false,
            is_instantiable: false,
            is_cloneable: false,
            is_iterable: false,
            modifiers: 0,
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
    Ok(empty_reflection_metadata())
}

/// Builds php-src's state-dependent dynamic property surface for `DateInterval` objects.
pub(super) fn reflection_date_interval_object_metadata(
    ctx: &FunctionContext<'_>,
    from_string: bool,
) -> Result<ReflectionOwnerMetadata> {
    let mut metadata = reflection_class_metadata_for_name(ctx, "DateInterval")?;
    let property_names = if from_string {
        vec!["from_string", "date_string"]
    } else {
        vec![
            "y",
            "m",
            "d",
            "h",
            "i",
            "s",
            "f",
            "invert",
            "days",
            "from_string",
        ]
    };
    metadata.property_names = property_names
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    metadata.property_members = metadata
        .property_names
        .iter()
        .map(|name| reflection_date_interval_dynamic_property_member(name))
        .collect();
    metadata.default_property_members.clear();
    Ok(metadata)
}

/// Builds one untyped public dynamic `ReflectionProperty` owned by `DateInterval`.
fn reflection_date_interval_dynamic_property_member(
    property_name: &str,
) -> ReflectionListedMember {
    let mut flags =
        reflection_member_flags(false, &Visibility::Public, false, false, false, false);
    flags.is_dynamic = true;
    ReflectionListedMember {
        name: property_name.to_string(),
        declaring_class_name: Some("DateInterval".to_string()),
        attr_names: Vec::new(),
        attr_args: Vec::new(),
        constant_value: None,
        backing_value: None,
        is_enum_case: false,
        flags,
        modifiers: reflection_property_modifiers_from_flags(flags),
        type_metadata: None,
        default_value: None,
        property_hook_members: Vec::new(),
        required_parameter_count: 0,
        is_deprecated: false,
        is_generator: false,
        prototype_member: None,
        parameters: Vec::new(),
    }
}

/// Resolves class metadata for nested declaring-class slots without recursive member objects.
pub(super) fn reflection_shallow_class_metadata_for_name(
    ctx: &FunctionContext<'_>,
    reflected_class: &str,
) -> Result<ReflectionOwnerMetadata> {
    let mut metadata = reflection_class_metadata_for_name(ctx, reflected_class)?;
    metadata.method_names.clear();
    metadata.property_names.clear();
    metadata.constant_names.clear();
    metadata.constant_members.clear();
    metadata.constant_reflection_members.clear();
    metadata.enum_case_members.clear();
    metadata.method_members.clear();
    metadata.property_members.clear();
    metadata.constructor_member = None;
    metadata.parent_class_name = None;
    Ok(metadata)
}

/// Resolves `ReflectionEnum` metadata for nested enum-case slots.
pub(super) fn reflection_enum_metadata_for_name(
    ctx: &FunctionContext<'_>,
    reflected_enum: &str,
) -> Result<ReflectionOwnerMetadata> {
    let mut metadata = reflection_class_metadata_for_name(ctx, reflected_enum)?;
    let Some(enum_name) = metadata.reflected_name.as_deref() else {
        return Ok(empty_reflection_metadata());
    };
    let Some(enum_info) = ctx.module.enum_infos.get(enum_name) else {
        return Ok(empty_reflection_metadata());
    };
    metadata.type_metadata = enum_info
        .backing_type
        .as_ref()
        .and_then(reflection_named_type_metadata)
        .map(ReflectionParameterTypeMetadata::Named);
    metadata.method_names.clear();
    metadata.property_names.clear();
    metadata.constant_names.clear();
    metadata.constant_members.clear();
    metadata.constant_reflection_members.clear();
    metadata.enum_case_members.clear();
    metadata.method_members.clear();
    metadata.property_members.clear();
    metadata.constructor_member = None;
    metadata.parent_class_name = None;
    Ok(metadata)
}
