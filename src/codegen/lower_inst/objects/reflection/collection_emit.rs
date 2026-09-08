//! Purpose:
//! Reflection arrays, hashes, constants, and static-property collections.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;

/// Allocates an indexed array of populated ReflectionMethod/ReflectionProperty objects.
pub(super) fn emit_reflection_member_array(
    ctx: &mut FunctionContext<'_>,
    member_class_name: &str,
    members: &[ReflectionListedMember],
) -> Result<()> {
    emit_reflection_indexed_array(ctx, members.len().max(1), 8);
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &PhpType::Object(member_class_name.to_string()),
    );

    for member in members {
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        emit_reflection_member_object(ctx, member_class_name, member)?;
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        emit_append_reflection_member_object(ctx, member_class_name);
    }

    Ok(())
}

/// Allocates a string-keyed hook map with populated ReflectionMethod objects.
pub(super) fn emit_reflection_property_hook_array(
    ctx: &mut FunctionContext<'_>,
    members: &[(String, ReflectionListedMember)],
) -> Result<()> {
    emit_empty_assoc_array_literal_to_result(
        ctx,
        &PhpType::Object("ReflectionMethod".to_string()),
    );
    for (key, member) in members {
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        emit_reflection_member_object(ctx, "ReflectionMethod", member)?;
        emit_reflection_method_hash_insert(ctx, key);
    }
    Ok(())
}

/// Allocates an indexed array of populated ReflectionParameter objects.
pub(super) fn emit_reflection_parameter_array(
    ctx: &mut FunctionContext<'_>,
    parameters: &[ReflectionParameterMember],
) -> Result<()> {
    emit_reflection_indexed_array(ctx, parameters.len().max(1), 8);
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &PhpType::Object("ReflectionParameter".to_string()),
    );

    for parameter in parameters {
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        emit_reflection_parameter_object(ctx, parameter)?;
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        emit_append_reflection_member_object(ctx, "ReflectionParameter");
    }

    Ok(())
}

/// Allocates and populates the associative ReflectionClass constant map.
pub(super) fn emit_reflection_constant_array(
    ctx: &mut FunctionContext<'_>,
    members: &[ReflectionConstantMember],
) -> Result<()> {
    emit_empty_assoc_array_literal_to_result(ctx, &PhpType::Mixed);
    for member in members {
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        emit_reflection_constant_value_as_mixed(ctx, &member.value);
        emit_reflection_constant_hash_insert(ctx, &member.name);
    }
    Ok(())
}

/// Allocates and populates a name-keyed map of full ReflectionClass objects.
pub(super) fn emit_reflection_class_array(ctx: &mut FunctionContext<'_>, names: &[String]) -> Result<()> {
    emit_empty_assoc_array_literal_to_result(ctx, &PhpType::Object("ReflectionClass".to_string()));
    for name in names {
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        let metadata = reflection_class_metadata_for_name(ctx, name)?;
        emit_reflection_owner_object(ctx, "ReflectionClass", &metadata)?;
        emit_reflection_class_hash_insert(ctx, name);
    }
    Ok(())
}

/// Inserts the current ReflectionClass object into the stacked associative array.
pub(super) fn emit_reflection_class_hash_insert(ctx: &mut FunctionContext<'_>, key: &str) {
    let (key_label, key_len) = ctx.data.add_string(key.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x3, x0");                              // pass the ReflectionClass object as the hash payload
            ctx.emitter.instruction("mov x4, xzr");                             // object hash payloads do not use the high word
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x1", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", key_len as i64);
            abi::emit_load_int_immediate(
                ctx.emitter,
                "x5",
                runtime_value_tag(&PhpType::Object("ReflectionClass".to_string())) as i64,
            );
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rcx, rax");                            // pass the ReflectionClass object as the hash payload
            ctx.emitter.instruction("xor r8, r8");                              // object hash payloads do not use the high word
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_symbol_address(ctx.emitter, "rsi", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", key_len as i64);
            abi::emit_load_int_immediate(
                ctx.emitter,
                "r9",
                runtime_value_tag(&PhpType::Object("ReflectionClass".to_string())) as i64,
            );
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
        }
    }
}

/// Inserts the current ReflectionMethod object into the stacked associative array.
pub(super) fn emit_reflection_method_hash_insert(ctx: &mut FunctionContext<'_>, key: &str) {
    let (key_label, key_len) = ctx.data.add_string(key.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x3, x0");                              // pass the ReflectionMethod object as the hook hash payload
            ctx.emitter.instruction("mov x4, xzr");                             // object hook hash payloads do not use the high word
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x1", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", key_len as i64);
            abi::emit_load_int_immediate(
                ctx.emitter,
                "x5",
                runtime_value_tag(&PhpType::Object("ReflectionMethod".to_string())) as i64,
            );
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rcx, rax");                            // pass the ReflectionMethod object as the hook hash payload
            ctx.emitter.instruction("xor r8, r8");                              // object hook hash payloads do not use the high word
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_symbol_address(ctx.emitter, "rsi", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", key_len as i64);
            abi::emit_load_int_immediate(
                ctx.emitter,
                "r9",
                runtime_value_tag(&PhpType::Object("ReflectionMethod".to_string())) as i64,
            );
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
        }
    }
}

