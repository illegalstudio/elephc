//! Purpose:
//! Reflection method/property object allocation.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;

/// Allocates and populates one ReflectionMethod/ReflectionProperty object.
pub(super) fn emit_reflection_member_object(
    ctx: &mut FunctionContext<'_>,
    member_class_name: &str,
    member: &ReflectionListedMember,
) -> Result<()> {
    let (class_id, property_count, uninitialized_marker_offsets) = {
        let class_info = ctx
            .module
            .class_infos
            .get(member_class_name)
            .ok_or_else(|| {
                CodegenIrError::unsupported(format!("unknown class {}", member_class_name))
            })?;
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
    emit_reflection_declaring_class_property(
        ctx,
        member_class_name,
        member.declaring_class_name.as_deref(),
    )?;
    if member_class_name == "ReflectionMethod" {
        let has_tentative_return_type = member.type_metadata.is_some()
            && reflection_datetime_method_has_tentative_return_type(
                member.declaring_class_name.as_deref(),
                Some(&member.name),
            );
        emit_reflection_parameter_array_property_by_name(
            ctx,
            member_class_name,
            "__parameters",
            &member.parameters,
        )?;
        emit_reflection_owner_int_property(
            ctx,
            member_class_name,
            "__required_parameter_count",
            member.required_parameter_count,
        )?;
        emit_reflection_owner_bool_property(
            ctx,
            member_class_name,
            "__has_return_type",
            member.type_metadata.is_some() && !has_tentative_return_type,
        )?;
        emit_reflection_owner_type_property(
            ctx,
            member_class_name,
            (!has_tentative_return_type)
                .then_some(member.type_metadata.as_ref())
                .flatten(),
        )?;
        emit_reflection_owner_bool_property(
            ctx,
            member_class_name,
            "__has_tentative_return_type",
            has_tentative_return_type,
        )?;
        emit_reflection_owner_type_property_by_name(
            ctx,
            member_class_name,
            "__tentative_type",
            has_tentative_return_type
                .then_some(member.type_metadata.as_ref())
                .flatten(),
        )?;
        emit_reflection_owner_bool_property(
            ctx,
            member_class_name,
            "__is_deprecated",
            member.is_deprecated,
        )?;
        emit_reflection_owner_bool_property(
            ctx,
            member_class_name,
            "__is_generator",
            member.is_generator,
        )?;
        emit_reflection_owner_int_property(
            ctx,
            member_class_name,
            "__modifiers",
            member.modifiers,
        )?;
        emit_reflection_owner_bool_property(
            ctx,
            member_class_name,
            "__has_prototype",
            member.prototype_member.is_some(),
        )?;
        emit_reflection_method_prototype_property(ctx, member.prototype_member.as_deref())?;
    }
    if member_class_name == "ReflectionProperty" {
        emit_reflection_owner_int_property(
            ctx,
            member_class_name,
            "__modifiers",
            member.modifiers,
        )?;
        emit_reflection_owner_type_property(ctx, member_class_name, member.type_metadata.as_ref())?;
        emit_reflection_owner_type_property_by_name(
            ctx,
            member_class_name,
            "__settable_type",
            member.type_metadata.as_ref(),
        )?;
        emit_reflection_owner_bool_property(
            ctx,
            member_class_name,
            "__has_default_value",
            member.default_value.is_some(),
        )?;
        emit_reflection_owner_default_value_property(
            ctx,
            member_class_name,
            member.default_value.as_ref(),
        )?;
        emit_reflection_owner_bool_property(
            ctx,
            member_class_name,
            "__has_hooks",
            !member.property_hook_members.is_empty(),
        )?;
        if !member.property_hook_members.is_empty() {
            emit_reflection_property_hook_array_property_by_name(
                ctx,
                member_class_name,
                "__hooks",
                &member.property_hook_members,
            )?;
        }
        let property_string = reflection_property_to_string(
            &member.name,
            member.flags,
            member.type_metadata.as_ref(),
            member.default_value.as_ref(),
        );
        emit_reflection_owner_string_property_by_name(
            ctx,
            member_class_name,
            "__string",
            &property_string,
        )?;
    }
    if member_class_name == "ReflectionClassConstant" {
        emit_reflection_owner_bool_property(
            ctx,
            member_class_name,
            "__has_type",
            member.type_metadata.is_some(),
        )?;
        emit_reflection_owner_type_property(ctx, member_class_name, member.type_metadata.as_ref())?;
    }
    if matches!(
        member_class_name,
        "ReflectionClassConstant" | "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase"
    ) {
        if let Some(value) = &member.constant_value {
            abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
            emit_reflection_constant_value_as_mixed(ctx, value);
            emit_reflection_owner_mixed_property_from_result(ctx, member_class_name, "__value")?;
        }
        emit_reflection_owner_bool_property(
            ctx,
            member_class_name,
            "__is_enum_case",
            member.is_enum_case,
        )?;
        emit_reflection_owner_int_property(
            ctx,
            member_class_name,
            "__modifiers",
            member.modifiers,
        )?;
    }
    if member_class_name == "ReflectionEnumBackedCase" {
        if let Some(value) = &member.backing_value {
            abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
            emit_reflection_constant_value_as_mixed(ctx, value);
            emit_reflection_owner_mixed_property_from_result(
                ctx,
                member_class_name,
                "__backing_value",
            )?;
        }
    }
    if matches!(
        member_class_name,
        "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase"
    ) {
        emit_reflection_enum_property(ctx, member_class_name, member.declaring_class_name.as_deref())?;
    }
    emit_reflection_member_flag_properties(ctx, member_class_name, member.flags)?;
    Ok(())
}

/// Allocates and populates one ReflectionParameter object.
pub(super) fn emit_reflection_parameter_object(
    ctx: &mut FunctionContext<'_>,
    parameter: &ReflectionParameterMember,
) -> Result<()> {
    let (class_id, property_count, uninitialized_marker_offsets) = {
        let class_info = ctx
            .module
            .class_infos
            .get("ReflectionParameter")
            .ok_or_else(|| CodegenIrError::unsupported("unknown class ReflectionParameter"))?;
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
    emit_reflection_parameter_properties(ctx, parameter)
}
