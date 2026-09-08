//! Purpose:
//! ReflectionParameter object properties, defaults, and declaring owners.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;

/// Writes one ReflectionParameter object's private metadata properties.
pub(super) fn emit_reflection_parameter_properties(
    ctx: &mut FunctionContext<'_>,
    parameter: &ReflectionParameterMember,
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get("ReflectionParameter")
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let name_offset = reflection_property_offset(class_info, "__name")?;
    let default_value_constant_name_offset =
        reflection_property_offset(class_info, "__default_value_constant_name")?;
    let default_value_object_class_offset =
        reflection_property_offset(class_info, "__default_value_object_class")?;
    emit_reflection_string_property(ctx, &parameter.name, name_offset, name_offset + 8);
    emit_reflection_attrs_property(
        ctx,
        "ReflectionParameter",
        &parameter.attr_names,
        &parameter.attr_args,
    )?;
    emit_reflection_owner_int_property(
        ctx,
        "ReflectionParameter",
        "__position",
        parameter.position,
    )?;
    emit_reflection_owner_bool_property(
        ctx,
        "ReflectionParameter",
        "__optional",
        parameter.is_optional,
    )?;
    emit_reflection_owner_bool_property(
        ctx,
        "ReflectionParameter",
        "__variadic",
        parameter.is_variadic,
    )?;
    emit_reflection_owner_bool_property(
        ctx,
        "ReflectionParameter",
        "__is_passed_by_reference",
        parameter.is_passed_by_reference,
    )?;
    emit_reflection_owner_bool_property(
        ctx,
        "ReflectionParameter",
        "__is_promoted",
        parameter.is_promoted,
    )?;
    emit_reflection_owner_bool_property(
        ctx,
        "ReflectionParameter",
        "__has_type",
        parameter.has_type,
    )?;
    emit_reflection_owner_bool_property(
        ctx,
        "ReflectionParameter",
        "__allows_null",
        parameter.allows_null,
    )?;
    emit_reflection_owner_bool_property(
        ctx,
        "ReflectionParameter",
        "__is_array_type",
        parameter.is_array_type,
    )?;
    emit_reflection_owner_bool_property(
        ctx,
        "ReflectionParameter",
        "__is_callable_type",
        parameter.is_callable_type,
    )?;
    emit_reflection_parameter_type_property(ctx, parameter)?;
    emit_reflection_parameter_class_property(ctx, parameter)?;
    emit_reflection_owner_bool_property(
        ctx,
        "ReflectionParameter",
        "__has_default_value",
        parameter.default_value.is_some(),
    )?;
    emit_reflection_owner_bool_property(
        ctx,
        "ReflectionParameter",
        "__is_default_value_constant",
        parameter.default_value_constant_name.is_some(),
    )?;
    emit_reflection_string_property(
        ctx,
        parameter
            .default_value_constant_name
            .as_deref()
            .unwrap_or(""),
        default_value_constant_name_offset,
        default_value_constant_name_offset + 8,
    );
    emit_reflection_string_property(
        ctx,
        reflection_parameter_default_object_class(parameter.default_value.as_ref()).unwrap_or(""),
        default_value_object_class_offset,
        default_value_object_class_offset + 8,
    );
    emit_reflection_parameter_default_property(ctx, parameter)?;
    emit_reflection_parameter_declaring_class_property(ctx, parameter)?;
    emit_reflection_parameter_declaring_function_property(ctx, parameter)?;
    Ok(())
}

/// Returns the class name for object parameter defaults that are materialized lazily.
pub(super) fn reflection_parameter_default_object_class(
    default_value: Option<&ReflectionParameterDefaultValue>,
) -> Option<&str> {
    match default_value {
        Some(ReflectionParameterDefaultValue::Object { class_name, .. }) => Some(class_name),
        _ => None,
    }
}

