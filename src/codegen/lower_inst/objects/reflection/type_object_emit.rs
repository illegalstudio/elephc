//! Purpose:
//! Reflection named, union, and intersection type object emission.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;

/// Allocates and populates one `ReflectionUnionType` object.
pub(super) fn emit_reflection_union_type_object(
    ctx: &mut FunctionContext<'_>,
    type_metadata: &ReflectionUnionTypeMetadata,
) -> Result<()> {
    let (class_id, property_count, uninitialized_marker_offsets) = {
        let class_info = ctx
            .module
            .class_infos
            .get("ReflectionUnionType")
            .ok_or_else(|| CodegenIrError::unsupported("unknown class ReflectionUnionType"))?;
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
    emit_reflection_union_type_types_property(ctx, &type_metadata.types)?;
    emit_reflection_owner_bool_property(
        ctx,
        "ReflectionUnionType",
        "__allows_null",
        type_metadata.allows_null,
    )?;
    emit_reflection_owner_bool_property(
        ctx,
        "ReflectionUnionType",
        "__is_builtin",
        type_metadata.types.iter().all(|member| member.is_builtin),
    )?;
    Ok(())
}

/// Writes the `ReflectionUnionType::__types` array of `ReflectionNamedType` objects.
pub(super) fn emit_reflection_union_type_types_property(
    ctx: &mut FunctionContext<'_>,
    types: &[ReflectionNamedTypeMetadata],
) -> Result<()> {
    let types_offset = {
        let class_info = ctx
            .module
            .class_infos
            .get("ReflectionUnionType")
            .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
        reflection_property_offset(class_info, "__types")?
    };
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    emit_reflection_named_type_array(ctx, types)?;
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, types_offset);
    abi::emit_load_int_immediate(ctx.emitter, abi::secondary_scratch_reg(ctx.emitter), 4);
    abi::emit_store_to_address(
        ctx.emitter,
        abi::secondary_scratch_reg(ctx.emitter),
        object_reg,
        types_offset + 8,
    );
    abi::emit_reg_move(ctx.emitter, result_reg, object_reg);
    Ok(())
}

