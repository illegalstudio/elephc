//! Purpose:
//! Lowers basic associative-array hash allocation, length reads, lookups, and writes
//! for the Phase 04 EIR backend.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - Hash writes may copy-on-write or grow the table, so the returned pointer is
//!   written back to the source SSA slot and local slot.

use crate::codegen::{
    abi, emit_box_current_owned_value_as_mixed, emit_box_current_value_as_mixed,
};
use crate::codegen::platform::Arch;
use crate::ir::{Immediate, Instruction, LocalSlotId, Op, ValueDef, ValueId};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{expect_operand, load_value_to_first_int_arg, store_if_result};
use crate::codegen_ir::{CodegenIrError, Result};

/// Lowers associative-array allocation through the shared runtime constructor.
pub(super) fn lower_hash_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let capacity = (expect_capacity(inst)? * 2).max(16);
    let value_tag = hash_value_type_tag(&inst.result_php_type)?;
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
    store_if_result(ctx, inst)
}

/// Lowers an associative-array length read by loading the hash count header.
pub(super) fn lower_hash_len(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let hash = expect_operand(inst, 0)?;
    require_hash(ctx.load_value_to_result(hash)?, inst)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, result_reg, result_reg, 0);
    store_if_result(ctx, inst)
}

/// Lowers associative-array widening to boxed Mixed entry payloads.
pub(super) fn lower_hash_to_mixed(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expects exactly one operand",
            inst.op.name()
        )));
    }
    let hash = expect_operand(inst, 0)?;
    require_hash(ctx.value_php_type(hash)?.codegen_repr(), inst)?;
    require_hash_to_mixed_result(&inst.result_php_type.codegen_repr(), inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(hash, "x0")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(hash, "rdi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
    store_if_result(ctx, inst)
}

/// Lowers an associative-array lookup with PHP null-sentinel fallback on misses.
pub(super) fn lower_hash_get(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let hash = expect_operand(inst, 0)?;
    let key = expect_operand(inst, 1)?;
    let value_ty = assoc_value_type(&ctx.value_php_type(hash)?, inst)?;
    require_hash_get_result(&value_ty, inst)?;
    let result_ty = inst.result_php_type.codegen_repr();
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_hash_get_aarch64(ctx, inst, hash, key, &value_ty, &result_ty),
        Arch::X86_64 => lower_hash_get_x86_64(ctx, inst, hash, key, &value_ty, &result_ty),
    }
}

/// Lowers an associative-array insert/update through the shared hash runtime helper.
pub(super) fn lower_hash_set(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let hash = expect_operand(inst, 0)?;
    let key = expect_operand(inst, 1)?;
    let value = expect_operand(inst, 2)?;
    let hash_ty = ctx.value_php_type(hash)?;
    require_hash(hash_ty.clone(), inst)?;
    let storage_value_ty = assoc_value_type(&hash_ty, inst)?;
    let value_ty = require_supported_hash_value(ctx.value_php_type(value)?, &storage_value_ty, inst)?;
    let source_local = source_load_local_slot(ctx, hash)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_hash_set_aarch64(ctx, hash, key, value, &value_ty, &storage_value_ty)?,
        Arch::X86_64 => lower_hash_set_x86_64(ctx, hash, key, value, &value_ty, &storage_value_ty)?,
    }
    ctx.store_result_value(hash)?;
    if let Some(slot) = source_local {
        ctx.store_value_to_local(slot, hash)?;
    }
    Ok(())
}

/// Lowers `unset($hash[$key])` for associative arrays through the shared hash-unset helper.
///
/// Materializes the key into the hash ABI key registers, then calls `__rt_hash_unset`, which
/// copy-on-write splits the table, removes the matching entry (releasing its owned key/value
/// payloads), and returns the unique (possibly cloned) table pointer. That pointer is written
/// back to the source SSA slot and array local, mirroring `lower_hash_set`. A missing key is a
/// runtime no-op.
pub(super) fn lower_hash_unset(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let hash = expect_operand(inst, 0)?;
    let key = expect_operand(inst, 1)?;
    let hash_ty = ctx.value_php_type(hash)?;
    require_hash(hash_ty.clone(), inst)?;
    let source_local = source_load_local_slot(ctx, hash)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            materialize_hash_key_aarch64(ctx, key)?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            ctx.load_value_to_reg(hash, "x0")?;
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            abi::emit_call_label(ctx.emitter, "__rt_hash_unset");
        }
        Arch::X86_64 => {
            materialize_hash_key_x86_64(ctx, key)?;
            abi::emit_push_reg_pair(ctx.emitter, "rsi", "rdx");
            ctx.load_value_to_reg(hash, "rdi")?;
            abi::emit_pop_reg_pair(ctx.emitter, "rsi", "rdx");
            abi::emit_call_label(ctx.emitter, "__rt_hash_unset");
        }
    }
    ctx.store_result_value(hash)?;
    if let Some(slot) = source_local {
        ctx.store_value_to_local(slot, hash)?;
    }
    Ok(())
}

/// Lowers `$hash[] = $value` runtime fallback appends for associative arrays.
pub(super) fn lower_hash_append(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let hash = expect_operand(inst, 0)?;
    let value = expect_operand(inst, 1)?;
    let hash_ty = ctx.value_php_type(hash)?;
    require_hash(hash_ty.clone(), inst)?;
    let storage_value_ty = assoc_value_type(&hash_ty, inst)?;
    let value_ty = require_supported_hash_value(ctx.value_php_type(value)?, &storage_value_ty, inst)?;
    let source_local = source_load_local_slot(ctx, hash)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_hash_append_aarch64(ctx, hash, value, &value_ty, &storage_value_ty)?,
        Arch::X86_64 => lower_hash_append_x86_64(ctx, hash, value, &value_ty, &storage_value_ty)?,
    }
    ctx.store_result_value(hash)?;
    if let Some(slot) = source_local {
        ctx.store_value_to_local(slot, hash)?;
    }
    Ok(())
}

