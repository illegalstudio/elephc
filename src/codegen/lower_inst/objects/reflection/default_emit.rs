//! Purpose:
//! Reflection default arrays and constant/static value boxing.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;

/// Materializes one Reflection constant value as a boxed Mixed cell.
pub(super) fn emit_reflection_constant_value_as_mixed(
    ctx: &mut FunctionContext<'_>,
    value: &ReflectionConstantValue,
) {
    match value {
        ReflectionConstantValue::Int(value) => emit_boxed_int_literal_to_result(ctx, *value),
        ReflectionConstantValue::Bool(value) => emit_boxed_bool_literal_to_result(ctx, *value),
        ReflectionConstantValue::Float(value) => emit_boxed_float_literal_to_result(ctx, *value),
        ReflectionConstantValue::Str(value) => {
            emit_boxed_string_literal_default_to_result(ctx, value)
        }
        ReflectionConstantValue::Null => emit_boxed_null_literal_to_result(ctx),
        ReflectionConstantValue::EnumCase {
            enum_name,
            case_name,
        } => {
            // Reading a case through Reflection is an access like any other, so it
            // materializes the case if this is its first evaluation.
            crate::codegen::enum_singletons::emit_lazy_case_load(ctx, enum_name, case_name);
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Object(enum_name.clone()));
        }
        ReflectionConstantValue::Array(elements) => {
            emit_reflection_constant_array_to_result(ctx, elements);
            emit_box_current_owned_value_as_mixed(
                ctx.emitter,
                &PhpType::Array(Box::new(PhpType::Mixed)),
            );
        }
        ReflectionConstantValue::AssocArray(entries) => {
            emit_reflection_constant_assoc_array_to_result(ctx, entries);
            emit_box_current_owned_value_as_mixed(
                ctx.emitter,
                &PhpType::AssocArray {
                    key: Box::new(PhpType::Mixed),
                    value: Box::new(PhpType::Mixed),
                },
            );
        }
    }
}

/// Allocates and populates an indexed array whose slots hold boxed constant values.
pub(super) fn emit_reflection_constant_array_to_result(
    ctx: &mut FunctionContext<'_>,
    elements: &[ReflectionConstantValue],
) {
    emit_reflection_mixed_array_allocation(ctx, elements.len());
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    for element in elements {
        emit_reflection_constant_value_as_mixed(ctx, element);
        append_reflection_mixed_array_default_element(ctx);
    }
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
}

/// Allocates and populates an associative array whose values hold boxed constant values.
pub(super) fn emit_reflection_constant_assoc_array_to_result(
    ctx: &mut FunctionContext<'_>,
    entries: &[ReflectionConstantAssocEntry],
) {
    emit_empty_assoc_array_literal_to_result(ctx, &PhpType::Mixed);
    for entry in entries {
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        emit_reflection_constant_value_as_mixed(ctx, &entry.value);
        emit_reflection_assoc_array_default_insert(ctx, &entry.key);
    }
}

/// Materializes one Reflection default-property value as a boxed Mixed cell.
pub(super) fn emit_reflection_default_value_as_mixed(
    ctx: &mut FunctionContext<'_>,
    value: &ReflectionParameterDefaultValue,
) {
    match value {
        ReflectionParameterDefaultValue::Int(value) => {
            emit_boxed_int_literal_to_result(ctx, *value)
        }
        ReflectionParameterDefaultValue::Bool(value) => {
            emit_boxed_bool_literal_to_result(ctx, *value)
        }
        ReflectionParameterDefaultValue::Float(value) => {
            emit_boxed_float_literal_to_result(ctx, *value)
        }
        ReflectionParameterDefaultValue::Str(value) => {
            emit_boxed_string_literal_default_to_result(ctx, value)
        }
        ReflectionParameterDefaultValue::Null => emit_boxed_null_literal_to_result(ctx),
        ReflectionParameterDefaultValue::Object { args, .. } if args.is_empty() => {
            emit_boxed_null_literal_to_result(ctx)
        }
        ReflectionParameterDefaultValue::Object { args, .. } => {
            emit_reflection_indexed_array_default_as_mixed(ctx, args)
        }
        ReflectionParameterDefaultValue::Array(elements) => {
            emit_reflection_indexed_array_default_as_mixed(ctx, elements)
        }
        ReflectionParameterDefaultValue::AssocArray(entries) => {
            emit_reflection_assoc_array_default_as_mixed(ctx, entries)
        }
    }
}