/// Allocates and populates one `ReflectionIntersectionType` object.
pub(super) fn emit_reflection_intersection_type_object(
    ctx: &mut FunctionContext<'_>,
    type_metadata: &ReflectionIntersectionTypeMetadata,
) -> Result<()> {
    let (class_id, property_count, uninitialized_marker_offsets) = {
        let class_info = ctx
            .module
            .class_infos
            .get("ReflectionIntersectionType")
            .ok_or_else(|| {
                CodegenIrError::unsupported("unknown class ReflectionIntersectionType")
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
    emit_reflection_intersection_type_types_property(ctx, &type_metadata.types)?;
    emit_reflection_owner_bool_property(ctx, "ReflectionIntersectionType", "__allows_null", false)?;
    emit_reflection_owner_bool_property(
        ctx,
        "ReflectionIntersectionType",
        "__is_builtin",
        type_metadata.types.iter().all(|member| member.is_builtin),
    )?;
    Ok(())
}

/// Writes the `ReflectionIntersectionType::__types` array of `ReflectionNamedType` objects.
pub(super) fn emit_reflection_intersection_type_types_property(
    ctx: &mut FunctionContext<'_>,
    types: &[ReflectionNamedTypeMetadata],
) -> Result<()> {
    let types_offset = {
        let class_info = ctx
            .module
            .class_infos
            .get("ReflectionIntersectionType")
            .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
        reflection_property_offset(class_info, "__types")?
    };
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    emit_reflection_named_type_array(ctx, types)?;
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, object_reg, types_offset);
    abi::emit_load_int_immediate(ctx.emitter, abi::secondary_scratch_reg(ctx.emitter), 4);
    abi::emit_store_to_address(
        ctx.emitter,
        abi::secondary_scratch_reg(ctx.emitter),
        object_reg,
        types_offset + 8,
    );
    abi::emit_reg_move(ctx.emitter, result_reg, object_reg);
    Ok(())
}

/// Allocates an indexed array of populated `ReflectionNamedType` objects.
pub(super) fn emit_reflection_named_type_array(
    ctx: &mut FunctionContext<'_>,
    types: &[ReflectionNamedTypeMetadata],
) -> Result<()> {
    emit_reflection_indexed_array(ctx, types.len().max(1), 8);
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &PhpType::Object("ReflectionNamedType".to_string()),
    );
    for type_metadata in types {
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        emit_reflection_named_type_object(ctx, type_metadata)?;
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        emit_append_reflection_member_object(ctx, "ReflectionNamedType");
    }
    Ok(())
}

/// Allocates and populates one `ReflectionNamedType` object.
pub(super) fn emit_reflection_named_type_object(
    ctx: &mut FunctionContext<'_>,
    type_metadata: &ReflectionNamedTypeMetadata,
) -> Result<()> {
    let (class_id, property_count, uninitialized_marker_offsets, name_offset) = {
        let class_info = ctx
            .module
            .class_infos
            .get("ReflectionNamedType")
            .ok_or_else(|| CodegenIrError::unsupported("unknown class ReflectionNamedType"))?;
        (
            class_info.class_id,
            class_info.properties.len(),
            super::super::uninitialized_property_marker_offsets(class_info),
            reflection_property_offset(class_info, "__name")?,
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
    emit_reflection_string_property(ctx, &type_metadata.name, name_offset, name_offset + 8);
    emit_reflection_owner_bool_property(
        ctx,
        "ReflectionNamedType",
        "__allows_null",
        type_metadata.allows_null,
    )?;
    emit_reflection_owner_bool_property(
        ctx,
        "ReflectionNamedType",
        "__is_builtin",
        type_metadata.is_builtin,
    )?;
    Ok(())
}

/// Allocates an indexed array for static reflection metadata.
pub(super) fn emit_reflection_indexed_array(ctx: &mut FunctionContext<'_>, capacity: usize, stride: i64) {
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

/// Retains and appends the stacked member object, then releases its temporary owner.
pub(super) fn emit_append_reflection_member_object(
    ctx: &mut FunctionContext<'_>,
    member_class_name: &str,
) {
    let member_type = PhpType::Object(member_class_name.to_string());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x1");
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_push_reg(ctx.emitter, "x1");
            abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
            emit_release_pushed_refcounted_temp_after_array_push(ctx.emitter, &member_type);
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "rsi");
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_push_reg(ctx.emitter, "rsi");
            abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
            emit_release_pushed_refcounted_temp_after_array_push(ctx.emitter, &member_type);
        }
    }
}

/// Allocates an indexed string array containing ReflectionClass metadata names.
pub(super) fn emit_reflection_string_array(ctx: &mut FunctionContext<'_>, names: &[String]) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", names.len().max(1) as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", 16);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", names.len().max(1) as i64);
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
pub(super) fn emit_reflection_string_array_fill_aarch64(ctx: &mut FunctionContext<'_>, names: &[String]) {
    ctx.emitter.instruction("str x0, [sp, #-16]!");                             // park the metadata-name array while appending strings
    for name in names {
        let (label, len) = ctx.data.add_string(name.as_bytes());
        ctx.emitter.instruction("ldr x0, [sp]");                                // reload the metadata-name array for this append
        abi::emit_symbol_address(ctx.emitter, "x1", &label);
        abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        ctx.emitter.instruction("str x0, [sp]");                                // preserve the possibly-grown metadata-name array
    }
    ctx.emitter.instruction("ldr x0, [sp], #16");                               // restore the final metadata-name array as the result
}

/// Appends ReflectionClass metadata names to the current x86_64 result array.
pub(super) fn emit_reflection_string_array_fill_x86_64(ctx: &mut FunctionContext<'_>, names: &[String]) {
    ctx.emitter.instruction("push rax");                                        // park the metadata-name array while appending strings
    ctx.emitter.instruction("sub rsp, 8");                                      // keep stack alignment stable across append helper calls
    for name in names {
        let (label, len) = ctx.data.add_string(name.as_bytes());
        ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");                // reload the metadata-name array for this append
        abi::emit_symbol_address(ctx.emitter, "rsi", &label);
        abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
        abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");                // preserve the possibly-grown metadata-name array
    }
    ctx.emitter.instruction("add rsp, 8");                                      // drop the temporary alignment slot
    ctx.emitter.instruction("pop rax");                                         // restore the final metadata-name array as the result
}