/// Lowers associative+associative array union through the shared hash helper.
pub(super) fn lower_hash_union(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let left = expect_operand(inst, 0)?;
    let right = expect_operand(inst, 1)?;
    require_hash(ctx.value_php_type(left)?, inst)?;
    require_hash(ctx.value_php_type(right)?, inst)?;
    let result_value_ty = require_hash_union_result(&inst.result_php_type.codegen_repr(), inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(left, "x0")?;
            ctx.load_value_to_reg(right, "x1")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(left, "rdi")?;
            ctx.load_value_to_reg(right, "rsi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_union");
    convert_hash_union_result_to_mixed_if_needed(ctx, &result_value_ty);
    store_if_result(ctx, inst)
}

/// Lowers associative+indexed array union through the shared hash helper.
pub(super) fn lower_hash_array_union(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let left = expect_operand(inst, 0)?;
    let right = expect_operand(inst, 1)?;
    require_hash(ctx.value_php_type(left)?, inst)?;
    require_indexed_union_array_operand(ctx.value_php_type(right)?, inst)?;
    let result_value_ty = require_hash_union_result(&inst.result_php_type.codegen_repr(), inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(left, "x0")?;
            ctx.load_value_to_reg(right, "x1")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(left, "rdi")?;
            ctx.load_value_to_reg(right, "rsi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_array_union");
    convert_hash_union_result_to_mixed_if_needed(ctx, &result_value_ty);
    store_if_result(ctx, inst)
}

/// Lowers an associative-array lookup for AArch64 targets.
fn lower_hash_get_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    hash: ValueId,
    key: ValueId,
    value_ty: &PhpType,
    result_ty: &PhpType,
) -> Result<()> {
    materialize_hash_key_aarch64(ctx, key)?;
    ctx.load_value_to_reg(hash, "x0")?;
    abi::emit_call_label(ctx.emitter, "__rt_hash_get");
    let miss = ctx.next_label("hash_get_miss");
    let done = ctx.next_label("hash_get_done");
    ctx.emitter.instruction(&format!("cbz x0, {}", miss));                      // branch to the null fallback when the associative lookup misses
    emit_hash_get_success_aarch64(ctx, value_ty, result_ty)?;
    ctx.emitter.instruction(&format!("b {}", done));                            // skip the miss fallback after materializing the hash value
    ctx.emitter.label(&miss);
    emit_hash_get_miss(ctx, result_ty);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Lowers an associative-array lookup for x86_64 targets.
fn lower_hash_get_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    hash: ValueId,
    key: ValueId,
    value_ty: &PhpType,
    result_ty: &PhpType,
) -> Result<()> {
    materialize_hash_key_x86_64(ctx, key)?;
    ctx.load_value_to_reg(hash, "rdi")?;
    abi::emit_call_label(ctx.emitter, "__rt_hash_get");
    let miss = ctx.next_label("hash_get_miss");
    let done = ctx.next_label("hash_get_done");
    ctx.emitter.instruction("test rax, rax");                                   // check whether the associative lookup found a matching key
    ctx.emitter.instruction(&format!("jz {}", miss));                           // branch to the null fallback when the associative lookup misses
    emit_hash_get_success_x86_64(ctx, value_ty, result_ty)?;
    ctx.emitter.instruction(&format!("jmp {}", done));                          // skip the miss fallback after materializing the hash value
    ctx.emitter.label(&miss);
    emit_hash_get_miss(ctx, result_ty);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Lowers an associative-array write for AArch64 targets.
fn lower_hash_set_aarch64(
    ctx: &mut FunctionContext<'_>,
    hash: ValueId,
    key: ValueId,
    value: ValueId,
    value_ty: &PhpType,
    storage_value_ty: &PhpType,
) -> Result<()> {
    materialize_hash_key_aarch64(ctx, key)?;
    abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
    materialize_hash_value_aarch64(ctx, value, value_ty, storage_value_ty)?;
    abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
    ctx.load_value_to_reg(hash, "x0")?;
    abi::emit_load_int_immediate(ctx.emitter, "x5", hash_set_value_tag(value_ty, storage_value_ty));
    abi::emit_call_label(ctx.emitter, "__rt_hash_set");
    Ok(())
}

/// Lowers an associative-array append for AArch64 targets.
fn lower_hash_append_aarch64(
    ctx: &mut FunctionContext<'_>,
    hash: ValueId,
    value: ValueId,
    value_ty: &PhpType,
    storage_value_ty: &PhpType,
) -> Result<()> {
    ctx.emitter.instruction("sub sp, sp, #32");                                 // reserve temporary slots for the hash pointer and computed append key
    ctx.load_value_to_reg(hash, "x0")?;
    ctx.emitter.instruction("str x0, [sp, #0]");                                // preserve the hash pointer across value materialization
    emit_hash_append_key_scan_aarch64(ctx);
    ctx.emitter.instruction("str x11, [sp, #8]");                               // preserve the next PHP integer append key
    materialize_hash_value_aarch64(ctx, value, value_ty, storage_value_ty)?;
    ctx.emitter.instruction("ldr x0, [sp, #0]");                                // pass the hash pointer to hash_set
    ctx.emitter.instruction("ldr x1, [sp, #8]");                                // pass the computed integer append key
    abi::emit_load_int_immediate(ctx.emitter, "x2", -1);
    abi::emit_load_int_immediate(ctx.emitter, "x5", hash_set_value_tag(value_ty, storage_value_ty));
    abi::emit_call_label(ctx.emitter, "__rt_hash_set");
    ctx.emitter.instruction("add sp, sp, #32");                                 // release append temporaries
    Ok(())
}

/// Lowers an associative-array write for x86_64 targets.
fn lower_hash_set_x86_64(
    ctx: &mut FunctionContext<'_>,
    hash: ValueId,
    key: ValueId,
    value: ValueId,
    value_ty: &PhpType,
    storage_value_ty: &PhpType,
) -> Result<()> {
    materialize_hash_key_x86_64(ctx, key)?;
    abi::emit_push_reg_pair(ctx.emitter, "rsi", "rdx");
    materialize_hash_value_x86_64(ctx, value, value_ty, storage_value_ty)?;
    abi::emit_pop_reg_pair(ctx.emitter, "rsi", "rdx");
    ctx.load_value_to_reg(hash, "rdi")?;
    abi::emit_load_int_immediate(ctx.emitter, "r9", hash_set_value_tag(value_ty, storage_value_ty));
    abi::emit_call_label(ctx.emitter, "__rt_hash_set");
    Ok(())
}

/// Lowers an associative-array append for x86_64 targets.
fn lower_hash_append_x86_64(
    ctx: &mut FunctionContext<'_>,
    hash: ValueId,
    value: ValueId,
    value_ty: &PhpType,
    storage_value_ty: &PhpType,
) -> Result<()> {
    ctx.emitter.instruction("sub rsp, 32");                                     // reserve aligned slots for the hash pointer and computed append key
    ctx.load_value_to_reg(hash, "rdi")?;
    ctx.emitter.instruction("mov QWORD PTR [rsp], rdi");                        // preserve the hash pointer across value materialization
    emit_hash_append_key_scan_x86_64(ctx);
    ctx.emitter.instruction("mov QWORD PTR [rsp + 8], r11");                    // preserve the next PHP integer append key
    materialize_hash_value_x86_64(ctx, value, value_ty, storage_value_ty)?;
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                        // pass the hash pointer to hash_set
    ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");                    // pass the computed integer append key
    abi::emit_load_int_immediate(ctx.emitter, "rdx", -1);
    abi::emit_load_int_immediate(ctx.emitter, "r9", hash_set_value_tag(value_ty, storage_value_ty));
    abi::emit_call_label(ctx.emitter, "__rt_hash_set");
    ctx.emitter.instruction("add rsp, 32");                                     // release append temporaries
    Ok(())
}

/// Computes the next PHP integer append key for the hash pointer in `x0`.
fn emit_hash_append_key_scan_aarch64(ctx: &mut FunctionContext<'_>) {
    let loop_label = ctx.next_label("hash_append_key_loop");
    let update_label = ctx.next_label("hash_append_key_update");
    let next_label = ctx.next_label("hash_append_key_next");
    let done_label = ctx.next_label("hash_append_key_done");

    ctx.emitter.instruction("ldr x10, [x0, #8]");                               // load hash capacity from the header
    ctx.emitter.instruction("mov x9, #0");                                      // start scanning at hash slot zero
    ctx.emitter.instruction("mov x11, #0");                                     // default append key when no integer keys exist
    ctx.emitter.instruction("mov x12, #0");                                     // track whether any integer key has been seen
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp x9, x10");                                     // stop once every allocated hash slot has been checked
    ctx.emitter.instruction(&format!("b.ge {}", done_label));                   // finish scanning when the slot index reaches capacity
    ctx.emitter.instruction("mov x13, #64");                                    // each hash entry occupies 64 bytes
    ctx.emitter.instruction("mul x14, x9, x13");                                // compute byte offset for the current hash slot
    ctx.emitter.instruction("add x14, x0, x14");                                // add the slot offset to the hash base
    ctx.emitter.instruction("add x14, x14, #40");                               // skip the 40-byte hash header to the current entry
    ctx.emitter.instruction("ldr x15, [x14]");                                  // read the occupied flag for this entry
    ctx.emitter.instruction("cmp x15, #1");                                     // only occupied entries can contribute integer keys
    ctx.emitter.instruction(&format!("b.ne {}", next_label));                   // skip empty entries
    ctx.emitter.instruction("ldr x15, [x14, #16]");                             // load key_hi to distinguish integer from string keys
    ctx.emitter.instruction("cmn x15, #1");                                     // integer keys use key_hi = -1
    ctx.emitter.instruction(&format!("b.ne {}", next_label));                   // skip string-keyed entries
    ctx.emitter.instruction("ldr x15, [x14, #8]");                              // load the integer key low word
    ctx.emitter.instruction("add x15, x15, #1");                                // candidate append key is existing integer key plus one
    ctx.emitter.instruction(&format!("cbz x12, {}", update_label));             // first integer key always seeds the append key
    ctx.emitter.instruction("cmp x15, x11");                                    // compare the candidate with the best key so far
    ctx.emitter.instruction(&format!("b.le {}", next_label));                   // keep the existing best key when it is larger
    ctx.emitter.label(&update_label);
    ctx.emitter.instruction("mov x11, x15");                                    // keep the largest observed integer key plus one
    ctx.emitter.instruction("mov x12, #1");                                     // remember that at least one integer key was found
    ctx.emitter.label(&next_label);
    ctx.emitter.instruction("add x9, x9, #1");                                  // advance to the next hash slot
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // continue scanning hash slots
    ctx.emitter.label(&done_label);
}

/// Computes the next PHP integer append key for the hash pointer in `rdi`.
fn emit_hash_append_key_scan_x86_64(ctx: &mut FunctionContext<'_>) {
    let loop_label = ctx.next_label("hash_append_key_loop");
    let update_label = ctx.next_label("hash_append_key_update");
    let next_label = ctx.next_label("hash_append_key_next");
    let done_label = ctx.next_label("hash_append_key_done");

    ctx.emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                    // load hash capacity from the header
    ctx.emitter.instruction("xor r9, r9");                                      // start scanning at hash slot zero
    ctx.emitter.instruction("xor r11, r11");                                    // default append key when no integer keys exist
    ctx.emitter.instruction("xor r8, r8");                                      // track whether any integer key has been seen
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp r9, r10");                                     // stop once every allocated hash slot has been checked
    ctx.emitter.instruction(&format!("jge {}", done_label));                    // finish scanning when the slot index reaches capacity
    ctx.emitter.instruction("mov rcx, r9");                                     // copy the slot index before scaling it
    ctx.emitter.instruction("imul rcx, 64");                                    // compute byte offset for the current hash slot
    ctx.emitter.instruction("lea rax, [rdi + rcx + 40]");                       // compute the current entry address after the hash header
    ctx.emitter.instruction("cmp QWORD PTR [rax], 1");                          // only occupied entries can contribute integer keys
    ctx.emitter.instruction(&format!("jne {}", next_label));                    // skip empty entries
    ctx.emitter.instruction("mov rdx, QWORD PTR [rax + 16]");                   // load key_hi to distinguish integer from string keys
    ctx.emitter.instruction("cmp rdx, -1");                                     // integer keys use key_hi = -1
    ctx.emitter.instruction(&format!("jne {}", next_label));                    // skip string-keyed entries
    ctx.emitter.instruction("mov rcx, QWORD PTR [rax + 8]");                    // load the integer key low word
    ctx.emitter.instruction("add rcx, 1");                                      // candidate append key is existing integer key plus one
    ctx.emitter.instruction("test r8, r8");                                     // has any integer key already seeded the append key?
    ctx.emitter.instruction(&format!("jz {}", update_label));                   // first integer key always seeds the append key
    ctx.emitter.instruction("cmp rcx, r11");                                    // compare the candidate with the best key so far
    ctx.emitter.instruction(&format!("jle {}", next_label));                    // keep the existing best key when it is larger
    ctx.emitter.label(&update_label);
    ctx.emitter.instruction("mov r11, rcx");                                    // keep the largest observed integer key plus one
    ctx.emitter.instruction("mov r8, 1");                                       // remember that at least one integer key was found
    ctx.emitter.label(&next_label);
    ctx.emitter.instruction("add r9, 1");                                       // advance to the next hash slot
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // continue scanning hash slots
    ctx.emitter.label(&done_label);
}

/// Materializes an EIR value as a normalized hash key for AArch64.
pub(super) fn materialize_hash_key_aarch64(ctx: &mut FunctionContext<'_>, key: ValueId) -> Result<()> {
    match ctx.value_php_type(key)? {
        PhpType::Str => {
            ctx.load_string_value_to_regs(key, "x1", "x2")?;
            abi::emit_call_label(ctx.emitter, "__rt_hash_normalize_key");
            Ok(())
        }
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            ctx.load_value_to_reg(key, "x1")?;
            abi::emit_load_int_immediate(ctx.emitter, "x2", -1);
            Ok(())
        }
        PhpType::Float => {
            ctx.load_value_to_reg(key, "d0")?;
            ctx.emitter.instruction("fcvtzs x1, d0");                           // PHP casts float array keys to integer keys
            abi::emit_load_int_immediate(ctx.emitter, "x2", -1);
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => {
            materialize_mixed_hash_key_aarch64(ctx, key)
        }
        other => Err(CodegenIrError::unsupported(format!(
            "hash key PHP type {:?}",
            other
        ))),
    }
}

/// Materializes an EIR value as a normalized hash key for x86_64.
pub(super) fn materialize_hash_key_x86_64(ctx: &mut FunctionContext<'_>, key: ValueId) -> Result<()> {
    match ctx.value_php_type(key)? {
        PhpType::Str => {
            ctx.load_string_value_to_regs(key, "rax", "rdx")?;
            abi::emit_call_label(ctx.emitter, "__rt_hash_normalize_key");
            ctx.emitter.instruction("mov rsi, rax");                            // move the normalized string-or-integer key low word into the hash ABI register
            Ok(())
        }
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            ctx.load_value_to_reg(key, "rsi")?;
            abi::emit_load_int_immediate(ctx.emitter, "rdx", -1);
            Ok(())
        }
        PhpType::Float => {
            ctx.load_value_to_reg(key, "xmm0")?;
            ctx.emitter.instruction("cvttsd2si rsi, xmm0");                     // PHP casts float array keys to integer keys
            abi::emit_load_int_immediate(ctx.emitter, "rdx", -1);
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => {
            materialize_mixed_hash_key_x86_64(ctx, key)
        }
        other => Err(CodegenIrError::unsupported(format!(
            "hash key PHP type {:?}",
            other
        ))),
    }
}

/// Materializes a boxed Mixed key as the AArch64 hash key pair `x1`/`x2`.
fn materialize_mixed_hash_key_aarch64(
    ctx: &mut FunctionContext<'_>,
    key: ValueId,
) -> Result<()> {
    let string_key = ctx.next_label("mixed_hash_key_string");
    let scalar_key = ctx.next_label("mixed_hash_key_scalar");
    let done = ctx.next_label("mixed_hash_key_done");
    ctx.load_value_to_reg(key, "x0")?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("cmp x0, #1");                                      // string mixed keys need PHP numeric-string normalization
    ctx.emitter.instruction(&format!("b.eq {}", string_key));                   // route string keys through the normal hash-key helper
    ctx.emitter.instruction("cmp x0, #0");                                      // integer mixed keys are already scalar hash keys
    ctx.emitter.instruction(&format!("b.eq {}", scalar_key));                   // keep integer keys as integer hash keys
    ctx.emitter.instruction("cmp x0, #3");                                      // boolean mixed keys normalize like integer keys
    ctx.emitter.instruction(&format!("b.eq {}", scalar_key));                   // keep boolean keys as integer hash keys
    ctx.emitter.instruction("mov x1, #0");                                      // unsupported mixed key tags fall back to integer key zero
    ctx.emitter.label(&scalar_key);
    ctx.emitter.instruction("mov x2, #-1");                                     // key_hi sentinel marks scalar mixed keys as integers
    ctx.emitter.instruction(&format!("b {}", done));                            // skip string-key normalization after scalar selection
    ctx.emitter.label(&string_key);
    abi::emit_call_label(ctx.emitter, "__rt_hash_normalize_key");
    ctx.emitter.label(&done);
    Ok(())
}

/// Materializes a boxed Mixed key as the x86_64 hash key pair `rsi`/`rdx`.
fn materialize_mixed_hash_key_x86_64(
    ctx: &mut FunctionContext<'_>,
    key: ValueId,
) -> Result<()> {
    let string_key = ctx.next_label("mixed_hash_key_string");
    let scalar_key = ctx.next_label("mixed_hash_key_scalar");
    let done = ctx.next_label("mixed_hash_key_done");
    ctx.load_value_to_reg(key, "rax")?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("cmp rax, 1");                                      // string mixed keys need PHP numeric-string normalization
    ctx.emitter.instruction(&format!("je {}", string_key));                     // route string keys through the normal hash-key helper
    ctx.emitter.instruction("cmp rax, 0");                                      // integer mixed keys are already scalar hash keys
    ctx.emitter.instruction(&format!("je {}", scalar_key));                     // keep integer keys as integer hash keys
    ctx.emitter.instruction("cmp rax, 3");                                      // boolean mixed keys normalize like integer keys
    ctx.emitter.instruction(&format!("je {}", scalar_key));                     // keep boolean keys as integer hash keys
    ctx.emitter.instruction("xor esi, esi");                                    // unsupported mixed key tags fall back to integer key zero
    ctx.emitter.instruction("mov rdx, -1");                                     // key_hi sentinel marks fallback mixed keys as integers
    ctx.emitter.instruction(&format!("jmp {}", done));                          // skip string-key normalization after fallback selection
    ctx.emitter.label(&scalar_key);
    ctx.emitter.instruction("mov rsi, rdi");                                    // publish the unboxed scalar payload as key_lo
    ctx.emitter.instruction("mov rdx, -1");                                     // key_hi sentinel marks scalar mixed keys as integers
    ctx.emitter.instruction(&format!("jmp {}", done));                          // skip string-key normalization after scalar selection
    ctx.emitter.label(&string_key);
    ctx.emitter.instruction("mov rax, rdi");                                    // move the unboxed string pointer into the hash normalizer input
    abi::emit_call_label(ctx.emitter, "__rt_hash_normalize_key");
    ctx.emitter.instruction("mov rsi, rax");                                    // move normalized key_lo into the hash-set ABI register
    ctx.emitter.label(&done);
    Ok(())
}

/// Materializes an EIR value as the hash-set value payload for AArch64.
fn materialize_hash_value_aarch64(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    value_ty: &PhpType,
    storage_value_ty: &PhpType,
) -> Result<()> {
    if matches!(storage_value_ty, PhpType::Mixed | PhpType::Iterable) {
        return materialize_hash_mixed_value_aarch64(ctx, value, value_ty, storage_value_ty);
    }
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        return materialize_hash_mixed_value_for_concrete_storage_aarch64(ctx, value, storage_value_ty);
    }
    match value_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Float => {
            ctx.load_value_to_reg(value, "x3")?;
            ctx.emitter.instruction("mov x4, xzr");                             // scalar associative-array payloads leave the high value word empty
        }
        PhpType::Str => {
            ctx.load_string_value_to_regs(value, "x1", "x2")?;
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            ctx.emitter.instruction("mov x3, x1");                              // pass the owned string pointer as the hash value low word
            ctx.emitter.instruction("mov x4, x2");                              // pass the owned string length as the hash value high word
        }
        other if hash_refcounted_value_matches_storage(other, storage_value_ty) => {
            ctx.load_value_to_result(value)?;
            retain_hash_refcounted_value_if_borrowed(ctx, value, other)?;
            ctx.emitter.instruction("mov x3, x0");                              // pass the retained pointer-backed payload as the hash value low word
            ctx.emitter.instruction("mov x4, xzr");                             // pointer-backed hash values leave the high value word empty
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "hash_set value PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Materializes an EIR value as the hash-set value payload for x86_64.
fn materialize_hash_value_x86_64(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    value_ty: &PhpType,
    storage_value_ty: &PhpType,
) -> Result<()> {
    if matches!(storage_value_ty, PhpType::Mixed | PhpType::Iterable) {
        return materialize_hash_mixed_value_x86_64(ctx, value, value_ty, storage_value_ty);
    }
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        return materialize_hash_mixed_value_for_concrete_storage_x86_64(ctx, value, storage_value_ty);
    }
    match value_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Float => {
            ctx.load_value_to_reg(value, "rcx")?;
            ctx.emitter.instruction("xor r8, r8");                              // scalar associative-array payloads leave the high value word empty
        }
        PhpType::Str => {
            ctx.load_string_value_to_regs(value, "rax", "rdx")?;
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            ctx.emitter.instruction("mov rcx, rax");                            // pass the owned string pointer as the hash value low word
            ctx.emitter.instruction("mov r8, rdx");                             // pass the owned string length as the hash value high word
        }
        other if hash_refcounted_value_matches_storage(other, storage_value_ty) => {
            ctx.load_value_to_result(value)?;
            retain_hash_refcounted_value_if_borrowed(ctx, value, other)?;
            ctx.emitter.instruction("mov rcx, rax");                            // pass the retained pointer-backed payload as the hash value low word
            ctx.emitter.instruction("xor r8, r8");                              // pointer-backed hash values leave the high value word empty
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "hash_set value PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Casts a boxed Mixed payload into a concrete AArch64 hash-set value payload.
fn materialize_hash_mixed_value_for_concrete_storage_aarch64(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    storage_value_ty: &PhpType,
) -> Result<()> {
    match storage_value_ty.codegen_repr() {
        PhpType::Int => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            ctx.emitter.instruction("mov x3, x0");                              // pass the cast integer payload as the hash value low word
            ctx.emitter.instruction("mov x4, xzr");                             // cast scalar hash values leave the high value word empty
        }
        PhpType::Bool => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
            ctx.emitter.instruction("mov x3, x0");                              // pass the cast boolean payload as the hash value low word
            ctx.emitter.instruction("mov x4, xzr");                             // cast scalar hash values leave the high value word empty
        }
        PhpType::Float => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float");
            ctx.emitter.instruction("fmov x3, d0");                             // pass the cast float bits as the hash value low word
            ctx.emitter.instruction("mov x4, xzr");                             // cast scalar hash values leave the high value word empty
        }
        PhpType::Str => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            ctx.emitter.instruction("mov x3, x1");                              // pass the persisted string pointer as the hash value low word
            ctx.emitter.instruction("mov x4, x2");                              // pass the persisted string length as the hash value high word
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "mixed hash_set value for concrete PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Casts a boxed Mixed payload into a concrete x86_64 hash-set value payload.
fn materialize_hash_mixed_value_for_concrete_storage_x86_64(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    storage_value_ty: &PhpType,
) -> Result<()> {
    match storage_value_ty.codegen_repr() {
        PhpType::Int => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            ctx.emitter.instruction("mov rcx, rax");                            // pass the cast integer payload as the hash value low word
            ctx.emitter.instruction("xor r8, r8");                              // cast scalar hash values leave the high value word empty
        }
        PhpType::Bool => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
            ctx.emitter.instruction("mov rcx, rax");                            // pass the cast boolean payload as the hash value low word
            ctx.emitter.instruction("xor r8, r8");                              // cast scalar hash values leave the high value word empty
        }
        PhpType::Float => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float");
            ctx.emitter.instruction("movq rcx, xmm0");                          // pass the cast float bits as the hash value low word
            ctx.emitter.instruction("xor r8, r8");                              // cast scalar hash values leave the high value word empty
        }
        PhpType::Str => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            ctx.emitter.instruction("mov rcx, rax");                            // pass the persisted string pointer as the hash value low word
            ctx.emitter.instruction("mov r8, rdx");                             // pass the persisted string length as the hash value high word
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "mixed hash_set value for concrete PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Retains a hash value unless the source producer transfers ownership into the table.
fn retain_hash_refcounted_value_if_borrowed(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    if !ctx.value_can_own_mixed_box_source(value)? {
        abi::emit_incref_if_refcounted(ctx.emitter, value_ty);
    }
    Ok(())
}

/// Returns true when a refcounted hash value has the same runtime storage family as the slot.
fn hash_refcounted_value_matches_storage(value_ty: &PhpType, storage_value_ty: &PhpType) -> bool {
    if !value_ty.is_refcounted() {
        return false;
    }
    match (value_ty.codegen_repr(), storage_value_ty.codegen_repr()) {
        (left, right) if left == right => true,
        (PhpType::Array(_), PhpType::Array(_)) => true,
        (PhpType::AssocArray { .. }, PhpType::AssocArray { .. }) => true,
        (PhpType::Object(_), PhpType::Object(_)) => true,
        _ => false,
    }
}

/// Materializes a hash payload as an owned boxed Mixed value for AArch64 hash storage.
fn materialize_hash_mixed_value_aarch64(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    value_ty: &PhpType,
    storage_value_ty: &PhpType,
) -> Result<()> {
    if value_ty == &PhpType::Mixed {
        ctx.load_value_to_result(value)?;
        retain_hash_refcounted_value_if_borrowed(ctx, value, value_ty)?;
        ctx.emitter.instruction("mov x3, x0");                                  // pass the retained boxed Mixed pointer as the hash value low word
        ctx.emitter.instruction("mov x4, xzr");                                 // boxed Mixed hash values do not use the high payload word
        return Ok(());
    }
    if storage_value_ty == &PhpType::Mixed && value_ty == &PhpType::Iterable {
        box_hash_value_for_mixed_storage(ctx, value, value_ty)?;
        ctx.emitter.instruction("mov x3, x0");                                  // pass the boxed iterable Mixed cell as the hash value low word
        ctx.emitter.instruction("mov x4, xzr");                                 // boxed iterable Mixed cells do not use the high payload word
        return Ok(());
    }
    if matches!(storage_value_ty, PhpType::Mixed | PhpType::Iterable)
        && value_ty == &PhpType::TaggedScalar
    {
        box_hash_value_for_mixed_storage(ctx, value, value_ty)?;
        ctx.emitter.instruction("mov x3, x0");                                  // pass the boxed tagged-scalar Mixed cell as the hash value low word
        ctx.emitter.instruction("mov x4, xzr");                                 // boxed tagged-scalar Mixed cells do not use the high payload word
        return Ok(());
    }
    materialize_hash_concrete_value_aarch64(ctx, value, value_ty)
}

/// Materializes a hash payload as an owned boxed Mixed value for x86_64 hash storage.
fn materialize_hash_mixed_value_x86_64(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    value_ty: &PhpType,
    storage_value_ty: &PhpType,
) -> Result<()> {
    if value_ty == &PhpType::Mixed {
        ctx.load_value_to_result(value)?;
        retain_hash_refcounted_value_if_borrowed(ctx, value, value_ty)?;
        ctx.emitter.instruction("mov rcx, rax");                                // pass the retained boxed Mixed pointer as the hash value low word
        ctx.emitter.instruction("xor r8, r8");                                  // boxed Mixed hash values do not use the high payload word
        return Ok(());
    }
    if storage_value_ty == &PhpType::Mixed && value_ty == &PhpType::Iterable {
        box_hash_value_for_mixed_storage(ctx, value, value_ty)?;
        ctx.emitter.instruction("mov rcx, rax");                                // pass the boxed iterable Mixed cell as the hash value low word
        ctx.emitter.instruction("xor r8, r8");                                  // boxed iterable Mixed cells do not use the high payload word
        return Ok(());
    }
    if matches!(storage_value_ty, PhpType::Mixed | PhpType::Iterable)
        && value_ty == &PhpType::TaggedScalar
    {
        box_hash_value_for_mixed_storage(ctx, value, value_ty)?;
        ctx.emitter.instruction("mov rcx, rax");                                // pass the boxed tagged-scalar Mixed cell as the hash value low word
        ctx.emitter.instruction("xor r8, r8");                                  // boxed tagged-scalar Mixed cells do not use the high payload word
        return Ok(());
    }
    materialize_hash_concrete_value_x86_64(ctx, value, value_ty)
}

/// Boxes an EIR value into a Mixed cell for Mixed-valued associative-array storage.
fn box_hash_value_for_mixed_storage(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    ctx.load_value_to_result(value)?;
    if ctx.value_can_own_mixed_box_source(value)? {
        emit_box_current_owned_value_as_mixed(ctx.emitter, &value_ty.codegen_repr());
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, &value_ty.codegen_repr());
    }
    Ok(())
}

/// Returns the runtime value tag to store for one hash-set payload.
fn hash_set_value_tag(value_ty: &PhpType, storage_value_ty: &PhpType) -> i64 {
    if matches!(storage_value_ty, PhpType::Mixed | PhpType::Iterable) {
        if value_ty.codegen_repr() == PhpType::TaggedScalar {
            return crate::codegen::runtime_value_tag(&PhpType::Mixed) as i64;
        }
        crate::codegen::runtime_value_tag(&value_ty.codegen_repr()) as i64
    } else {
        crate::codegen::runtime_value_tag(storage_value_ty) as i64
    }
}

/// Materializes a concrete payload for a Mixed-capable AArch64 hash entry.
fn materialize_hash_concrete_value_aarch64(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    match value_ty {
        PhpType::Void | PhpType::Never => {
            ctx.emitter.instruction("mov x3, xzr");                             // null associative-array payloads use a zero low word
            ctx.emitter.instruction("mov x4, xzr");                             // null associative-array payloads use a zero high word
        }
        PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Float => {
            ctx.load_value_to_reg(value, "x3")?;
            ctx.emitter.instruction("mov x4, xzr");                             // scalar associative-array payloads leave the high value word empty
        }
        PhpType::Str => {
            ctx.load_string_value_to_regs(value, "x1", "x2")?;
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            ctx.emitter.instruction("mov x3, x1");                              // pass the owned string pointer as the hash value low word
            ctx.emitter.instruction("mov x4, x2");                              // pass the owned string length as the hash value high word
        }
        other if other.is_refcounted() => {
            ctx.load_value_to_result(value)?;
            retain_hash_refcounted_value_if_borrowed(ctx, value, other)?;
            ctx.emitter.instruction("mov x3, x0");                              // pass the retained pointer-backed payload as the hash value low word
            ctx.emitter.instruction("mov x4, xzr");                             // pointer-backed hash values leave the high value word empty
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "mixed hash_set value PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Materializes a concrete payload for a Mixed-capable x86_64 hash entry.
fn materialize_hash_concrete_value_x86_64(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    match value_ty {
        PhpType::Void | PhpType::Never => {
            ctx.emitter.instruction("xor rcx, rcx");                            // null associative-array payloads use a zero low word
            ctx.emitter.instruction("xor r8, r8");                              // null associative-array payloads use a zero high word
        }
        PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Float => {
            ctx.load_value_to_reg(value, "rcx")?;
            ctx.emitter.instruction("xor r8, r8");                              // scalar associative-array payloads leave the high value word empty
        }
        PhpType::Str => {
            ctx.load_string_value_to_regs(value, "rax", "rdx")?;
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            ctx.emitter.instruction("mov rcx, rax");                            // pass the owned string pointer as the hash value low word
            ctx.emitter.instruction("mov r8, rdx");                             // pass the owned string length as the hash value high word
        }
        other if other.is_refcounted() => {
            ctx.load_value_to_result(value)?;
            retain_hash_refcounted_value_if_borrowed(ctx, value, other)?;
            ctx.emitter.instruction("mov rcx, rax");                            // pass the retained pointer-backed payload as the hash value low word
            ctx.emitter.instruction("xor r8, r8");                              // pointer-backed hash values leave the high value word empty
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "mixed hash_set value PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Moves a successful AArch64 hash lookup payload into the canonical result registers.
fn emit_hash_get_success_aarch64(
    ctx: &mut FunctionContext<'_>,
    value_ty: &PhpType,
    result_ty: &PhpType,
) -> Result<()> {
    match value_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            ctx.emitter.instruction("mov x0, x1");                              // move the borrowed hash scalar payload into the standard integer result
            if matches!(result_ty, PhpType::TaggedScalar) {
                crate::codegen::sentinels::emit_tagged_scalar_from_int_result(ctx.emitter);
            }
        }
        PhpType::Float => {
            ctx.emitter.instruction("fmov d0, x1");                             // move the borrowed hash float bits into the standard float result
        }
        PhpType::Str => {}
        PhpType::Mixed => {
            emit_hash_get_mixed_success_aarch64(ctx);
        }
        other if other.is_refcounted() => {
            ctx.emitter.instruction("mov x0, x1");                              // return the borrowed pointer-backed hash payload
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "hash_get value PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Moves a successful x86_64 hash lookup payload into the canonical result registers.
fn emit_hash_get_success_x86_64(
    ctx: &mut FunctionContext<'_>,
    value_ty: &PhpType,
    result_ty: &PhpType,
) -> Result<()> {
    match value_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            ctx.emitter.instruction("mov rax, rdi");                            // move the borrowed hash scalar payload into the standard integer result
            if matches!(result_ty, PhpType::TaggedScalar) {
                crate::codegen::sentinels::emit_tagged_scalar_from_int_result(ctx.emitter);
            }
        }
        PhpType::Float => {
            ctx.emitter.instruction("movq xmm0, rdi");                          // move the borrowed hash float bits into the standard float result
        }
        PhpType::Str => {
            ctx.emitter.instruction("mov rax, rdi");                            // move the borrowed hash string pointer into the standard string result
            ctx.emitter.instruction("mov rdx, rsi");                            // move the borrowed hash string length into the paired string result
        }
        PhpType::Mixed => {
            emit_hash_get_mixed_success_x86_64(ctx);
        }
        other if other.is_refcounted() => {
            ctx.emitter.instruction("mov rax, rdi");                            // return the borrowed pointer-backed hash payload
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "hash_get value PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Materializes a successful AArch64 Mixed hash lookup as a boxed Mixed result.
fn emit_hash_get_mixed_success_aarch64(ctx: &mut FunctionContext<'_>) {
    let box_label = ctx.next_label("hash_get_mixed_box");
    let done_label = ctx.next_label("hash_get_mixed_done");
    ctx.emitter.instruction("cmp x3, #7");                                      // check whether the entry already stores a boxed Mixed cell
    ctx.emitter.instruction(&format!("b.ne {}", box_label));                    // box concrete per-entry payloads before returning them as Mixed
    ctx.emitter.instruction("mov x0, x1");                                      // return the boxed Mixed pointer stored in the hash entry
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip on-demand boxing for already boxed entries
    ctx.emitter.label(&box_label);
    ctx.emitter.instruction("mov x0, x3");                                      // pass the concrete entry tag to the Mixed boxing helper
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.label(&done_label);
}

/// Materializes a successful x86_64 Mixed hash lookup as a boxed Mixed result.
fn emit_hash_get_mixed_success_x86_64(ctx: &mut FunctionContext<'_>) {
    let box_label = ctx.next_label("hash_get_mixed_box");
    let done_label = ctx.next_label("hash_get_mixed_done");
    ctx.emitter.instruction("cmp rcx, 7");                                      // check whether the entry already stores a boxed Mixed cell
    ctx.emitter.instruction(&format!("jne {}", box_label));                     // box concrete per-entry payloads before returning them as Mixed
    ctx.emitter.instruction("mov rax, rdi");                                    // return the boxed Mixed pointer stored in the hash entry
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip on-demand boxing for already boxed entries
    ctx.emitter.label(&box_label);
    ctx.emitter.instruction("mov rax, rcx");                                    // pass the concrete entry tag to the Mixed boxing helper
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.label(&done_label);
}

/// Emits the miss fallback in the result shape expected by the associative-array value type.
fn emit_hash_get_miss(ctx: &mut FunctionContext<'_>, value_ty: &PhpType) {
    match value_ty {
        PhpType::TaggedScalar => {
            crate::codegen::sentinels::emit_tagged_scalar_null(ctx.emitter);
        }
        PhpType::Float => match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("fmov d0, xzr");                        // materialize a stable zero float for a missing associative-array read
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("xorpd xmm0, xmm0");                    // materialize a stable zero float for a missing associative-array read
            }
        },
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, ptr_reg, 0);
            abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
        }
        PhpType::Mixed => match ctx.emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_load_int_immediate(ctx.emitter, "x0", 8);
                abi::emit_load_int_immediate(ctx.emitter, "x1", 0);
                abi::emit_load_int_immediate(ctx.emitter, "x2", 0);
                abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            }
            Arch::X86_64 => {
                abi::emit_load_int_immediate(ctx.emitter, "rax", 8);
                abi::emit_load_int_immediate(ctx.emitter, "rdi", 0);
                abi::emit_load_int_immediate(ctx.emitter, "rsi", 0);
                abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            }
        },
        _ => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
        }
    }
}

/// Returns the runtime value tag for a hash allocation result type.
fn hash_value_type_tag(hash_ty: &PhpType) -> Result<i64> {
    match hash_ty.codegen_repr() {
        PhpType::AssocArray { value, .. } => {
            Ok(crate::codegen::runtime_value_tag(&value.codegen_repr()) as i64)
        }
        other => Err(CodegenIrError::unsupported(format!(
            "hash_new result PHP type {:?}",
            other
        ))),
    }
}

/// Returns the static value type for an associative-array operand.
fn assoc_value_type(hash_ty: &PhpType, inst: &Instruction) -> Result<PhpType> {
    match hash_ty.codegen_repr() {
        PhpType::AssocArray { value, .. } => Ok(value.codegen_repr()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            inst.op.name(),
            other
        ))),
    }
}

