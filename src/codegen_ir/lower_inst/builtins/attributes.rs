//! Purpose:
//! Lowers class-level PHP attribute metadata builtins for the EIR backend.
//! Materializes attribute name arrays and literal argument hashes from EIR class metadata.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - Class and attribute lookup follows PHP's case-insensitive symbol rules.
//! - Captured literal attribute arguments are boxed as owned Mixed cells in PHP array order.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::{Immediate, Instruction, Module, Op, ValueDef, ValueId};
use crate::names::php_symbol_key;
use crate::types::{AttrArgValue, ClassInfo, PhpType};

use super::super::super::context::FunctionContext;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;
pub(in crate::codegen_ir::lower_inst) const REFLECTION_ATTRIBUTE_TARGET_CLASS: i64 = 1;
pub(in crate::codegen_ir::lower_inst) const REFLECTION_ATTRIBUTE_TARGET_FUNCTION: i64 = 2;
pub(in crate::codegen_ir::lower_inst) const REFLECTION_ATTRIBUTE_TARGET_METHOD: i64 = 4;
pub(in crate::codegen_ir::lower_inst) const REFLECTION_ATTRIBUTE_TARGET_PROPERTY: i64 = 8;
pub(in crate::codegen_ir::lower_inst) const REFLECTION_ATTRIBUTE_TARGET_CLASS_CONSTANT: i64 = 16;
pub(in crate::codegen_ir::lower_inst) const REFLECTION_ATTRIBUTE_TARGET_PARAMETER: i64 = 32;

/// Fixed object slot layout for the synthetic `ReflectionAttribute` class.
struct ReflectionAttributeLayout {
    class_id: u64,
    property_count: usize,
    name_lo: usize,
    name_hi: usize,
    args_lo: usize,
    args_hi: usize,
    factory_lo: usize,
    factory_hi: usize,
    target_lo: usize,
    target_hi: usize,
    repeated_lo: usize,
    repeated_hi: usize,
}

/// Lowers `class_attribute_names(class)` into an indexed string array.
pub(super) fn lower_class_attribute_names(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "class_attribute_names", 1)?;
    let class = super::expect_operand(inst, 0)?;
    let class_name = const_string_operand(ctx, class, "class_attribute_names")?;
    let names = class_info(ctx, &class_name)
        .map(|info| info.attribute_names.clone())
        .unwrap_or_default();

    emit_string_array(ctx, &names)?;
    super::store_if_result(ctx, inst)
}

/// Lowers `class_attribute_args(class, attr)` into a Mixed PHP argument array.
pub(super) fn lower_class_attribute_args(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "class_attribute_args", 2)?;
    let class = super::expect_operand(inst, 0)?;
    let attr = super::expect_operand(inst, 1)?;
    let class_name = const_string_operand(ctx, class, "class_attribute_args")?;
    let attr_name = const_string_operand(ctx, attr, "class_attribute_args")?;
    let attr_args = attribute_args(ctx, &class_name, &attr_name);

    emit_mixed_array(ctx, &attr_args)?;
    super::store_if_result(ctx, inst)
}

/// Lowers `class_get_attributes(class)` into an array of `ReflectionAttribute` objects.
pub(super) fn lower_class_get_attributes(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "class_get_attributes", 1)?;
    let class = super::expect_operand(inst, 0)?;
    let class_name = const_string_operand(ctx, class, "class_get_attributes")?;
    let (attr_names, attr_args) = class_info(ctx, &class_name)
        .map(|info| (info.attribute_names.clone(), info.attribute_args.clone()))
        .unwrap_or_else(|| (Vec::new(), Vec::new()));

    emit_reflection_attribute_array(
        ctx,
        &attr_names,
        &attr_args,
        REFLECTION_ATTRIBUTE_TARGET_CLASS,
    )?;
    super::store_if_result(ctx, inst)
}

