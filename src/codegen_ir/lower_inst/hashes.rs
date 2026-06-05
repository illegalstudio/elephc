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

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::{Immediate, Instruction, LocalSlotId, Op, ValueDef, ValueId};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};
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

/// Lowers an associative-array lookup with PHP null-sentinel fallback on misses.
pub(super) fn lower_hash_get(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let hash = expect_operand(inst, 0)?;
    let key = expect_operand(inst, 1)?;
    let value_ty = assoc_value_type(&ctx.value_php_type(hash)?, inst)?;
    require_hash_get_result(&value_ty, inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_hash_get_aarch64(ctx, inst, hash, key, &value_ty),
        Arch::X86_64 => lower_hash_get_x86_64(ctx, inst, hash, key, &value_ty),
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

/// Lowers an associative-array lookup for AArch64 targets.
fn lower_hash_get_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    hash: ValueId,
    key: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    materialize_hash_key_aarch64(ctx, key)?;
    ctx.load_value_to_reg(hash, "x0")?;
    abi::emit_call_label(ctx.emitter, "__rt_hash_get");
    let miss = ctx.next_label("hash_get_miss");
    let done = ctx.next_label("hash_get_done");
    ctx.emitter.instruction(&format!("cbz x0, {}", miss));                      // branch to the null fallback when the associative lookup misses
    emit_hash_get_success_aarch64(ctx, value_ty)?;
    ctx.emitter.instruction(&format!("b {}", done));                            // skip the miss fallback after materializing the hash value
    ctx.emitter.label(&miss);
    emit_hash_get_miss(ctx, value_ty);
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
) -> Result<()> {
    materialize_hash_key_x86_64(ctx, key)?;
    ctx.load_value_to_reg(hash, "rdi")?;
    abi::emit_call_label(ctx.emitter, "__rt_hash_get");
    let miss = ctx.next_label("hash_get_miss");
    let done = ctx.next_label("hash_get_done");
    ctx.emitter.instruction("test rax, rax");                                   // check whether the associative lookup found a matching key
    ctx.emitter.instruction(&format!("jz {}", miss));                           // branch to the null fallback when the associative lookup misses
    emit_hash_get_success_x86_64(ctx, value_ty)?;
    ctx.emitter.instruction(&format!("jmp {}", done));                          // skip the miss fallback after materializing the hash value
    ctx.emitter.label(&miss);
    emit_hash_get_miss(ctx, value_ty);
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
    abi::emit_load_int_immediate(ctx.emitter, "x5", crate::codegen::runtime_value_tag(storage_value_ty) as i64);
    abi::emit_call_label(ctx.emitter, "__rt_hash_set");
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
    abi::emit_load_int_immediate(ctx.emitter, "r9", crate::codegen::runtime_value_tag(storage_value_ty) as i64);
    abi::emit_call_label(ctx.emitter, "__rt_hash_set");
    Ok(())
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
        other => Err(CodegenIrError::unsupported(format!(
            "hash key PHP type {:?}",
            other
        ))),
    }
}

/// Materializes an EIR value as the hash-set value payload for AArch64.
fn materialize_hash_value_aarch64(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    value_ty: &PhpType,
    storage_value_ty: &PhpType,
) -> Result<()> {
    if storage_value_ty == &PhpType::Mixed {
        return materialize_hash_mixed_value_aarch64(ctx, value, value_ty);
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
    if storage_value_ty == &PhpType::Mixed {
        return materialize_hash_mixed_value_x86_64(ctx, value, value_ty);
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
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "hash_set value PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Materializes a hash payload as an owned boxed Mixed value for AArch64 hash storage.
fn materialize_hash_mixed_value_aarch64(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    if value_ty == &PhpType::Mixed {
        ctx.load_value_to_reg(value, "x3")?;
        ctx.emitter.instruction("mov x4, xzr");                                 // boxed Mixed hash values do not use the high payload word
        return Ok(());
    }
    ctx.load_value_to_result(value)?;
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, value_ty);
    ctx.emitter.instruction("mov x3, x0");                                      // pass the boxed Mixed pointer as the hash value low word
    ctx.emitter.instruction("mov x4, xzr");                                     // boxed Mixed hash values do not use the high payload word
    Ok(())
}

/// Materializes a hash payload as an owned boxed Mixed value for x86_64 hash storage.
fn materialize_hash_mixed_value_x86_64(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    if value_ty == &PhpType::Mixed {
        ctx.load_value_to_reg(value, "rcx")?;
        ctx.emitter.instruction("xor r8, r8");                                  // boxed Mixed hash values do not use the high payload word
        return Ok(());
    }
    ctx.load_value_to_result(value)?;
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, value_ty);
    ctx.emitter.instruction("mov rcx, rax");                                    // pass the boxed Mixed pointer as the hash value low word
    ctx.emitter.instruction("xor r8, r8");                                      // boxed Mixed hash values do not use the high payload word
    Ok(())
}

/// Moves a successful AArch64 hash lookup payload into the canonical result registers.
fn emit_hash_get_success_aarch64(ctx: &mut FunctionContext<'_>, value_ty: &PhpType) -> Result<()> {
    match value_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            ctx.emitter.instruction("mov x0, x1");                              // move the borrowed hash scalar payload into the standard integer result
        }
        PhpType::Float => {
            ctx.emitter.instruction("fmov d0, x1");                             // move the borrowed hash float bits into the standard float result
        }
        PhpType::Str => {}
        PhpType::Mixed => {
            ctx.emitter.instruction("mov x0, x1");                              // return the boxed Mixed pointer stored in the hash entry
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
fn emit_hash_get_success_x86_64(ctx: &mut FunctionContext<'_>, value_ty: &PhpType) -> Result<()> {
    match value_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            ctx.emitter.instruction("mov rax, rdi");                            // move the borrowed hash scalar payload into the standard integer result
        }
        PhpType::Float => {
            ctx.emitter.instruction("movq xmm0, rdi");                          // move the borrowed hash float bits into the standard float result
        }
        PhpType::Str => {
            ctx.emitter.instruction("mov rax, rdi");                            // move the borrowed hash string pointer into the standard string result
            ctx.emitter.instruction("mov rdx, rsi");                            // move the borrowed hash string length into the paired string result
        }
        PhpType::Mixed => {
            ctx.emitter.instruction("mov rax, rdi");                            // return the boxed Mixed pointer stored in the hash entry
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

/// Emits the miss fallback in the result shape expected by the associative-array value type.
fn emit_hash_get_miss(ctx: &mut FunctionContext<'_>, value_ty: &PhpType) {
    match value_ty {
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
    if matches!(
        value_ty,
        PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Float | PhpType::Str | PhpType::Mixed
    ) && result_ty == *value_ty
    {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "hash_get value PHP type {:?} with result PHP type {:?}",
        value_ty, inst.result_php_type
    )))
}

/// Rejects hash write payload types that do not have Phase 04 storage lowering yet.
fn require_supported_hash_value(
    value_ty: PhpType,
    storage_value_ty: &PhpType,
    inst: &Instruction,
) -> Result<PhpType> {
    let value_ty = value_ty.codegen_repr();
    if storage_value_ty == &PhpType::Mixed
        && (matches!(
            value_ty,
            PhpType::Int
                | PhpType::Bool
                | PhpType::Callable
                | PhpType::Float
                | PhpType::Str
                | PhpType::Void
                | PhpType::Mixed
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
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