/// Verifies that a hash opcode receives an associative array.
fn require_hash(ty: PhpType, inst: &Instruction) -> Result<()> {
    if matches!(ty.codegen_repr(), PhpType::AssocArray { .. }) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for PHP type {:?}",
        inst.op.name(),
        ty
    )))
}

/// Rejects hash-get result shapes that do not match the lowered hash value type.
fn require_hash_get_result(value_ty: &PhpType, inst: &Instruction) -> Result<()> {
    let result_ty = inst.result_php_type.codegen_repr();
    if crate::codegen::sentinels::null_repr_is_tagged()
        && matches!(value_ty, PhpType::Int)
        && result_ty == PhpType::TaggedScalar
    {
        return Ok(());
    }
    if matches!(
        value_ty,
            PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Float | PhpType::Str | PhpType::Mixed
    ) && result_ty == *value_ty
    {
        return Ok(());
    }
    if value_ty.is_refcounted() && result_ty == *value_ty {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "hash_get value PHP type {:?} with result PHP type {:?}",
        value_ty, inst.result_php_type
    )))
}

/// Verifies that `hash_to_mixed` produces a Mixed-valued associative array.
fn require_hash_to_mixed_result(result_ty: &PhpType, inst: &Instruction) -> Result<()> {
    match result_ty {
        PhpType::AssocArray { value, .. } if value.codegen_repr() == PhpType::Mixed => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} result PHP type {:?}",
            inst.op.name(),
            other
        ))),
    }
}

