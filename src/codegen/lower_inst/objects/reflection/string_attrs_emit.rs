//! Purpose:
//! Reflection string, attribute, and associative metadata property emission.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;

/// Writes a heap-persisted string into the current Reflection object result slot.
pub(super) fn emit_reflection_string_property(
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

/// Writes a heap-persisted string into a named Reflection owner property slot.
pub(super) fn emit_reflection_owner_string_property_by_name(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    property_name: &str,
    value: &str,
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get(class_name)
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, property_name)?;
    emit_reflection_string_property(ctx, value, low_offset, low_offset + 8);
    Ok(())
}

/// Replaces the Reflection object's default `__attrs` array with populated metadata.
pub(super) fn emit_reflection_attrs_property(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    attr_names: &[String],
    attr_args: &[Option<Vec<AttrArgEntry>>],
) -> Result<()> {
    let (attrs_low_offset, attrs_high_offset) = reflection_attrs_offsets(ctx, class_name)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_reg_move(ctx.emitter, object_reg, result_reg);
    abi::emit_load_from_address(ctx.emitter, result_reg, object_reg, attrs_low_offset);
    abi::emit_call_label(ctx.emitter, "__rt_decref_array");
    super::super::super::builtins::attributes::emit_reflection_attribute_array(
        ctx,
        attr_names,
        attr_args,
        reflection_attribute_target_for_owner(class_name),
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

/// Returns PHP's `Attribute::TARGET_*` bitmask for attributes on one Reflection owner type.
pub(super) fn reflection_attribute_target_for_owner(class_name: &str) -> i64 {
    match class_name {
        "ReflectionClass" | "ReflectionObject" => {
            super::super::super::builtins::attributes::REFLECTION_ATTRIBUTE_TARGET_CLASS
        }
        "ReflectionFunction" => {
            super::super::super::builtins::attributes::REFLECTION_ATTRIBUTE_TARGET_FUNCTION
        }
        "ReflectionMethod" => {
            super::super::super::builtins::attributes::REFLECTION_ATTRIBUTE_TARGET_METHOD
        }
        "ReflectionProperty" => {
            super::super::super::builtins::attributes::REFLECTION_ATTRIBUTE_TARGET_PROPERTY
        }
        "ReflectionParameter" => {
            super::super::super::builtins::attributes::REFLECTION_ATTRIBUTE_TARGET_PARAMETER
        }
        "ReflectionClassConstant" | "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase" => {
            super::super::super::builtins::attributes::REFLECTION_ATTRIBUTE_TARGET_CLASS_CONSTANT
        }
        _ => 0,
    }
}

/// Replaces a Reflection owner private array slot with an indexed string array.
pub(super) fn emit_reflection_owner_string_array_property_by_name(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    property_name: &str,
    names: &[String],
) -> Result<()> {
    let class_info = ctx
        .module
        .class_infos
        .get(class_name)
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let low_offset = reflection_property_offset(class_info, property_name)?;
    let high_offset = low_offset + 8;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_reg_move(ctx.emitter, object_reg, result_reg);
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

/// Replaces a ReflectionClass-like private slot with name-keyed ReflectionClass objects.
pub(super) fn emit_reflection_class_array_property_by_name(
    ctx: &mut FunctionContext<'_>,
    owner_class_name: &str,
    property_name: &str,
    names: &[String],
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
    emit_reflection_class_array(ctx, names)?;
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

/// Replaces a ReflectionClass-like private slot with an associative constant-value array.
pub(super) fn emit_reflection_constant_array_property_by_name(
    ctx: &mut FunctionContext<'_>,
    owner_class_name: &str,
    property_name: &str,
    members: &[ReflectionConstantMember],
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
    emit_reflection_constant_array(ctx, members)?;
    let assoc_type = PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    };
    emit_box_current_owned_value_as_mixed(ctx.emitter, &assoc_type);
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_store_zero_to_address(ctx.emitter, object_reg, high_offset);
    abi::emit_push_reg(ctx.emitter, object_reg);
    abi::emit_pop_reg(ctx.emitter, result_reg);
    Ok(())
}

/// Replaces a ReflectionClass-like private slot with an associative default-property array.
pub(super) fn emit_reflection_default_property_array_property_by_name(
    ctx: &mut FunctionContext<'_>,
    owner_class_name: &str,
    property_name: &str,
    members: &[ReflectionDefaultPropertyMember],
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
    emit_reflection_default_property_array(ctx, members);
    let assoc_type = PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    };
    emit_box_current_owned_value_as_mixed(ctx.emitter, &assoc_type);
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_store_zero_to_address(ctx.emitter, object_reg, high_offset);
    abi::emit_push_reg(ctx.emitter, object_reg);
    abi::emit_pop_reg(ctx.emitter, result_reg);
    Ok(())
}

/// Replaces a ReflectionClass-like private slot with current static-property values.
pub(super) fn emit_reflection_static_property_array_property_by_name(
    ctx: &mut FunctionContext<'_>,
    owner_class_name: &str,
    property_name: &str,
    members: &[ReflectionStaticPropertyMember],
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
    emit_reflection_static_property_array(ctx, members);
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, low_offset);
    abi::emit_store_zero_to_address(ctx.emitter, object_reg, high_offset);
    abi::emit_push_reg(ctx.emitter, object_reg);
    abi::emit_pop_reg(ctx.emitter, result_reg);
    Ok(())
}