/// Returns captured literal args for the first matching class attribute.
fn attribute_args(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    attr_name: &str,
) -> Vec<AttrArgValue> {
    let attr_key = php_symbol_key(attr_name.trim_start_matches('\\'));
    class_info(ctx, class_name)
        .and_then(|info| {
            info.attribute_names
                .iter()
                .enumerate()
                .find_map(|(idx, name)| {
                    let candidate = php_symbol_key(name.trim_start_matches('\\'));
                    (candidate == attr_key).then(|| {
                        info.attribute_args
                            .get(idx)
                            .and_then(Clone::clone)
                            .unwrap_or_default()
                    })
                })
        })
        .unwrap_or_default()
}

/// Looks up class metadata by PHP-style case-insensitive name.
fn class_info<'a>(ctx: &'a FunctionContext<'_>, class_name: &str) -> Option<&'a ClassInfo> {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    ctx.module
        .class_infos
        .iter()
        .find(|(candidate, _)| php_symbol_key(candidate.trim_start_matches('\\')) == class_key)
        .map(|(_, info)| info)
}

/// Allocates and fills an indexed array of populated `ReflectionAttribute` objects.
pub(in crate::codegen_ir::lower_inst) fn emit_reflection_attribute_array(
    ctx: &mut FunctionContext<'_>,
    attr_names: &[String],
    attr_args: &[Option<Vec<AttrArgValue>>],
    target: i64,
) -> Result<()> {
    let layout = reflection_attribute_layout(ctx)?;
    allocate_indexed_array(ctx, attr_names.len().max(1), 8);
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &PhpType::Object("ReflectionAttribute".to_string()),
    );

    for (idx, attr_name) in attr_names.iter().enumerate() {
        let attr_arg_list = attr_args
            .get(idx)
            .and_then(|args| args.as_deref())
            .unwrap_or(&[]);
        let factory_id = {
            let function_attrs = function_attribute_sources(ctx.module);
            crate::codegen::reflection::attribute_factory_id_with_extra(
                &ctx.module.class_infos,
                &function_attrs,
                attr_name,
                attr_arg_list,
            )
        };

        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        emit_reflection_attribute_object(ctx, &layout);
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        emit_set_name_property(ctx, attr_name, &layout);
        emit_set_args_property(ctx, attr_arg_list, &layout)?;
        emit_set_factory_property(ctx, factory_id, &layout);
        emit_set_target_property(ctx, target, &layout);
        emit_set_repeated_property(
            ctx,
            reflection_attribute_name_is_repeated(attr_names, attr_name),
            &layout,
        );
        emit_append_reflection_attribute_object(ctx);
    }

    Ok(())
}

/// Returns reflection-visible top-level function attribute metadata sources.
fn function_attribute_sources(
    module: &Module,
) -> Vec<crate::codegen::reflection::AttributeMetadataSource<'_>> {
    module
        .functions
        .iter()
        .filter(|function| !function.attribute_names.is_empty())
        .map(|function| {
            (
                function.attribute_names.as_slice(),
                function.attribute_args.as_slice(),
            )
        })
        .collect()
}

/// Returns the synthetic `ReflectionAttribute` class layout from EIR metadata.
fn reflection_attribute_layout(ctx: &FunctionContext<'_>) -> Result<ReflectionAttributeLayout> {
    let info = ctx
        .module
        .class_infos
        .get("ReflectionAttribute")
        .ok_or_else(|| CodegenIrError::missing_entry("class", 0))?;
    let name_lo = reflection_property_offset(info, "__name")?;
    let args_lo = reflection_property_offset(info, "__args")?;
    let factory_lo = reflection_property_offset(info, "__factory")?;
    let target_lo = reflection_property_offset(info, "__target")?;
    let repeated_lo = reflection_property_offset(info, "__is_repeated")?;
    Ok(ReflectionAttributeLayout {
        class_id: info.class_id,
        property_count: info.properties.len(),
        name_lo,
        name_hi: name_lo + 8,
        args_lo,
        args_hi: args_lo + 8,
        factory_lo,
        factory_hi: factory_lo + 8,
        target_lo,
        target_hi: target_lo + 8,
        repeated_lo,
        repeated_hi: repeated_lo + 8,
    })
}

/// Returns one declared property offset from the synthetic reflection class layout.
fn reflection_property_offset(info: &ClassInfo, property: &str) -> Result<usize> {
    info.property_offsets.get(property).copied().ok_or_else(|| {
        CodegenIrError::invalid_module(format!(
            "ReflectionAttribute missing property offset for ${}",
            property
        ))
    })
}

