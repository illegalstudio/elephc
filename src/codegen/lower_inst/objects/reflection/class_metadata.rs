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
    reflection_enum_metadata_for_name(ctx, &reflected_enum)
}

/// Resolves ReflectionEnum metadata for a class-like name selected at runtime.
pub(super) fn reflection_enum_metadata_for_name(
    ctx: &FunctionContext<'_>,
    reflected_enum: &str,
) -> Result<ReflectionOwnerMetadata> {
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
    // `Closure` has no declaration to reflect. A program that names `fromCallable` at run time
    // carries `__ElephcClosureFactory` (`crate::closure_from_callable_prelude`), whose static
    // `fromCallable()` is the one `Closure` member such a program reflects: Symfony's container
    // checks its `['Closure', 'fromCallable']` factories through `ReflectionClass('Closure')`.
    if reflected_class.trim_start_matches('\\').eq_ignore_ascii_case("Closure") {
        let factory = crate::closure_from_callable_prelude::CLOSURE_FACTORY_CLASS;
        if resolve_reflection_class(ctx, factory).is_some() {
            let mut metadata = reflection_class_metadata_for_name(ctx, factory)?;
            metadata.reflected_name = Some("Closure".to_string());
            metadata.is_final = true;
            metadata.is_instantiable = false;
            // php's own signature, `Closure::fromCallable(callable $callback): Closure`: the helper
            // declares `mixed` so the conversion keeps its scope (see the prelude).
            let named = |name: &str, is_builtin: bool| {
                Some(super::ReflectionParameterTypeMetadata::Named(super::ReflectionNamedTypeMetadata {
                    name: name.to_string(),
                    allows_null: false,
                    is_builtin,
                }))
            };
            for method in &mut metadata.method_members {
                method.declaring_class_name = Some("Closure".to_string());
                method.type_metadata = named("Closure", false);
                for parameter in &mut method.parameters {
                    parameter.declaring_class_name = Some("Closure".to_string());
                    parameter.type_metadata = named("callable", true);
                    parameter.is_callable_type = true;
                    if let Some(super::ReflectionDeclaringFunctionMember::Method {
                        declaring_class_name,
                        type_metadata,
                        ..
                    }) = &mut parameter.declaring_function
                    {
                        *declaring_class_name = Some("Closure".to_string());
                        *type_metadata = named("Closure", false);
                    }
                }
            }
            return Ok(metadata);
        }
    }
    if let Some((class_name, info)) = resolve_reflection_class(ctx, &reflected_class) {
        let is_enum = is_reflection_enum(ctx, class_name);
        let method_names = reflection_class_method_names(ctx, class_name);
        let property_names = reflection_class_property_names(ctx, class_name, info);
        let constant_names = reflection_class_constant_names(ctx, class_name, info);
        let constant_members = reflection_class_constant_members(ctx, class_name, info)?;
        let default_property_members =
            reflection_class_default_property_members(ctx, info, &property_names);
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
        let constructor_member = match reflection_constructor_member(&method_members) {
            Some(member) => Some(member),
            None => inherited_private_constructor_member(ctx, class_name)?,
        };
        let is_instantiable =
            reflection_class_is_instantiable(info, is_enum, constructor_member.as_ref());
        let is_cloneable = reflection_class_is_cloneable(class_name, info, is_enum, &ctx.module.class_infos);
        let is_iterable = reflection_class_is_iterable(info, is_enum);
        return Ok(ReflectionOwnerMetadata {
            rendered_to_string: None,
            reflected_name: Some(class_name.to_string()),
            doc_comment: info.doc_comment.clone(),
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
        returns_reference: false,
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
            rendered_to_string: None,
            reflected_name: Some(interface_name.to_string()),
            // `debug:autowiring` prints each interface's first doc line ("Describes a logger
            // instance." for `Psr\Log\LoggerInterface`).
            doc_comment: ctx
                .module
                .interface_infos
                .get(interface_name)
                .and_then(|info| info.doc_comment.clone()),
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
        returns_reference: false,
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
            rendered_to_string: None,
            reflected_name: Some(trait_name.to_string()),
            doc_comment: None,
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
        returns_reference: false,
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

/// Resolves class metadata for nested declaring-class slots without recursive member objects.
pub(super) fn reflection_shallow_class_metadata_for_name(
    ctx: &FunctionContext<'_>,
    reflected_class: &str,
) -> Result<ReflectionOwnerMetadata> {
    let metadata = reflection_direct_shallow_class_metadata_for_name(ctx, reflected_class)?;
    #[cfg(test)]
    assert_eq!(
        metadata,
        reflection_legacy_shallow_class_metadata_for_name(ctx, reflected_class)?,
        "direct shallow Reflection metadata diverged for {reflected_class}"
    );
    Ok(metadata)
}

/// Resolves only the declaration identity needed for a source-file ReflectionObject query.
pub(super) fn reflection_source_file_class_metadata_for_name(
    ctx: &FunctionContext<'_>,
    reflected_class: &str,
) -> Result<ReflectionOwnerMetadata> {
    let Some((class_name, _)) = resolve_reflection_class(ctx, reflected_class) else {
        return Ok(empty_reflection_metadata());
    };
    let mut metadata = empty_reflection_metadata();
    metadata.reflected_name = Some(class_name.to_string());
    metadata.doc_comment = ctx
        .module
        .class_infos
        .get(class_name)
        .and_then(|class| class.doc_comment.clone());
    Ok(metadata)
}

/// Resolves shallow `ReflectionEnum` metadata for nested enum-case slots.
pub(super) fn reflection_shallow_enum_metadata_for_name(
    ctx: &FunctionContext<'_>,
    reflected_enum: &str,
) -> Result<ReflectionOwnerMetadata> {
    let mut metadata = reflection_direct_shallow_class_metadata_for_name(ctx, reflected_enum)?;
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
    #[cfg(test)]
    {
        let mut expected =
            reflection_legacy_shallow_class_metadata_for_name(ctx, reflected_enum)?;
        expected.type_metadata = enum_info
            .backing_type
            .as_ref()
            .and_then(reflection_named_type_metadata)
            .map(ReflectionParameterTypeMetadata::Named);
        assert_eq!(
            metadata, expected,
            "direct shallow ReflectionEnum metadata diverged for {reflected_enum}"
        );
    }
    Ok(metadata)
}

/// Recreates the former full-then-clear projection for exact test-only equivalence checks.
#[cfg(test)]
fn reflection_legacy_shallow_class_metadata_for_name(
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

/// Builds only the fields retained by nested shallow class-like Reflection objects.
pub(super) fn reflection_direct_shallow_class_metadata_for_name(
    ctx: &FunctionContext<'_>,
    reflected_class: &str,
) -> Result<ReflectionOwnerMetadata> {
    if let Some((class_name, info)) = resolve_reflection_class(ctx, reflected_class) {
        let is_enum = is_reflection_enum(ctx, class_name);
        let property_names = reflection_class_property_names(ctx, class_name, info);
        let constructor_member = reflection_class_method_member(
            ctx,
            class_name,
            info,
            &php_symbol_key("__construct"),
        )?;
        let mut metadata = empty_reflection_metadata();
        metadata.reflected_name = Some(class_name.to_string());
        metadata.doc_comment = info.doc_comment.clone();
        metadata.attr_names = info.attribute_names.clone();
        metadata.attr_args = info.attribute_args.clone();
        metadata.interface_names = info.interfaces.clone();
        metadata.trait_names = info.used_traits.clone();
        metadata.trait_aliases = info.trait_aliases.clone();
        metadata.parent_names = reflection_parent_class_names(ctx, info);
        metadata.default_property_members =
            reflection_class_default_property_members(ctx, info, &property_names);
        metadata.static_property_members = reflection_class_static_property_members(class_name, info);
        metadata.is_final = info.is_final;
        metadata.is_abstract = info.is_abstract;
        metadata.is_enum = is_enum;
        metadata.is_readonly = info.is_readonly_class && !is_enum;
        metadata.is_anonymous = is_reflection_anonymous_class_name(class_name);
        metadata.is_instantiable =
            reflection_class_is_instantiable(info, is_enum, constructor_member.as_ref());
        metadata.is_cloneable = reflection_class_is_cloneable(class_name, info, is_enum, &ctx.module.class_infos);
        metadata.is_iterable = reflection_class_is_iterable(info, is_enum);
        metadata.modifiers = reflection_class_modifiers(
            info.is_final,
            info.is_abstract,
            info.is_readonly_class,
            is_enum,
        );
        return Ok(metadata);
    }

    if let Some(interface_name) = resolve_reflection_interface(ctx, reflected_class) {
        let mut metadata = empty_reflection_metadata();
        metadata.reflected_name = Some(interface_name.to_string());
        metadata.interface_names = reflection_interface_parent_names(ctx, interface_name);
        metadata.is_interface = true;
        metadata.member_flags = reflection_member_flags(
            false,
            &Visibility::Public,
            false,
            false,
            false,
            false,
        );
        return Ok(metadata);
    }

    if let Some(trait_name) = resolve_reflection_trait(ctx, reflected_class) {
        let mut metadata = empty_reflection_metadata();
        metadata.reflected_name = Some(trait_name.to_string());
        metadata.trait_names = ctx
            .module
            .declared_trait_uses
            .get(trait_name)
            .cloned()
            .unwrap_or_default();
        metadata.is_trait = true;
        metadata.member_flags = reflection_member_flags(
            false,
            &Visibility::Public,
            false,
            false,
            false,
            false,
        );
        return Ok(metadata);
    }

    Ok(empty_reflection_metadata())
}
