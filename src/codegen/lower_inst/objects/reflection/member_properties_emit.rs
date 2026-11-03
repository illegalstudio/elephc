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
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, 0);
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
        emit_box_current_value_as_mixed(
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
        emit_box_current_value_as_mixed(
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
        emit_box_current_value_as_mixed(
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

/// Static slot that shares one shallow `ReflectionClass` for `class_name` while a top-level
/// Reflection owner is being materialized.
fn shared_reflection_class_slot(class_name: &str) -> String {
    let key: String = crate::names::php_symbol_key(class_name)
        .bytes()
        .map(|byte| format!("{:02x}", byte))
        .collect();
    format!("_elephc_refl_shared_{}", key)
}

/// Leaves an OWNED shallow `ReflectionClass` for `class_name` in the integer result register.
///
/// Every method, property, and parameter reflector carries a declaring-class (or parameter
/// type-class) `ReflectionClass`. Building a fresh one per member made a single
/// `new ReflectionClass(...)` allocate O(members) copies of the class's default and static
/// property arrays — megabytes for a class like `mysqli`. Inside a top-level owner the first
/// member builds it and parks it in a static slot; the rest retain that same object, so
/// `getDeclaringClass()` of two members of one class returns one shared reflector (php
/// returns equal, separately allocated ones). The slot holds one reference until
/// `release_shared_reflection_classes` drops it, and is checked at runtime because the owner's
/// runtime-class dispatch emits several candidate paths.
pub(super) fn emit_shared_shallow_reflection_class(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
) -> Result<()> {
    let metadata = reflection_shallow_class_metadata_for_name(ctx, class_name)?;
    let Some(cache) = ctx.reflection_class_cache.as_mut() else {
        return emit_reflection_owner_object(ctx, "ReflectionClass", &metadata);
    };
    if !cache.iter().any(|cached| cached == class_name) {
        cache.push(class_name.to_string());
    }
    let slot = shared_reflection_class_slot(class_name);
    ctx.data.add_comm(slot.clone(), 8);
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_ty = PhpType::Object("ReflectionClass".to_string());
    let build = ctx.next_label("refl_shared_class_build");
    let done = ctx.next_label("refl_shared_class_done");
    abi::emit_load_symbol_to_reg(ctx.emitter, result_reg, &slot, 0);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &build);
    abi::emit_incref_if_refcounted(ctx.emitter, &object_ty);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&build);
    emit_reflection_owner_object(ctx, "ReflectionClass", &metadata)?;
    abi::emit_store_reg_to_symbol(ctx.emitter, result_reg, &slot, 0);
    abi::emit_incref_if_refcounted(ctx.emitter, &object_ty);
    ctx.emitter.label(&done);
    Ok(())
}

/// Runs `emit` as one top-level Reflection owner materialization, sharing the shallow
/// `ReflectionClass` reflectors its members need and releasing them afterwards. Nested calls
/// join the enclosing scope.
pub(super) fn with_shared_reflection_classes(
    ctx: &mut FunctionContext<'_>,
    emit: impl FnOnce(&mut FunctionContext<'_>) -> Result<()>,
) -> Result<()> {
    if ctx.reflection_class_cache.is_some() {
        return emit(ctx);
    }
    ctx.reflection_class_cache = Some(Vec::new());
    let outcome = emit(ctx);
    let used = ctx.reflection_class_cache.take().unwrap_or_default();
    outcome?;
    if used.is_empty() {
        return Ok(());
    }
    // The owner object is the result; keep it while the shared slots are released.
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_ty = PhpType::Object("ReflectionClass".to_string());
    abi::emit_push_reg(ctx.emitter, result_reg);
    for class_name in used {
        let slot = shared_reflection_class_slot(&class_name);
        let skip = ctx.next_label("refl_shared_class_unused");
        abi::emit_load_symbol_to_reg(ctx.emitter, result_reg, &slot, 0);
        abi::emit_branch_if_int_result_zero(ctx.emitter, &skip);
        abi::emit_decref_if_refcounted(ctx.emitter, &object_ty);
        abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
        abi::emit_store_reg_to_symbol(ctx.emitter, result_reg, &slot, 0);
        ctx.emitter.label(&skip);
    }
    abi::emit_pop_reg(ctx.emitter, result_reg);
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
    let declaring_class_offset = class_info.property_offsets.get("__declaring_class").copied();
    let public_class_name_offset = class_info.property_offsets.get("class").copied();
    if let (Some(offset), Some(name)) = (public_class_name_offset, declaring_class_name) {
        emit_reflection_literal_string_property(ctx, name, offset, offset + 8);
    }
    let Some(low_offset) = declaring_class_offset else {
        return Ok(());
    };
    let high_offset = low_offset + 8;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    if let Some(declaring_class_name) = declaring_class_name {
        emit_shared_shallow_reflection_class(ctx, declaring_class_name)?;
        emit_box_current_value_as_mixed(
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
        emit_box_current_value_as_mixed(
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
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, 0);
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