/// Allocates a zero-initialized `ReflectionAttribute` object payload.
fn emit_reflection_attribute_object(
    ctx: &mut FunctionContext<'_>,
    layout: &ReflectionAttributeLayout,
) {
    let payload_size = 8 + layout.property_count * 16;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("mov x0, #{}", payload_size));            // request ReflectionAttribute object payload storage
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("mov x9, #4");                              // heap kind 4 marks ReflectionAttribute as an object
            ctx.emitter.instruction("str x9, [x0, #-8]");                       // stamp the object heap header before the payload
            ctx.emitter
                .instruction(&format!("mov x10, #{}", layout.class_id));        // materialize the ReflectionAttribute class id
            ctx.emitter.instruction("str x10, [x0]");                           // store the class id at object payload offset zero
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov rax, {}", payload_size));            // request ReflectionAttribute object payload storage
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction(&format!(
                "mov r10, 0x{:x}",
                (X86_64_HEAP_MAGIC_HI32 << 32) | 4
            ));                                                                 // materialize the x86_64 object heap kind word
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp the object heap header before the payload
            ctx.emitter
                .instruction(&format!("mov r10, {}", layout.class_id));         // materialize the ReflectionAttribute class id
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");                // store the class id at object payload offset zero
        }
    }
    let object_reg = abi::int_result_reg(ctx.emitter);
    for index in 0..layout.property_count {
        let offset = 8 + index * 16;
        abi::emit_store_zero_to_address(ctx.emitter, object_reg, offset);
        abi::emit_store_zero_to_address(ctx.emitter, object_reg, offset + 8);
    }
}

/// Stores the reflected attribute name into the object currently parked on the stack.
fn emit_set_name_property(
    ctx: &mut FunctionContext<'_>,
    attr_name: &str,
    layout: &ReflectionAttributeLayout,
) {
    let (label, len) = ctx.data.add_string(attr_name.as_bytes());
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, 0);
            abi::emit_store_to_address(ctx.emitter, "x1", object_reg, layout.name_lo);
            abi::emit_store_to_address(ctx.emitter, "x2", object_reg, layout.name_hi);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rax", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, 0);
            abi::emit_store_to_address(ctx.emitter, "rax", object_reg, layout.name_lo);
            abi::emit_store_to_address(ctx.emitter, "rdx", object_reg, layout.name_hi);
        }
    }
}

/// Stores a freshly materialized mixed argument array on the stacked object.
fn emit_set_args_property(
    ctx: &mut FunctionContext<'_>,
    attr_args: &[AttrArgValue],
    layout: &ReflectionAttributeLayout,
) -> Result<()> {
    emit_mixed_array(ctx, attr_args)?;
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    let tag_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, 0);
    abi::emit_store_to_address(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        object_reg,
        layout.args_lo,
    );
    abi::emit_load_int_immediate(ctx.emitter, tag_reg, 5);
    abi::emit_store_to_address(ctx.emitter, tag_reg, object_reg, layout.args_hi);
    Ok(())
}

/// Stores the `newInstance()` factory id on the stacked reflection object.
fn emit_set_factory_property(
    ctx: &mut FunctionContext<'_>,
    factory_id: i64,
    layout: &ReflectionAttributeLayout,
) {
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    let factory_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, 0);
    abi::emit_load_int_immediate(ctx.emitter, factory_reg, factory_id);
    abi::emit_store_to_address(ctx.emitter, factory_reg, object_reg, layout.factory_lo);
    abi::emit_store_zero_to_address(ctx.emitter, object_reg, layout.factory_hi);
}

/// Stores the PHP `Attribute::TARGET_*` bitmask on the stacked reflection object.
fn emit_set_target_property(
    ctx: &mut FunctionContext<'_>,
    target: i64,
    layout: &ReflectionAttributeLayout,
) {
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    let target_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, 0);
    abi::emit_load_int_immediate(ctx.emitter, target_reg, target);
    abi::emit_store_to_address(ctx.emitter, target_reg, object_reg, layout.target_lo);
    abi::emit_store_zero_to_address(ctx.emitter, object_reg, layout.target_hi);
}