/// Writes one ReflectionParameter object's declaring-function slot.
pub(super) fn emit_reflection_parameter_declaring_function_property(
    ctx: &mut FunctionContext<'_>,
    parameter: &ReflectionParameterMember,
) -> Result<()> {
    let declaring_function_offset = {
        let class_info = ctx
            .module
            .class_infos
            .get("ReflectionParameter")
            .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
        reflection_property_offset(class_info, "__declaring_function")?
    };
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    match parameter.declaring_function.as_ref() {
        Some(ReflectionDeclaringFunctionMember::Function {
            name,
            attr_names,
            attr_args,
            required_parameter_count,
            type_metadata,
            is_deprecated,
            is_generator,
        }) => {
            let mut metadata = empty_reflection_metadata();
            metadata.reflected_name = Some(name.clone());
            metadata.attr_names = attr_names.clone();
            metadata.attr_args = attr_args.clone();
            metadata.required_parameter_count = *required_parameter_count;
            metadata.type_metadata = type_metadata.clone();
            metadata.is_deprecated = *is_deprecated;
            metadata.is_generator = *is_generator;
            emit_reflection_owner_object(ctx, "ReflectionFunction", &metadata)?;
            emit_box_current_owned_value_as_mixed(
                ctx.emitter,
                &PhpType::Object("ReflectionFunction".to_string()),
            );
        }
        Some(ReflectionDeclaringFunctionMember::Method {
            name,
            declaring_class_name,
            attr_names,
            attr_args,
            flags,
            required_parameter_count,
            type_metadata,
            is_deprecated,
            is_generator,
        }) => {
            let mut metadata = empty_reflection_metadata();
            metadata.reflected_name = Some(name.clone());
            metadata.parent_class_name = declaring_class_name.clone();
            metadata.attr_names = attr_names.clone();
            metadata.attr_args = attr_args.clone();
            metadata.member_flags = *flags;
            metadata.required_parameter_count = *required_parameter_count;
            metadata.type_metadata = type_metadata.clone();
            metadata.modifiers = reflection_method_modifiers_from_flags(*flags);
            metadata.is_deprecated = *is_deprecated;
            metadata.is_generator = *is_generator;
            emit_reflection_owner_object(ctx, "ReflectionMethod", &metadata)?;
            emit_box_current_owned_value_as_mixed(
                ctx.emitter,
                &PhpType::Object("ReflectionMethod".to_string()),
            );
        }
        None => emit_boxed_null_literal_to_result(ctx),
    }
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(
        ctx.emitter,
        result_reg,
        object_reg,
        declaring_function_offset,
    );
    abi::emit_store_zero_to_address(ctx.emitter, object_reg, declaring_function_offset + 8);
    abi::emit_reg_move(ctx.emitter, result_reg, object_reg);
    Ok(())
}

/// Writes one ReflectionParameter object's nullable declaring-class slot.
pub(super) fn emit_reflection_parameter_declaring_class_property(
    ctx: &mut FunctionContext<'_>,
    parameter: &ReflectionParameterMember,
) -> Result<()> {
    let declaring_class_offset = {
        let class_info = ctx
            .module
            .class_infos
            .get("ReflectionParameter")
            .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
        reflection_property_offset(class_info, "__declaring_class")?
    };
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    if let Some(declaring_class_name) = parameter.declaring_class_name.as_deref() {
        let declaring_metadata =
            reflection_shallow_class_metadata_for_name(ctx, declaring_class_name)?;
        emit_reflection_owner_object(ctx, "ReflectionClass", &declaring_metadata)?;
        emit_box_current_owned_value_as_mixed(
            ctx.emitter,
            &PhpType::Object("ReflectionClass".to_string()),
        );
    } else {
        emit_boxed_null_literal_to_result(ctx);
    }
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, declaring_class_offset);
    abi::emit_store_zero_to_address(ctx.emitter, object_reg, declaring_class_offset + 8);
    abi::emit_reg_move(ctx.emitter, result_reg, object_reg);
    Ok(())
}

/// Writes one ReflectionParameter object's nullable `ReflectionNamedType` slot.
pub(super) fn emit_reflection_parameter_type_property(
    ctx: &mut FunctionContext<'_>,
    parameter: &ReflectionParameterMember,
) -> Result<()> {
    emit_reflection_owner_type_property(
        ctx,
        "ReflectionParameter",
        parameter.type_metadata.as_ref(),
    )
}

