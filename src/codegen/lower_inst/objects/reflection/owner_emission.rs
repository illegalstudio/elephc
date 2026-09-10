//! Purpose:
//! Reflection owner object allocation and core property population.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;

/// Allocates and populates one builtin Reflection owner object from metadata.
pub(super) fn emit_reflection_owner_object(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    metadata: &ReflectionOwnerMetadata,
) -> Result<()> {
    let is_reflection_class_owner = matches!(class_name, "ReflectionClass" | "ReflectionObject");
    let (class_id, property_count, uninitialized_marker_offsets) = {
        let class_info = ctx
            .module
            .class_infos
            .get(class_name)
            .ok_or_else(|| CodegenIrError::unsupported(format!("unknown class {}", class_name)))?;
        (
            class_info.class_id,
            class_info.properties.len(),
            super::super::uninitialized_property_marker_offsets(class_info),
        )
    };
    super::super::emit_object_allocation(
        ctx,
        class_id,
        property_count,
        false,
        &uninitialized_marker_offsets,
        &[],
    )?;
    if let Some(reflected_name) = metadata.reflected_name.as_deref() {
        emit_reflection_owner_string_property_by_name(ctx, class_name, "__name", reflected_name)?;
        if class_name == "ReflectionExtension" {
            let extension = crate::internal_extensions::registry()
                .extension(reflected_name)
                .ok_or_else(|| {
                    CodegenIrError::unsupported(format!(
                        "ReflectionExtension metadata for unknown extension {}",
                        reflected_name
                    ))
                })?;
            let version = extension.version.as_deref().ok_or_else(|| {
                CodegenIrError::unsupported(format!(
                    "ReflectionExtension::getVersion has no version for {}",
                    reflected_name
                ))
            })?;
            emit_reflection_owner_string_property_by_name(
                ctx,
                class_name,
                "__version",
                version,
            )?;
            let info = reflection_extension_info(reflected_name).ok_or_else(|| {
                CodegenIrError::unsupported(format!(
                    "ReflectionExtension::info has no module block for {}",
                    reflected_name
                ))
            })?;
            emit_reflection_owner_string_property_by_name(ctx, class_name, "__info", info)?;
            emit_reflection_owner_bool_property(ctx, class_name, "__is_persistent", true)?;
            emit_reflection_owner_bool_property(ctx, class_name, "__is_temporary", false)?;
            emit_reflection_owner_string_array_property_by_name(
                ctx,
                class_name,
                "__class_names",
                &metadata.interface_names,
            )?;
            let enum_names = extension
                .classes
                .iter()
                .filter(|class| class.enum_type)
                .map(|class| class.exported_name.clone())
                .collect::<Vec<_>>();
            emit_reflection_owner_string_array_property_by_name(
                ctx,
                class_name,
                "__enum_names",
                &enum_names,
            )?;
            let function_names = extension
                .functions
                .iter()
                .map(|function| function.exported_name.clone())
                .collect::<Vec<_>>();
            emit_reflection_owner_string_array_property_by_name(
                ctx,
                class_name,
                "__function_names",
                &function_names,
            )?;
            emit_reflection_constant_array_property_by_name(
                ctx,
                class_name,
                "__constants",
                &reflection_extension_constant_members(reflected_name)?,
            )?;
            emit_reflection_constant_array_property_by_name(
                ctx,
                class_name,
                "__ini_entries",
                &[],
            )?;
            let dependencies = reflection_extension_dependencies(reflected_name)
                .ok_or_else(|| {
                    CodegenIrError::unsupported(format!(
                        "ReflectionExtension::getDependencies for unknown extension {}",
                        reflected_name
                    ))
                })?
                .iter()
                .map(|(name, kind)| ((*name).to_string(), (*kind).to_string()))
                .collect::<Vec<_>>();
            emit_reflection_string_assoc_property_by_name(
                ctx,
                class_name,
                "__dependencies",
                &dependencies,
            )?;
        }
        if is_reflection_class_owner || class_name == "ReflectionEnum" {
            emit_reflection_class_name_parts(ctx, class_name, reflected_name)?;
        }
        if is_reflection_class_owner || class_name == "ReflectionEnum" {
            emit_reflection_owner_string_array_property_by_name(
                ctx,
                class_name,
                "__interface_names",
                &metadata.interface_names,
            )?;
        }
        if is_reflection_class_owner {
            emit_reflection_class_array_property_by_name(
                ctx,
                class_name,
                "__interfaces",
                &metadata.interface_names,
            )?;
            emit_reflection_owner_string_array_property_by_name(
                ctx,
                class_name,
                "__trait_names",
                &metadata.trait_names,
            )?;
            emit_reflection_class_array_property_by_name(
                ctx,
                class_name,
                "__traits",
                &metadata.trait_names,
            )?;
            emit_reflection_string_assoc_property_by_name(
                ctx,
                class_name,
                "__trait_aliases",
                &metadata.trait_aliases,
            )?;
            emit_reflection_owner_string_array_property_by_name(
                ctx,
                class_name,
                "__parent_names",
                &metadata.parent_names,
            )?;
            emit_reflection_owner_string_array_property_by_name(
                ctx,
                class_name,
                "__method_names",
                &metadata.method_names,
            )?;
            emit_reflection_owner_string_array_property_by_name(
                ctx,
                class_name,
                "__property_names",
                &metadata.property_names,
            )?;
            emit_reflection_owner_string_array_property_by_name(
                ctx,
                class_name,
                "__constant_names",
                &metadata.constant_names,
            )?;
            emit_reflection_constant_array_property_by_name(
                ctx,
                class_name,
                "__constants",
                &metadata.constant_members,
            )?;
            emit_reflection_default_property_array_property_by_name(
                ctx,
                class_name,
                "__default_properties",
                &metadata.default_property_members,
            )?;
            emit_reflection_static_property_array_property_by_name(
                ctx,
                class_name,
                "__static_properties",
                &metadata.static_property_members,
            )?;
            emit_reflection_member_array_property_by_name(
                ctx,
                class_name,
                "__reflection_constants",
                "ReflectionClassConstant",
                &metadata.constant_reflection_members,
            )?;
            emit_reflection_member_array_property_by_name(
                ctx,
                class_name,
                "__methods",
                "ReflectionMethod",
                &metadata.method_members,
            )?;
            emit_reflection_constructor_property(
                ctx,
                class_name,
                metadata.constructor_member.as_ref(),
            )?;
            emit_reflection_parent_class_property(
                ctx,
                class_name,
                metadata.parent_class_name.as_deref(),
            )?;
            if let Some(extension_name) = reflection_extension_name_for_class(reflected_name) {
                emit_reflection_owner_string_property_by_name(
                    ctx,
                    class_name,
                    "__extension_name",
                    extension_name,
                )?;
                abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
                emit_reflection_extension_factory(ctx, extension_name)?;
                emit_reflection_owner_mixed_property_from_result(ctx, class_name, "__extension")?;
            }
            emit_reflection_member_array_property_by_name(
                ctx,
                class_name,
                "__properties",
                "ReflectionProperty",
                &metadata.property_members,
            )?;
        } else if class_name == "ReflectionFunction" {
            let (_, short_name) = reflection_name_parts(reflected_name);
            emit_reflection_owner_string_property_by_name(ctx, class_name, "__short_name", short_name)?;
            if let Some(extension_name) = reflection_extension_name_for_function(reflected_name) {
                emit_reflection_owner_string_property_by_name(
                    ctx,
                    class_name,
                    "__extension_name",
                    extension_name,
                )?;
                abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
                emit_reflection_extension_factory(ctx, extension_name)?;
                emit_reflection_owner_mixed_property_from_result(ctx, class_name, "__extension")?;
            }
        } else if class_name == "ReflectionMethod" {
            if let Some(extension_name) = metadata
                .parent_class_name
                .as_deref()
                .and_then(reflection_extension_name_for_class)
            {
                emit_reflection_owner_string_property_by_name(
                    ctx,
                    class_name,
                    "__extension_name",
                    extension_name,
                )?;
                abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
                emit_reflection_extension_factory(ctx, extension_name)?;
                emit_reflection_owner_mixed_property_from_result(ctx, class_name, "__extension")?;
            }
        }
        if class_name == "ReflectionEnum" {
            if let Some(extension_name) = reflection_extension_name_for_class(reflected_name) {
                emit_reflection_owner_string_property_by_name(
                    ctx,
                    class_name,
                    "__extension_name",
                    extension_name,
                )?;
                abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
                emit_reflection_extension_factory(ctx, extension_name)?;
                emit_reflection_owner_mixed_property_from_result(ctx, class_name, "__extension")?;
            }
            let case_names = metadata
                .enum_case_members
                .iter()
                .map(|member| member.name.clone())
                .collect::<Vec<_>>();
            let case_class = if metadata.type_metadata.is_some() {
                "ReflectionEnumBackedCase"
            } else {
                "ReflectionEnumUnitCase"
            };
            emit_reflection_owner_string_array_property_by_name(
                ctx,
                class_name,
                "__case_names",
                &case_names,
            )?;
            emit_reflection_member_array_property_by_name(
                ctx,
                "ReflectionEnum",
                "__cases",
                case_class,
                &metadata.enum_case_members,
            )?;
            emit_reflection_owner_bool_property(
                ctx,
                class_name,
                "__is_backed",
                metadata.type_metadata.is_some(),
            )?;
            emit_reflection_owner_type_property_by_name(
                ctx,
                class_name,
                "__backing_type",
                metadata.type_metadata.as_ref(),
            )?;
        }
        if class_name == "ReflectionFunction" {
            emit_reflection_function_name_parts(ctx, reflected_name)?;
        }
        if class_name == "ReflectionMethod" {
            emit_reflection_method_name_parts(ctx, reflected_name)?;
        }
    }
    if class_name != "ReflectionExtension" {
        emit_reflection_attrs_property(ctx, class_name, &metadata.attr_names, &metadata.attr_args)?;
    }
    if is_reflection_class_owner || class_name == "ReflectionEnum" {
        emit_reflection_owner_bool_property(ctx, class_name, "__is_final", metadata.is_final)?;
        emit_reflection_owner_bool_property(ctx, class_name, "__is_abstract", metadata.is_abstract)?;
        emit_reflection_owner_bool_property(ctx, class_name, "__is_interface", metadata.is_interface)?;
        emit_reflection_owner_bool_property(ctx, class_name, "__is_trait", metadata.is_trait)?;
        emit_reflection_owner_bool_property(ctx, class_name, "__is_enum", metadata.is_enum)?;
        emit_reflection_owner_bool_property(ctx, class_name, "__is_readonly", metadata.is_readonly)?;
        emit_reflection_owner_bool_property(ctx, class_name, "__is_anonymous", metadata.is_anonymous)?;
        emit_reflection_owner_bool_property(ctx, class_name, "__is_instantiable", metadata.is_instantiable)?;
        emit_reflection_owner_bool_property(ctx, class_name, "__is_cloneable", metadata.is_cloneable)?;
        emit_reflection_owner_bool_property(ctx, class_name, "__is_iterable", metadata.is_iterable)?;
        let is_internal = metadata
            .reflected_name
            .as_deref()
            .is_some_and(reflection_class_like_is_internal);
        emit_reflection_owner_bool_property(ctx, class_name, "__is_internal", is_internal)?;
        emit_reflection_owner_bool_property(
            ctx,
            class_name,
            "__is_user_defined",
            metadata.reflected_name.is_some() && !is_internal,
        )?;
        emit_reflection_owner_int_property(ctx, class_name, "__modifiers", metadata.modifiers)?;
    }
    if matches!(
        class_name,
        "ReflectionMethod"
            | "ReflectionProperty"
            | "ReflectionClassConstant"
            | "ReflectionEnumUnitCase"
            | "ReflectionEnumBackedCase"
    ) {
        emit_reflection_declaring_class_property(
            ctx,
            class_name,
            metadata.parent_class_name.as_deref(),
        )?;
        if matches!(class_name, "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase") {
            emit_reflection_enum_property(ctx, class_name, metadata.parent_class_name.as_deref())?;
        }
    }
    if matches!(class_name, "ReflectionFunction" | "ReflectionMethod") {
        let is_internal = reflection_function_or_method_is_internal(class_name, &metadata);
        emit_reflection_owner_bool_property(ctx, class_name, "__is_internal", is_internal)?;
        emit_reflection_owner_bool_property(
            ctx,
            class_name,
            "__is_user_defined",
            metadata.reflected_name.is_some() && !is_internal,
        )?;
        emit_reflection_parameter_array_property_by_name(
            ctx,
            class_name,
            "__parameters",
            &metadata.parameter_members,
        )?;
        emit_reflection_owner_int_property(
            ctx,
            class_name,
            "__required_parameter_count",
            metadata.required_parameter_count,
        )?;
        emit_reflection_owner_bool_property(
            ctx,
            class_name,
            "__has_return_type",
            metadata.type_metadata.is_some(),
        )?;
        emit_reflection_owner_type_property(ctx, class_name, metadata.type_metadata.as_ref())?;
        emit_reflection_owner_bool_property(
            ctx,
            class_name,
            "__is_deprecated",
            metadata.is_deprecated,
        )?;
        emit_reflection_owner_bool_property(
            ctx,
            class_name,
            "__is_generator",
            metadata.is_generator,
        )?;
    }
    if matches!(
        class_name,
        "ReflectionClassConstant" | "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase"
    ) {
        if let Some(value) = &metadata.constant_value {
            abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
            emit_reflection_constant_value_as_mixed(ctx, value);
            emit_reflection_owner_mixed_property_from_result(ctx, class_name, "__value")?;
        }
    }
    if class_name == "ReflectionClassConstant" {
        emit_reflection_owner_bool_property(
            ctx,
            class_name,
            "__has_type",
            metadata.type_metadata.is_some(),
        )?;
        emit_reflection_owner_type_property(ctx, class_name, metadata.type_metadata.as_ref())?;
    }
    if class_name == "ReflectionEnumBackedCase" {
        if let Some(value) = &metadata.backing_value {
            abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
            emit_reflection_constant_value_as_mixed(ctx, value);
            emit_reflection_owner_mixed_property_from_result(ctx, class_name, "__backing_value")?;
        }
    }
    if matches!(
        class_name,
        "ReflectionClassConstant" | "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase"
    ) {
        emit_reflection_owner_bool_property(
            ctx,
            class_name,
            "__is_enum_case",
            metadata.is_enum_case,
        )?;
        emit_reflection_owner_int_property(ctx, class_name, "__modifiers", metadata.modifiers)?;
    }
    if class_name == "ReflectionMethod" {
        emit_reflection_owner_int_property(ctx, class_name, "__modifiers", metadata.modifiers)?;
        emit_reflection_owner_bool_property(
            ctx,
            class_name,
            "__has_prototype",
            metadata.prototype_member.is_some(),
        )?;
        emit_reflection_method_prototype_property(ctx, metadata.prototype_member.as_deref())?;
    }
    if class_name == "ReflectionProperty" {
        emit_reflection_owner_int_property(ctx, class_name, "__modifiers", metadata.modifiers)?;
        emit_reflection_owner_type_property(ctx, class_name, metadata.type_metadata.as_ref())?;
        emit_reflection_owner_type_property_by_name(
            ctx,
            class_name,
            "__settable_type",
            metadata.type_metadata.as_ref(),
        )?;
        emit_reflection_owner_bool_property(
            ctx,
            class_name,
            "__has_default_value",
            metadata.property_default_value.is_some(),
        )?;
        emit_reflection_owner_default_value_property(
            ctx,
            class_name,
            metadata.property_default_value.as_ref(),
        )?;
        emit_reflection_owner_bool_property(
            ctx,
            class_name,
            "__has_hooks",
            !metadata.property_hook_members.is_empty(),
        )?;
        emit_reflection_property_hook_array_property_by_name(
            ctx,
            class_name,
            "__hooks",
            &metadata.property_hook_members,
        )?;
        let property_string = reflection_property_to_string(
            metadata.reflected_name.as_deref().unwrap_or(""),
            metadata.member_flags,
            metadata.type_metadata.as_ref(),
            metadata.property_default_value.as_ref(),
        );
        emit_reflection_owner_string_property_by_name(
            ctx,
            class_name,
            "__string",
            &property_string,
        )?;
    }
    if class_name == "ReflectionParameter" {
        if let Some(parameter) = metadata.parameter_members.first() {
            emit_reflection_parameter_properties(ctx, parameter)?;
        }
    }
    emit_reflection_member_flag_properties(ctx, class_name, metadata.member_flags)?;
    Ok(())
}