/// Stores whether this attribute name is repeated on the same owner.
fn emit_set_repeated_property(
    ctx: &mut FunctionContext<'_>,
    repeated: bool,
    layout: &ReflectionAttributeLayout,
) {
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    let repeated_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, 0);
    abi::emit_load_int_immediate(ctx.emitter, repeated_reg, if repeated { 1 } else { 0 });
    abi::emit_store_to_address(ctx.emitter, repeated_reg, object_reg, layout.repeated_lo);
    abi::emit_store_zero_to_address(ctx.emitter, object_reg, layout.repeated_hi);
}

/// Returns true when an attribute name appears multiple times on one reflected owner.
fn reflection_attribute_name_is_repeated(attr_names: &[String], attr_name: &str) -> bool {
    let needle = php_symbol_key(attr_name.trim_start_matches('\\'));
    attr_names
        .iter()
        .filter(|candidate| php_symbol_key(candidate.trim_start_matches('\\')) == needle)
        .nth(1)
        .is_some()
}

/// Appends the stacked object to the stacked result array and leaves the array in result.
fn emit_append_reflection_attribute_object(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x1");
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "rsi");
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
    }
}

/// Allocates and fills an indexed array of attribute-name strings.
fn emit_string_array(ctx: &mut FunctionContext<'_>, names: &[String]) -> Result<()> {
    allocate_indexed_array(ctx, names.len().max(1), 16);
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_string_array_fill_aarch64(ctx, names),
        Arch::X86_64 => emit_string_array_fill_x86_64(ctx, names),
    }
    Ok(())
}

/// Appends attribute-name strings to the current result array on AArch64.
fn emit_string_array_fill_aarch64(ctx: &mut FunctionContext<'_>, names: &[String]) {
    ctx.emitter.instruction("str x0, [sp, #-16]!");                             // park the attribute-name array while appending names
    for name in names {
        let (label, len) = ctx.data.add_string(name.as_bytes());
        ctx.emitter.instruction("ldr x0, [sp]");                                // reload the attribute-name array for this append
        abi::emit_symbol_address(ctx.emitter, "x1", &label);
        abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        ctx.emitter.instruction("str x0, [sp]");                                // preserve the possibly-grown attribute-name array
    }
    ctx.emitter.instruction("ldr x0, [sp], #16");                               // restore the final attribute-name array as the result
}

/// Appends attribute-name strings to the current result array on x86_64.
fn emit_string_array_fill_x86_64(ctx: &mut FunctionContext<'_>, names: &[String]) {
    ctx.emitter.instruction("push rax");                                        // park the attribute-name array while appending names
    ctx.emitter.instruction("sub rsp, 8");                                      // keep stack alignment stable across append helper calls
    for name in names {
        let (label, len) = ctx.data.add_string(name.as_bytes());
        ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");                // reload the attribute-name array for this append
        abi::emit_symbol_address(ctx.emitter, "rsi", &label);
        abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
        abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");                // preserve the possibly-grown attribute-name array
    }
    ctx.emitter.instruction("add rsp, 8");                                      // drop the temporary alignment slot
    ctx.emitter.instruction("pop rax");                                         // restore the final attribute-name array as the result
}

/// Allocates and fills a PHP hash array of boxed Mixed attribute arguments.
fn emit_mixed_array(ctx: &mut FunctionContext<'_>, attr_args: &[AttrArgValue]) -> Result<()> {
    allocate_mixed_hash(ctx, attr_args.len().max(1));
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_mixed_array_fill_aarch64(ctx, attr_args),
        Arch::X86_64 => emit_mixed_array_fill_x86_64(ctx, attr_args),
    }
    Ok(())
}