/// Verifies that a hash-union opcode produces associative-array storage.
fn require_hash_union_result(result_ty: &PhpType, inst: &Instruction) -> Result<PhpType> {
    match result_ty {
        PhpType::AssocArray { value, .. } => Ok(value.codegen_repr()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} result PHP type {:?}",
            inst.op.name(),
            other
        ))),
    }
}

/// Verifies that a cross-array union operand uses indexed-array storage.
fn require_indexed_union_array_operand(ty: PhpType, inst: &Instruction) -> Result<()> {
    if matches!(ty.codegen_repr(), PhpType::Array(_)) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} array operand PHP type {:?}",
        inst.op.name(),
        ty
    )))
}

/// Converts a just-returned hash union result to boxed Mixed entries when required.
fn convert_hash_union_result_to_mixed_if_needed(
    ctx: &mut FunctionContext<'_>,
    result_value_ty: &PhpType,
) {
    if result_value_ty != &PhpType::Mixed {
        return;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the hash result to the Mixed-entry conversion helper
            abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
        }
    }
}

/// Rejects hash write payload types that do not have Phase 04 storage lowering yet.
fn require_supported_hash_value(
    value_ty: PhpType,
    storage_value_ty: &PhpType,
    inst: &Instruction,
) -> Result<PhpType> {
    let value_ty = value_ty.codegen_repr();
    if matches!(storage_value_ty, PhpType::Mixed | PhpType::Iterable)
        && (matches!(
            value_ty,
            PhpType::Int
                | PhpType::Bool
                | PhpType::Callable
                | PhpType::Float
                | PhpType::Str
                | PhpType::Void
                | PhpType::Mixed
                | PhpType::TaggedScalar
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Iterable
                | PhpType::Object(_)
        ))
    {
        return Ok(value_ty);
    }
    if matches!(
        value_ty,
        PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Float | PhpType::Str
    ) {
        return Ok(value_ty);
    }
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_))
        && matches!(
            storage_value_ty.codegen_repr(),
            PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Str
        )
    {
        return Ok(value_ty);
    }
    if hash_refcounted_value_matches_storage(&value_ty, storage_value_ty) {
        return Ok(value_ty);
    }
    Err(CodegenIrError::unsupported(format!(
        "{} value PHP type {:?}",
        inst.op.name(),
        value_ty
    )))
}

/// Returns the stack/local slot loaded by a hash operand when it came from `load_local`.
fn source_load_local_slot(ctx: &FunctionContext<'_>, value: ValueId) -> Result<Option<LocalSlotId>> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let Some(inst_ref) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    if inst_ref.op == Op::LoadLocal {
        if let Some(Immediate::LocalSlot(slot)) = inst_ref.immediate {
            return Ok(Some(slot));
        }
    }
    Ok(None)
}

/// Returns the capacity immediate attached to a hash allocation.
fn expect_capacity(inst: &Instruction) -> Result<u32> {
    match inst.immediate {
        Some(Immediate::Capacity(capacity)) => Ok(capacity),
        _ => Err(CodegenIrError::invalid_module(format!(
            "{} missing capacity immediate",
            inst.op.name()
        ))),
    }
}