/// Returns the associative map type used by `ReflectionProperty::getHooks()`.
pub(super) fn reflection_property_hook_map_type() -> PhpType {
    PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Object("ReflectionMethod".to_string())),
    }
}

/// Replaces a ReflectionClass-like private slot with a string-keyed string-value map.
pub(super) fn emit_reflection_string_assoc_property_by_name(
    ctx: &mut FunctionContext<'_>,
    owner_class_name: &str,
    property_name: &str,
    entries: &[(String, String)],
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
    abi::emit_call_label(ctx.emitter, "__rt_decref_hash");
    emit_reflection_string_assoc_array(ctx, entries);
    let assoc_type = PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Str),
    };
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

/// Allocates and populates a string-keyed associative array of string values.
pub(super) fn emit_reflection_string_assoc_array(
    ctx: &mut FunctionContext<'_>,
    entries: &[(String, String)],
) {
    emit_empty_assoc_array_literal_to_result(ctx, &PhpType::Str);
    for (key, value) in entries {
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        emit_string_literal_default_to_result(ctx, value);
        emit_reflection_string_hash_insert(ctx, key);
    }
}

/// Inserts the current owned string value into the stacked associative array.
#[rustfmt::skip]
pub(super) fn emit_reflection_string_hash_insert(ctx: &mut FunctionContext<'_>, key: &str) {
    let (key_label, key_len) = ctx.data.add_string(key.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            ctx.emitter.instruction("mov x3, x1");                              // pass the persistent Reflection string as the hash payload pointer
            ctx.emitter.instruction("mov x4, x2");                              // pass the Reflection string length as the hash payload high word
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x1", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", key_len as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x5", runtime_value_tag(&PhpType::Str) as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
        }
        Arch::X86_64 => {
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            ctx.emitter.instruction("mov rcx, rax");                            // pass the persistent Reflection string as the hash payload pointer
            ctx.emitter.instruction("mov r8, rdx");                             // pass the Reflection string length as the hash payload high word
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_symbol_address(ctx.emitter, "rsi", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", key_len as i64);
            abi::emit_load_int_immediate(ctx.emitter, "r9", runtime_value_tag(&PhpType::Str) as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
        }
    }
}

/// Allocates and populates the associative ReflectionClass default-property map.
pub(super) fn emit_reflection_default_property_array(
    ctx: &mut FunctionContext<'_>,
    members: &[ReflectionDefaultPropertyMember],
) {
    emit_empty_assoc_array_literal_to_result(ctx, &PhpType::Mixed);
    for member in members {
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        emit_reflection_default_value_as_mixed(ctx, &member.value);
        emit_reflection_constant_hash_insert(ctx, &member.name);
    }
}

/// Allocates and populates current static-property values for ReflectionClass.
pub(super) fn emit_reflection_static_property_array(
    ctx: &mut FunctionContext<'_>,
    members: &[ReflectionStaticPropertyMember],
) {
    emit_empty_assoc_array_literal_to_result(ctx, &PhpType::Mixed);
    for member in members {
        let skip_label = emit_skip_if_static_property_uninitialized(ctx, member);
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        let symbol = static_property_symbol(&member.declaring_class_name, &member.name);
        abi::emit_load_symbol_to_result(ctx.emitter, &symbol, &member.php_type);
        emit_box_current_value_as_mixed(ctx.emitter, &member.php_type.codegen_repr());
        emit_reflection_static_property_hash_insert(ctx, &member.name);
        if let Some(skip_label) = skip_label {
            ctx.emitter.label(&skip_label);
        }
    }
}

/// Emits a branch over uninitialized typed static properties, matching PHP reflection.
#[rustfmt::skip]
pub(super) fn emit_skip_if_static_property_uninitialized(
    ctx: &mut FunctionContext<'_>,
    member: &ReflectionStaticPropertyMember,
) -> Option<String> {
    if !member.is_declared {
        return None;
    }
    let skip_label = ctx.next_label("reflection_static_uninitialized");
    let symbol = static_property_symbol(&member.declaring_class_name, &member.name);
    let marker_reg = abi::secondary_scratch_reg(ctx.emitter);
    let sentinel_reg = abi::tertiary_scratch_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, marker_reg, &symbol, 8);
    abi::emit_load_int_immediate(
        ctx.emitter,
        sentinel_reg,
        UNINITIALIZED_TYPED_PROPERTY_SENTINEL,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(
                &format!("cmp {}, {}", marker_reg, sentinel_reg)
            );                                                                  // compare the static property marker against the uninitialized sentinel
            ctx.emitter.instruction(&format!("b.eq {}", skip_label));           // omit uninitialized typed static properties from the reflection map
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(
                &format!("cmp {}, {}", marker_reg, sentinel_reg)
            );                                                                  // compare the static property marker against the uninitialized sentinel
            ctx.emitter.instruction(&format!("je {}", skip_label));             // omit uninitialized typed static properties from the reflection map
        }
    }
    Some(skip_label)
}