/// Inserts boxed Mixed attribute arguments into the current result hash on AArch64.
fn emit_mixed_array_fill_aarch64(ctx: &mut FunctionContext<'_>, attr_args: &[AttrArgValue]) {
    ctx.emitter.instruction("str x0, [sp, #-16]!");                             // park the attribute-arg hash while boxing values
    for (index, arg) in attr_args.iter().enumerate() {
        emit_box_arg_aarch64(ctx, arg.value());
        ctx.emitter.instruction("mov x3, x0");                                  // pass the boxed argument as the hash value payload
        ctx.emitter.instruction("mov x4, xzr");                                 // boxed Mixed hash entries do not use a high payload word
        abi::emit_load_int_immediate(
            ctx.emitter,
            "x5",
            crate::codegen::runtime_value_tag(&PhpType::Mixed) as i64,
        );
        ctx.emitter.instruction("ldr x0, [sp]");                                // reload the attribute-arg hash for this insertion
        emit_attribute_arg_key_aarch64(ctx, index, arg);
        abi::emit_call_label(ctx.emitter, "__rt_hash_set");
        ctx.emitter.instruction("str x0, [sp]");                                // preserve the possibly-grown attribute-arg hash
    }
    ctx.emitter.instruction("ldr x0, [sp], #16");                               // restore the final attribute-arg hash as the result
}

/// Inserts boxed Mixed attribute arguments into the current result hash on x86_64.
fn emit_mixed_array_fill_x86_64(ctx: &mut FunctionContext<'_>, attr_args: &[AttrArgValue]) {
    ctx.emitter.instruction("push rax");                                        // park the attribute-arg hash while boxing values
    ctx.emitter.instruction("sub rsp, 8");                                      // keep stack alignment stable across helper calls
    for (index, arg) in attr_args.iter().enumerate() {
        emit_box_arg_x86_64(ctx, arg.value());
        ctx.emitter.instruction("mov rcx, rax");                                // pass the boxed argument as the hash value payload
        abi::emit_load_int_immediate(ctx.emitter, "r8", 0);
        abi::emit_load_int_immediate(
            ctx.emitter,
            "r9",
            crate::codegen::runtime_value_tag(&PhpType::Mixed) as i64,
        );
        ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");                // reload the attribute-arg hash for this insertion
        emit_attribute_arg_key_x86_64(ctx, index, arg);
        abi::emit_call_label(ctx.emitter, "__rt_hash_set");
        ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");                // preserve the possibly-grown attribute-arg hash
    }
    ctx.emitter.instruction("add rsp, 8");                                      // drop the temporary alignment slot
    ctx.emitter.instruction("pop rax");                                         // restore the final attribute-arg hash as the result
}

/// Materializes the hash key for one attribute argument on AArch64.
fn emit_attribute_arg_key_aarch64(
    ctx: &mut FunctionContext<'_>,
    index: usize,
    arg: &AttrArgValue,
) {
    if let Some(name) = arg.name() {
        let (label, len) = ctx.data.add_string(name.as_bytes());
        abi::emit_symbol_address(ctx.emitter, "x1", &label);
        abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
    } else {
        abi::emit_load_int_immediate(ctx.emitter, "x1", index as i64);
        abi::emit_load_int_immediate(ctx.emitter, "x2", -1);
    }
}

/// Materializes the hash key for one attribute argument on x86_64.
fn emit_attribute_arg_key_x86_64(
    ctx: &mut FunctionContext<'_>,
    index: usize,
    arg: &AttrArgValue,
) {
    if let Some(name) = arg.name() {
        let (label, len) = ctx.data.add_string(name.as_bytes());
        abi::emit_symbol_address(ctx.emitter, "rsi", &label);
        abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
    } else {
        abi::emit_load_int_immediate(ctx.emitter, "rsi", index as i64);
        abi::emit_load_int_immediate(ctx.emitter, "rdx", -1);
    }
}

/// Allocates a Mixed-valued PHP hash with room for captured attribute args.
fn allocate_mixed_hash(ctx: &mut FunctionContext<'_>, capacity: usize) {
    let capacity = (capacity * 2).max(16);
    let value_tag = crate::codegen::runtime_value_tag(&PhpType::Mixed) as i64;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", value_tag);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", value_tag);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_new");
}

