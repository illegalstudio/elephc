//! Purpose:
//! Reflection member, constructor, parent, enum, and parameter properties.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;

/// Replaces a reflection-owner private array slot with member reflector objects.
pub(super) fn emit_reflection_member_array_property_by_name(
    ctx: &mut FunctionContext<'_>,
    owner_class_name: &str,
    property_name: &str,
    member_class_name: &str,
    members: &[ReflectionListedMember],
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get(owner_class_name)
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, property_name)?;
    let high_offset = low_offset + 8;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_reg_move(ctx.emitter, object_reg, result_reg);
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

/// Replaces a ReflectionProperty private slot with string-keyed hook ReflectionMethod objects.
pub(super) fn emit_reflection_property_hook_array_property_by_name(
    ctx: &mut FunctionContext<'_>,
    owner_class_name: &str,
    property_name: &str,
    members: &[(String, ReflectionListedMember)],
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get(owner_class_name)
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, property_name)?;
    let high_offset = low_offset + 8;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_reg_move(ctx.emitter, object_reg, result_reg);
    abi::emit_load_from_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_call_label(ctx.emitter, "__rt_decref_array");
    emit_reflection_property_hook_array(ctx, members)?;
    let assoc_type = reflection_property_hook_map_type();
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::secondary_scratch_reg(ctx.emitter),
        runtime_value_tag(&assoc_type) as i64,
    );
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

/// Replaces a ReflectionClass-like private constructor slot with `ReflectionMethod|null`.
pub(super) fn emit_reflection_constructor_property(
    ctx: &mut FunctionContext<'_>,
    owner_class_name: &str,
    member: Option<&ReflectionListedMember>,
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get(owner_class_name)
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, "__constructor")?;
    let high_offset = low_offset + 8;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    if let Some(member) = member {
        emit_reflection_member_object(ctx, "ReflectionMethod", member)?;
        emit_box_current_owned_value_as_mixed(
            ctx.emitter,
            &PhpType::Object("ReflectionMethod".to_string()),
        );
    } else {
        super::super::emit_boxed_null(ctx);
    }
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_store_zero_to_address(ctx.emitter, object_reg, high_offset);
    abi::emit_reg_move(ctx.emitter, result_reg, object_reg);
    Ok(())
}

/// Replaces a ReflectionMethod private prototype slot with `ReflectionMethod|null`.
pub(super) fn emit_reflection_method_prototype_property(
    ctx: &mut FunctionContext<'_>,
    member: Option<&ReflectionListedMember>,
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get("ReflectionMethod")
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, "__prototype")?;
    let high_offset = low_offset + 8;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    if let Some(member) = member {
        emit_reflection_member_object(ctx, "ReflectionMethod", member)?;
        emit_box_current_owned_value_as_mixed(
            ctx.emitter,
            &PhpType::Object("ReflectionMethod".to_string()),
        );
    } else {
        emit_boxed_null_literal_to_result(ctx);
    }
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_store_zero_to_address(ctx.emitter, object_reg, high_offset);
    abi::emit_reg_move(ctx.emitter, result_reg, object_reg);
    Ok(())
}

/// Replaces a ReflectionClass-like private parent slot with `ReflectionClass|false`.
pub(super) fn emit_reflection_parent_class_property(
    ctx: &mut FunctionContext<'_>,
    owner_class_name: &str,
    parent_class_name: Option<&str>,
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get(owner_class_name)
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, "__parent_class")?;
    let high_offset = low_offset + 8;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    if let Some(parent_class_name) = parent_class_name {
        let parent_metadata = reflection_class_metadata_for_name(ctx, parent_class_name)?;
        emit_reflection_owner_object(ctx, "ReflectionClass", &parent_metadata)?;
        emit_box_current_owned_value_as_mixed(
            ctx.emitter,
            &PhpType::Object("ReflectionClass".to_string()),
        );
    } else {
        abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    }
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_store_zero_to_address(ctx.emitter, object_reg, high_offset);
    abi::emit_reg_move(ctx.emitter, result_reg, object_reg);
    Ok(())
}

/// Replaces a member reflector's private declaring-class slot with `ReflectionClass|false`.
pub(super) fn emit_reflection_declaring_class_property(
    ctx: &mut FunctionContext<'_>,
    member_class_name: &str,
    declaring_class_name: Option<&str>,
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get(member_class_name)
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let Some(low_offset) = class_info
        .property_offsets
        .get("__declaring_class")
        .copied()
    else {
        return Ok(());
    };
    let high_offset = low_offset + 8;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    if let Some(declaring_class_name) = declaring_class_name {
        let declaring_metadata =
            reflection_shallow_class_metadata_for_name(ctx, declaring_class_name)?;
        emit_reflection_owner_object(ctx, "ReflectionClass", &declaring_metadata)?;
        emit_box_current_owned_value_as_mixed(
            ctx.emitter,
            &PhpType::Object("ReflectionClass".to_string()),
        );
    } else {
        abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    }
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_store_zero_to_address(ctx.emitter, object_reg, high_offset);
    abi::emit_reg_move(ctx.emitter, result_reg, object_reg);
    Ok(())
}

/// Replaces an enum-case reflector's private enum slot with `ReflectionEnum`.
pub(super) fn emit_reflection_enum_property(
    ctx: &mut FunctionContext<'_>,
    member_class_name: &str,
    enum_name: Option<&str>,
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get(member_class_name)
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let Some(low_offset) = class_info.property_offsets.get("__enum").copied() else {
        return Ok(());
    };
    let high_offset = low_offset + 8;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    if let Some(enum_name) = enum_name {
        let enum_metadata = reflection_enum_metadata_for_name(ctx, enum_name)?;
        emit_reflection_owner_object(ctx, "ReflectionEnum", &enum_metadata)?;
        emit_box_current_owned_value_as_mixed(
            ctx.emitter,
            &PhpType::Object("ReflectionEnum".to_string()),
        );
    } else {
        abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
    }
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_store_zero_to_address(ctx.emitter, object_reg, high_offset);
    abi::emit_reg_move(ctx.emitter, result_reg, object_reg);
    Ok(())
}

/// Replaces a ReflectionMethod private array slot with ReflectionParameter objects.
pub(super) fn emit_reflection_parameter_array_property_by_name(
    ctx: &mut FunctionContext<'_>,
    owner_class_name: &str,
    property_name: &str,
    parameters: &[ReflectionParameterMember],
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get(owner_class_name)
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, property_name)?;
    let high_offset = low_offset + 8;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_reg_move(ctx.emitter, object_reg, result_reg);
    abi::emit_load_from_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_call_label(ctx.emitter, "__rt_decref_array");
    emit_reflection_parameter_array(ctx, parameters)?;
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
