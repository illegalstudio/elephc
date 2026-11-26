//! Purpose:
//! Reads ordinary dynamic-property hashes and selects their attached writable cells.
//!
//! Called from:
//! - Typed property hash read, presence and write-fetch operations after exact class selection.
//!
//! Key details:
//! - Shared hashes and child payloads retain their ordinary COW boundaries.
//! - The existing nested-write helper accepts the transient stack receiver cell.
//! - Creation diagnostics precede mutation and callbacks can replace the owning hash.

use super::*;

/// Returns an owned installed child, publishing a separated hash before the caller mutates it.
pub(in crate::codegen::lower_inst) fn lower_property_hash_fetch_for_write(
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
    emit_dynamic_property_creation_deprecation(
        ctx, &class, offset, 24, 32, inst.span.map_or(0, |span| span.line),
    )?;
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
pub(in crate::codegen::lower_inst) fn lower_property_hash_contains(
    ctx: &mut FunctionContext<'_>, inst: &Instruction,
) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let name = expect_operand(inst, 1)?;
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
    emit_load_readable_property_hash(ctx, object, scratch, abi::int_arg_reg_name(target, 0))?;
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

/// Returns an independent PHP value without getter calls, creation diagnostics or hash mutation.
pub(in crate::codegen::lower_inst) fn lower_property_hash_read(
    ctx: &mut FunctionContext<'_>, inst: &Instruction,
) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let name = expect_operand(inst, 1)?;
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
    emit_load_readable_property_hash(ctx, object, scratch, abi::int_arg_reg_name(target, 0))?;
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 1), 8);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 2), 16);
    abi::emit_call_label(ctx.emitter, "__rt_hash_get");
    let found = ctx.next_label("property_hash_read_found");
    let done = ctx.next_label("property_hash_read_done");
    emit_branch_if_hash_entry_found(ctx, &found);
    emit_boxed_null(ctx);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&found);
    let payload = if target.arch == Arch::AArch64 { "x1" } else { "rdi" };
    abi::emit_reg_move(ctx.emitter, abi::int_result_reg(ctx.emitter), payload);
    // The universal value copy resolves references and preserves shared resource identity.
    abi::emit_call_label(ctx.emitter, "__rt_mixed_clone");
    ctx.emitter.label(&done);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
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

/// Reads a proven dynamic-hash receiver through its real class layout, without nominal retagging.
fn emit_load_readable_property_hash(
    ctx: &mut FunctionContext<'_>, object: ValueId, object_register: &str, result_register: &str,
) -> Result<()> {
    if !matches!(ctx.value_php_type(object)?.codegen_repr(), PhpType::Object(class) if class.is_empty()) {
        let (_, offset) = property_hash_layout(ctx, object)?;
        abi::emit_load_from_address(ctx.emitter, result_register, object_register, offset);
        return Ok(());
    }
    // These storage flags describe the real tail, independently of __debugInfo projections.
    let done = ctx.next_label("property_hash_runtime_layout_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            debug_assert_eq!((object_register, result_register), ("x9", "x0"));
            ctx.emitter.instruction("mov x0, #0");                              // default to an absent dynamic hash
            ctx.emitter.instruction(&format!("cbz x9, {done}"));                // a null receiver has no tail
            ctx.emitter.instruction("ldr x10, [x9]");                           // read the actual object's class id
            abi::emit_symbol_address(ctx.emitter, "x11", "_class_gc_desc_count");
            ctx.emitter.instruction("ldr x11, [x11]");                          // load the runtime class table extent
            ctx.emitter.instruction("cmp x10, x11");                            // reject incomplete or unregistered class ids
            ctx.emitter.instruction(&format!("b.hs {done}"));                   // preserve the absent hash on invalid ids
            abi::emit_symbol_address(ctx.emitter, "x11", "_class_object_dynamic_prop_flags");
            ctx.emitter.instruction("ldr x11, [x11, x10, lsl #3]");             // inspect storage capability rather than dump visibility
            ctx.emitter.instruction(&format!("cbz x11, {done}"));               // a class without a hash cannot be read
            abi::emit_symbol_address(ctx.emitter, "x11", "_class_object_payload_sizes");
            ctx.emitter.instruction("ldr x11, [x11, x10, lsl #3]");             // obtain this class's exact payload size
            ctx.emitter.instruction("sub x11, x11, #8");                        // the dynamic hash occupies the last payload word
            ctx.emitter.instruction("ldr x0, [x9, x11]");                       // borrow the installed hash without mutating it
        }
        Arch::X86_64 => {
            debug_assert_eq!((object_register, result_register), ("r11", "rdi"));
            ctx.emitter.instruction("xor edi, edi");                            // default to an absent dynamic hash
            ctx.emitter.instruction("test r11, r11");                           // determine whether the receiver exists
            ctx.emitter.instruction(&format!("jz {done}"));                     // a null receiver has no tail
            ctx.emitter.instruction("mov r10, QWORD PTR [r11]");                // read the actual object's class id
            abi::emit_symbol_address(ctx.emitter, "rcx", "_class_gc_desc_count");
            ctx.emitter.instruction("cmp r10, QWORD PTR [rcx]");                // reject incomplete or unregistered class ids
            ctx.emitter.instruction(&format!("jae {done}"));                    // preserve the absent hash on invalid ids
            abi::emit_symbol_address(ctx.emitter, "rcx", "_class_object_dynamic_prop_flags");
            ctx.emitter.instruction("cmp QWORD PTR [rcx + r10 * 8], 0");        // inspect storage capability rather than dump visibility
            ctx.emitter.instruction(&format!("je {done}"));                     // a class without a hash cannot be read
            abi::emit_symbol_address(ctx.emitter, "rcx", "_class_object_payload_sizes");
            ctx.emitter.instruction("mov rcx, QWORD PTR [rcx + r10 * 8]");      // obtain this class's exact payload size
            ctx.emitter.instruction("mov rdi, QWORD PTR [r11 + rcx - 8]");      // borrow the installed hash without mutating it
        }
    }
    ctx.emitter.label(&done);
    Ok(())
}