/// Writes one ReflectionParameter object's legacy nullable class-type slot.
pub(super) fn emit_reflection_parameter_class_property(
    ctx: &mut FunctionContext<'_>,
    parameter: &ReflectionParameterMember,
) -> Result<()> {
    let class_offset = {
        let class_info = ctx
            .module
            .class_infos
            .get("ReflectionParameter")
            .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
        reflection_property_offset(class_info, "__class")?
    };
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    if let Some(class_name) = reflection_parameter_class_name(parameter) {
        let class_metadata = reflection_shallow_class_metadata_for_name(ctx, class_name)?;
        emit_reflection_owner_object(ctx, "ReflectionClass", &class_metadata)?;
        emit_box_current_owned_value_as_mixed(
            ctx.emitter,
            &PhpType::Object("ReflectionClass".to_string()),
        );
    } else {
        emit_boxed_null_literal_to_result(ctx);
    }
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, class_offset);
    abi::emit_store_zero_to_address(ctx.emitter, object_reg, class_offset + 8);
    abi::emit_reg_move(ctx.emitter, result_reg, object_reg);
    Ok(())
}

/// Returns the retained object class name for ReflectionParameter::getClass().
pub(super) fn reflection_parameter_class_name(parameter: &ReflectionParameterMember) -> Option<&str> {
    match parameter.type_metadata.as_ref()? {
        ReflectionParameterTypeMetadata::Named(metadata) if !metadata.is_builtin => {
            Some(metadata.name.as_str())
        }
        _ => None,
    }
}

/// Writes one reflection owner's nullable type slot.
pub(super) fn emit_reflection_owner_type_property(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    type_metadata: Option<&ReflectionParameterTypeMetadata>,
) -> Result<()> {
    emit_reflection_owner_type_property_by_name(ctx, class_name, "__type", type_metadata)
}

/// Writes one reflection owner's nullable type-like slot.
pub(super) fn emit_reflection_owner_type_property_by_name(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    property_name: &str,
    type_metadata: Option<&ReflectionParameterTypeMetadata>,
) -> Result<()> {
    let type_offset = {
        let class_info = ctx
            .module
            .class_infos
            .get(class_name)
            .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
        reflection_property_offset(class_info, property_name)?
    };
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    match type_metadata {
        Some(ReflectionParameterTypeMetadata::Named(type_metadata)) => {
            emit_reflection_named_type_object(ctx, type_metadata)?;
            emit_box_current_owned_value_as_mixed(
                ctx.emitter,
                &PhpType::Object("ReflectionNamedType".to_string()),
            );
        }
        Some(ReflectionParameterTypeMetadata::Union(type_metadata)) => {
            emit_reflection_union_type_object(ctx, type_metadata)?;
            emit_box_current_owned_value_as_mixed(
                ctx.emitter,
                &PhpType::Object("ReflectionUnionType".to_string()),
            );
        }
        Some(ReflectionParameterTypeMetadata::Intersection(type_metadata)) => {
            emit_reflection_intersection_type_object(ctx, type_metadata)?;
            emit_box_current_owned_value_as_mixed(
                ctx.emitter,
                &PhpType::Object("ReflectionIntersectionType".to_string()),
            );
        }
        None => emit_boxed_null_literal_to_result(ctx),
    }
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, type_offset);
    abi::emit_store_zero_to_address(ctx.emitter, object_reg, type_offset + 8);
    abi::emit_reg_move(ctx.emitter, result_reg, object_reg);
    Ok(())
}

/// Writes one ReflectionParameter object's boxed default-value slot.
pub(super) fn emit_reflection_parameter_default_property(
    ctx: &mut FunctionContext<'_>,
    parameter: &ReflectionParameterMember,
) -> Result<()> {
    emit_reflection_owner_default_value_property(
        ctx,
        "ReflectionParameter",
        parameter.default_value.as_ref(),
    )
}

/// Writes one reflection owner's boxed default-value slot.
pub(super) fn emit_reflection_owner_default_value_property(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    default_value: Option<&ReflectionParameterDefaultValue>,
) -> Result<()> {
    let default_offset = {
        let class_info = ctx
            .module
            .class_infos
            .get(class_name)
            .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
        reflection_property_offset(class_info, "__default_value")?
    };
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    match default_value {
        Some(value) => emit_reflection_default_value_as_mixed(ctx, value),
        None => emit_boxed_null_literal_to_result(ctx),
    }
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, default_offset);
    abi::emit_store_zero_to_address(ctx.emitter, object_reg, default_offset + 8);
    abi::emit_reg_move(ctx.emitter, result_reg, object_reg);
    Ok(())
}
