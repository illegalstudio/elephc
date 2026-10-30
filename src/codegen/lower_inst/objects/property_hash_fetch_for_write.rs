//! Purpose:
//! Fetches an attached writable cell from an ordinary dynamic-property hash.
//!
//! Called from:
//! - The typed PropertyHashFetchForWrite runtime operation after exact class selection.
//!
//! Key details:
//! - Shared hashes and child payloads retain their ordinary COW boundaries.
//! - The existing nested-write helper accepts the transient stack receiver cell.
//! - Creation diagnostics precede mutation and callbacks can replace the owning hash.

use super::*;

/// Returns an owned installed child, publishing a separated hash before the caller mutates it.
pub(super) fn lower_property_hash_fetch_for_write(
    ctx: &mut FunctionContext<'_>, inst: &Instruction,
) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let name = expect_operand(inst, 1)?;
    let (class, offset) = property_hash_layout(ctx, object)?;
    let target = ctx.emitter.target;
    let scratch = abi::symbol_scratch_reg(ctx.emitter);
    let second = abi::secondary_scratch_reg(ctx.emitter);
    let (ptr, len) = abi::string_result_regs(ctx.emitter);
    abi::emit_reserve_temporary_stack(ctx.emitter, 48);
    ctx.load_value_to_reg(object, scratch)?;
    abi::emit_store_to_sp(ctx.emitter, scratch, 24);
    ctx.load_string_value_to_regs(name, ptr, len)?;
    abi::emit_store_to_sp(ctx.emitter, ptr, 32);
    abi::emit_store_to_sp(ctx.emitter, len, 40);
    emit_dynamic_property_creation_deprecation(ctx, &class, offset, 24, 32)?;
    // The warning handler may replace the hash, so select it only after the diagnostic.
    abi::emit_load_temporary_stack_slot(ctx.emitter, scratch, 24);
    abi::emit_load_from_address(ctx.emitter, second, scratch, offset);
    abi::emit_store_to_sp(ctx.emitter, second, 8);
    abi::emit_load_int_immediate(ctx.emitter, second, 5);
    abi::emit_store_to_sp(ctx.emitter, second, 0);
    abi::emit_load_int_immediate(ctx.emitter, second, 0);
    abi::emit_store_to_sp(ctx.emitter, second, 16);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 1), 32);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 2), 40);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, 3), 0);
    abi::emit_temporary_stack_address(ctx.emitter, abi::int_arg_reg_name(target, 0), 0);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_array_get_for_write");
    // This owned result is the stored child, while the stack cell owns the updated hash.
    abi::emit_load_temporary_stack_slot(ctx.emitter, scratch, 24);
    abi::emit_load_temporary_stack_slot(ctx.emitter, second, 8);
    abi::emit_store_to_address(ctx.emitter, second, scratch, offset);
    abi::emit_release_temporary_stack(ctx.emitter, 48);
    store_if_result(ctx, inst)
}

/// Reads entry presence independently of its value or magic property accessors.
pub(super) fn lower_property_hash_contains(
    ctx: &mut FunctionContext<'_>, inst: &Instruction,
) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let name = expect_operand(inst, 1)?;
    let (_, offset) = property_hash_layout(ctx, object)?;
    let target = ctx.emitter.target;
    let (ptr, len) = abi::string_result_regs(ctx.emitter);
    let scratch = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_reserve_temporary_stack(ctx.emitter, 32);
    ctx.load_value_to_reg(object, scratch)?;
    abi::emit_store_to_sp(ctx.emitter, scratch, 0);
    ctx.load_string_value_to_regs(name, ptr, len)?;
    abi::emit_store_to_sp(ctx.emitter, ptr, 8);
    abi::emit_store_to_sp(ctx.emitter, len, 16);
    abi::emit_load_temporary_stack_slot(ctx.emitter, scratch, 0);
    abi::emit_load_from_address(ctx.emitter, abi::int_arg_reg_name(target, 0), scratch, offset);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 1), 8);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 2), 16);
    abi::emit_call_label(ctx.emitter, "__rt_hash_get");
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    let present = ctx.next_label("property_hash_present");
    let done = ctx.next_label("property_hash_presence_done");
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, result, 0);
    emit_branch_if_hash_entry_found(ctx, &present);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&present);
    abi::emit_load_int_immediate(ctx.emitter, result, 1);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Resolves the shared physical hash layout without duplicating allocation offsets.
fn property_hash_layout(ctx: &FunctionContext<'_>, object: ValueId) -> Result<(String, usize)> {
    let PhpType::Object(class) = ctx.value_php_type(object)?.codegen_repr() else {
        return Err(CodegenIrError::invalid_module("property hash fetch requires a concrete object"));
    };
    let class = class.trim_start_matches('\\');
    let property_count = if is_builtin_stdclass(class) { 0 } else {
        let info = ctx.module.class_infos.get(class).ok_or_else(||
            CodegenIrError::unsupported(format!("unknown dynamic-property class {class}")))?;
        if !info.dynamic_property_hash_is_name_addressable() {
            return Err(CodegenIrError::unsupported(format!("class {class} has no ordinary property hash")));
        }
        info.properties.len()
    };
    Ok((class.to_string(), dynamic_property_hash_offset(property_count)))
}