/// Stores an integer immediate into a Reflection object's property slot.
pub(super) fn emit_reflection_int_property(
    ctx: &mut FunctionContext<'_>,
    value: i64,
    low_offset: usize,
    high_offset: usize,
) {
    let object_reg = abi::int_result_reg(ctx.emitter);
    let scratch = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, scratch, value);
    abi::emit_store_to_address(ctx.emitter, scratch, object_reg, low_offset);
    abi::emit_load_int_immediate(ctx.emitter, scratch, 0);
    abi::emit_store_to_address(ctx.emitter, scratch, object_reg, high_offset);
}

/// Stores namespace-aware name parts for a statically materialized class-like reflector.
pub(super) fn emit_reflection_class_name_parts(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    reflected_name: &str,
) -> Result<()> {
    let (namespace_name, short_name) = reflection_name_parts(reflected_name);
    emit_reflection_owner_string_property_by_name(ctx, class_name, "__short_name", short_name)?;
    emit_reflection_owner_string_property_by_name(
        ctx,
        class_name,
        "__namespace_name",
        namespace_name,
    )?;
    emit_reflection_owner_bool_property(
        ctx,
        class_name,
        "__in_namespace",
        !namespace_name.is_empty(),
    )?;
    Ok(())
}

/// Stores namespace-aware name parts for a statically materialized ReflectionFunction.
pub(super) fn emit_reflection_function_name_parts(
    ctx: &mut FunctionContext<'_>,
    reflected_name: &str,
) -> Result<()> {
    let (namespace_name, short_name) = reflection_name_parts(reflected_name);
    emit_reflection_owner_string_property_by_name(
        ctx,
        "ReflectionFunction",
        "__short_name",
        short_name,
    )?;
    emit_reflection_owner_string_property_by_name(
        ctx,
        "ReflectionFunction",
        "__namespace_name",
        namespace_name,
    )?;
    emit_reflection_owner_bool_property(
        ctx,
        "ReflectionFunction",
        "__in_namespace",
        !namespace_name.is_empty(),
    )?;
    Ok(())
}

/// Stores PHP's method reflection name parts for a statically materialized method.
pub(super) fn emit_reflection_method_name_parts(
    ctx: &mut FunctionContext<'_>,
    reflected_name: &str,
) -> Result<()> {
    emit_reflection_owner_string_property_by_name(
        ctx,
        "ReflectionMethod",
        "__short_name",
        reflected_name,
    )?;
    emit_reflection_owner_string_property_by_name(
        ctx,
        "ReflectionMethod",
        "__namespace_name",
        "",
    )?;
    emit_reflection_owner_bool_property(ctx, "ReflectionMethod", "__in_namespace", false)?;
    Ok(())
}

/// Splits a canonical PHP class-like name into namespace and short-name parts.
pub(super) fn reflection_name_parts(reflected_name: &str) -> (&str, &str) {
    match reflected_name.rfind('\\') {
        Some(separator) => (
            &reflected_name[..separator],
            &reflected_name[separator + 1..],
        ),
        None => ("", reflected_name),
    }
}