/// Materializes an indexed Reflection default array as a boxed Mixed cell.
pub(super) fn emit_reflection_indexed_array_default_as_mixed(
    ctx: &mut FunctionContext<'_>,
    elements: &[ReflectionParameterDefaultValue],
) {
    emit_reflection_indexed_array_default_to_result(ctx, elements);
    emit_box_current_owned_value_as_mixed(ctx.emitter, &PhpType::Array(Box::new(PhpType::Mixed)));
}

/// Allocates and populates an indexed array whose slots hold boxed Mixed defaults.
pub(super) fn emit_reflection_indexed_array_default_to_result(
    ctx: &mut FunctionContext<'_>,
    elements: &[ReflectionParameterDefaultValue],
) {
    emit_reflection_mixed_array_allocation(ctx, elements.len());
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    for element in elements {
        emit_reflection_default_value_as_mixed(ctx, element);
        append_reflection_mixed_array_default_element(ctx);
    }
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
}

/// Allocates an indexed array stamped for boxed Mixed payload slots.
pub(super) fn emit_reflection_mixed_array_allocation(ctx: &mut FunctionContext<'_>, element_count: usize) {
    let capacity = element_count.max(4);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", 8);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", 8);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    emit_array_value_type_stamp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &PhpType::Mixed,
    );
}

/// Appends the boxed Mixed result value to the indexed array saved on the stack.
#[rustfmt::skip]
pub(super) fn append_reflection_mixed_array_default_element(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x9");
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("mov x1, x0");                              // pass the boxed Reflection default to the array append helper
            ctx.emitter.instruction("mov x0, x9");                              // pass the saved default-array pointer to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
            emit_release_pushed_refcounted_temp_after_array_push(ctx.emitter, &PhpType::Mixed);
            abi::emit_push_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "r11");
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rsi, rax");                            // pass the boxed Reflection default to the array append helper
            ctx.emitter.instruction("mov rdi, r11");                            // pass the saved default-array pointer to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
            emit_release_pushed_refcounted_temp_after_array_push(ctx.emitter, &PhpType::Mixed);
            abi::emit_push_reg(ctx.emitter, "rax");
        }
    }
}

/// Materializes an associative Reflection default array as a boxed Mixed cell.
pub(super) fn emit_reflection_assoc_array_default_as_mixed(
    ctx: &mut FunctionContext<'_>,
    entries: &[ReflectionDefaultAssocEntry],
) {
    emit_reflection_assoc_array_default_to_result(ctx, entries);
    emit_box_current_owned_value_as_mixed(
        ctx.emitter,
        &PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        },
    );
}

/// Allocates and populates an associative array whose values are boxed Mixed defaults.
pub(super) fn emit_reflection_assoc_array_default_to_result(
    ctx: &mut FunctionContext<'_>,
    entries: &[ReflectionDefaultAssocEntry],
) {
    emit_empty_assoc_array_literal_to_result(ctx, &PhpType::Mixed);
    for entry in entries {
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        emit_reflection_default_value_as_mixed(ctx, &entry.value);
        emit_reflection_assoc_array_default_insert(ctx, &entry.key);
    }
}

/// Inserts the current boxed Mixed default value into the stacked associative default array.
#[rustfmt::skip]
pub(super) fn emit_reflection_assoc_array_default_insert(
    ctx: &mut FunctionContext<'_>,
    key: &ReflectionDefaultArrayKey,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x3, x0");                              // pass the boxed Reflection default as the hash payload
            ctx.emitter.instruction("mov x4, xzr");                             // boxed Mixed hash payloads do not use the high word
            abi::emit_pop_reg(ctx.emitter, "x0");
            emit_reflection_default_array_key_aarch64(ctx, key);
            abi::emit_load_int_immediate(
                ctx.emitter,
                "x5",
                runtime_value_tag(&PhpType::Mixed) as i64,
            );
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rcx, rax");                            // pass the boxed Reflection default as the hash payload
            ctx.emitter.instruction("xor r8, r8");                              // boxed Mixed hash payloads do not use the high word
            abi::emit_pop_reg(ctx.emitter, "rdi");
            emit_reflection_default_array_key_x86_64(ctx, key);
            abi::emit_load_int_immediate(
                ctx.emitter,
                "r9",
                runtime_value_tag(&PhpType::Mixed) as i64,
            );
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
        }
    }
}