/// Allocates an indexed array with the requested capacity and element stride.
fn allocate_indexed_array(ctx: &mut FunctionContext<'_>, capacity: usize, stride: i64) {
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

/// Boxes one captured attribute argument into the AArch64 Mixed-cell ABI.
fn emit_box_arg_aarch64(ctx: &mut FunctionContext<'_>, arg: &AttrArgValue) {
    match arg.value() {
        AttrArgValue::Null => {
            ctx.emitter.instruction("mov x0, #8");                              // runtime tag 8 = null payload
            ctx.emitter.instruction("mov x1, xzr");                             // null mixed payloads carry no low word
            ctx.emitter.instruction("mov x2, xzr");                             // null mixed payloads carry no high word
        }
        AttrArgValue::Int(value) => {
            ctx.emitter.instruction("mov x0, #0");                              // runtime tag 0 = integer payload
            abi::emit_load_int_immediate(ctx.emitter, "x1", *value);
            ctx.emitter.instruction("mov x2, xzr");                             // integer mixed payloads do not use the high word
        }
        AttrArgValue::Bool(value) => {
            ctx.emitter.instruction("mov x0, #3");                              // runtime tag 3 = boolean payload
            ctx.emitter
                .instruction(&format!("mov x1, #{}", *value as u64));           // pass the captured boolean as the mixed low word
            ctx.emitter.instruction("mov x2, xzr");                             // boolean mixed payloads do not use the high word
        }
        AttrArgValue::Str(value) => {
            let bytes = crate::string_bytes::literal_bytes(value);
            let (label, len) = ctx.data.add_string(&bytes);
            ctx.emitter.instruction("mov x0, #1");                              // runtime tag 1 = string payload
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            ctx.emitter.instruction(&format!("mov x2, #{}", len));              // pass the captured string length as the mixed high word
        }
        AttrArgValue::Named { .. } => unreachable!("named attribute arguments are unwrapped before boxing"),
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
}

/// Boxes one captured attribute argument into the x86_64 Mixed-cell ABI.
fn emit_box_arg_x86_64(ctx: &mut FunctionContext<'_>, arg: &AttrArgValue) {
    match arg.value() {
        AttrArgValue::Null => {
            ctx.emitter.instruction("mov rax, 8");                              // runtime tag 8 = null payload
            ctx.emitter.instruction("xor rdi, rdi");                            // null mixed payloads carry no low word
            ctx.emitter.instruction("xor rsi, rsi");                            // null mixed payloads carry no high word
        }
        AttrArgValue::Int(value) => {
            ctx.emitter.instruction("mov rax, 0");                              // runtime tag 0 = integer payload
            ctx.emitter.instruction(&format!("mov rdi, {}", value));            // pass the captured integer as the mixed low word
            ctx.emitter.instruction("xor rsi, rsi");                            // integer mixed payloads do not use the high word
        }
        AttrArgValue::Bool(value) => {
            ctx.emitter.instruction("mov rax, 3");                              // runtime tag 3 = boolean payload
            ctx.emitter
                .instruction(&format!("mov rdi, {}", *value as u64));           // pass the captured boolean as the mixed low word
            ctx.emitter.instruction("xor rsi, rsi");                            // boolean mixed payloads do not use the high word
        }
        AttrArgValue::Str(value) => {
            let bytes = crate::string_bytes::literal_bytes(value);
            let (label, len) = ctx.data.add_string(&bytes);
            ctx.emitter.instruction("mov rax, 1");                              // runtime tag 1 = string payload
            abi::emit_symbol_address(ctx.emitter, "rdi", &label);
            ctx.emitter.instruction(&format!("mov rsi, {}", len));              // pass the captured string length as the mixed high word
        }
        AttrArgValue::Named { .. } => unreachable!("named attribute arguments are unwrapped before boxing"),
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
}

/// Returns a string literal value defined by a `ConstStr` instruction operand.
fn const_string_operand(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    builtin: &str,
) -> Result<String> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Err(CodegenIrError::unsupported(format!(
            "{} with non-literal string argument",
            builtin
        )));
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if inst_ref.op != Op::ConstStr {
        return Err(CodegenIrError::unsupported(format!(
            "{} with non-literal string argument",
            builtin
        )));
    }
    let Some(Immediate::Data(data)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(format!(
            "{} string literal has no data id",
            builtin
        )));
    };
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))
}