/// Materializes an associative default-array key in AArch64 hash-key registers.
pub(super) fn emit_reflection_default_array_key_aarch64(
    ctx: &mut FunctionContext<'_>,
    key: &ReflectionDefaultArrayKey,
) {
    match key {
        ReflectionDefaultArrayKey::Int(value) => {
            abi::emit_load_int_immediate(ctx.emitter, "x1", *value);
            abi::emit_load_int_immediate(ctx.emitter, "x2", -1);
        }
        ReflectionDefaultArrayKey::Str(value) => {
            let (key_label, key_len) = ctx.data.add_string(value.as_bytes());
            abi::emit_symbol_address(ctx.emitter, "x1", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", key_len as i64);
        }
    }
}

/// Materializes an associative default-array key in x86_64 SysV hash-key registers.
pub(super) fn emit_reflection_default_array_key_x86_64(
    ctx: &mut FunctionContext<'_>,
    key: &ReflectionDefaultArrayKey,
) {
    match key {
        ReflectionDefaultArrayKey::Int(value) => {
            abi::emit_load_int_immediate(ctx.emitter, "rsi", *value);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", -1);
        }
        ReflectionDefaultArrayKey::Str(value) => {
            let (key_label, key_len) = ctx.data.add_string(value.as_bytes());
            abi::emit_symbol_address(ctx.emitter, "rsi", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", key_len as i64);
        }
    }
}

/// Inserts the current boxed Mixed constant value into the stacked associative array.
pub(super) fn emit_reflection_constant_hash_insert(ctx: &mut FunctionContext<'_>, key: &str) {
    let (key_label, key_len) = ctx.data.add_string(key.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x3, x0");                              // pass the boxed Reflection constant value as the hash payload
            ctx.emitter.instruction("mov x4, xzr");                             // boxed Mixed hash payloads do not use the high word
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x1", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", key_len as i64);
            abi::emit_load_int_immediate(
                ctx.emitter,
                "x5",
                runtime_value_tag(&PhpType::Mixed) as i64,
            );
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rcx, rax");                            // pass the boxed Reflection constant value as the hash payload
            ctx.emitter.instruction("xor r8, r8");                              // boxed Mixed hash payloads do not use the high word
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_symbol_address(ctx.emitter, "rsi", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", key_len as i64);
            abi::emit_load_int_immediate(
                ctx.emitter,
                "r9",
                runtime_value_tag(&PhpType::Mixed) as i64,
            );
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
        }
    }
}

/// Inserts the current boxed Mixed static-property value into the stacked associative array.
#[rustfmt::skip]
pub(super) fn emit_reflection_static_property_hash_insert(ctx: &mut FunctionContext<'_>, key: &str) {
    let (key_label, key_len) = ctx.data.add_string(key.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x3, x0");                              // pass the boxed Reflection static value as the hash payload
            ctx.emitter.instruction("mov x4, xzr");                             // boxed Mixed hash payloads do not use the high word
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x1", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", key_len as i64);
            abi::emit_load_int_immediate(
                ctx.emitter,
                "x5",
                runtime_value_tag(&PhpType::Mixed) as i64,
            );
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rcx, rax");                            // pass the boxed Reflection static value as the hash payload
            ctx.emitter.instruction("xor r8, r8");                              // boxed Mixed hash payloads do not use the high word
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_symbol_address(ctx.emitter, "rsi", &key_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", key_len as i64);
            abi::emit_load_int_immediate(
                ctx.emitter,
                "r9",
                runtime_value_tag(&PhpType::Mixed) as i64,
            );
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
        }
    }
}

