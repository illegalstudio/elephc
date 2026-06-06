//! Purpose:
//! Lowers basic indexed-array allocation, length reads, and append operations
//! for the Phase 04 EIR backend.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - Runtime append helpers may grow arrays and return a new heap pointer, so
//!   the backend writes that pointer back to the source SSA slot and local slot.

use crate::codegen::{
    abi, emit_box_current_owned_value_as_mixed, emit_box_current_value_as_mixed,
    emit_release_pushed_refcounted_temp_after_array_push, runtime_value_tag,
};
use crate::codegen::platform::Arch;
use crate::ir::{Immediate, Instruction, LocalSlotId, Op, ValueDef, ValueId};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};
use crate::codegen_ir::{CodegenIrError, Result};

/// Lowers indexed-array allocation through the shared runtime constructor.
pub(super) fn lower_array_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let capacity = expect_capacity(inst)?.max(4);
    let result_ty = inst.result_php_type.codegen_repr();
    let elem_ty = indexed_array_element_type(&result_ty, inst)?;
    let elem_size = array_element_size(&result_ty)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", elem_size);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", elem_size);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &elem_ty,
    );
    store_if_result(ctx, inst)
}

/// Lowers an indexed-array length read by loading the first header word.
pub(super) fn lower_array_len(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    require_indexed_array(ctx.load_value_to_result(array)?, inst)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, result_reg, result_reg, 0);
    store_if_result(ctx, inst)
}

/// Lowers typed indexed-array widening to boxed Mixed slots.
pub(super) fn lower_array_to_mixed(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expects exactly one operand",
            inst.op.name()
        )));
    }
    let array = expect_operand(inst, 0)?;
    let elem_ty = indexed_array_element_type(&ctx.value_php_type(array)?, inst)?;
    require_array_to_mixed_result(&inst.result_php_type.codegen_repr(), inst)?;
    let value_tag = runtime_value_tag(&elem_ty) as i64;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            abi::emit_load_int_immediate(ctx.emitter, "x1", value_tag);
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
            abi::emit_load_int_immediate(ctx.emitter, "rsi", value_tag);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
    store_if_result(ctx, inst)
}

/// Lowers indexed-array promotion to associative hash storage.
pub(super) fn lower_array_to_hash(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expects exactly one operand",
            inst.op.name()
        )));
    }
    let array = expect_operand(inst, 0)?;
    require_indexed_array(ctx.value_php_type(array)?.codegen_repr(), inst)?;
    let result_value_ty = require_array_to_hash_result(&inst.result_php_type.codegen_repr(), inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let already_hash = ctx.next_label("array_to_hash_already_hash");
            let convert = ctx.next_label("array_to_hash_convert");
            let done = ctx.next_label("array_to_hash_done");
            ctx.load_value_to_reg(array, "x0")?;
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
            ctx.emitter.instruction("cmp x0, #3");                              // check whether the source is already associative hash storage
            ctx.emitter.instruction(&format!("b.eq {}", already_hash));         // reuse already-promoted hashes without reinterpreting them as indexed arrays
            ctx.emitter.instruction("cmp x0, #2");                              // check whether the source is still indexed-array storage
            ctx.emitter.instruction(&format!("b.eq {}", convert));              // convert indexed arrays to hash storage
            ctx.emitter.label(&already_hash);
            abi::emit_pop_reg(ctx.emitter, "x0");
            if result_value_ty == PhpType::Mixed {
                abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
            }
            ctx.emitter.instruction(&format!("b {}", done));                    // finish after reusing an existing hash payload
            ctx.emitter.label(&convert);
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_load_int_immediate(ctx.emitter, "x0", 16);
            abi::emit_load_int_immediate(ctx.emitter, "x1", runtime_value_tag(&PhpType::Mixed) as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_new");
            ctx.emitter.instruction("mov x1, x0");                              // pass the empty temporary hash as the right union operand
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_push_reg(ctx.emitter, "x1");
            abi::emit_call_label(ctx.emitter, "__rt_array_hash_union");
            abi::emit_pop_reg(ctx.emitter, "x1");
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("mov x0, x1");                              // release the empty temporary hash after the union copy
            abi::emit_call_label(ctx.emitter, "__rt_decref_hash");
            abi::emit_pop_reg(ctx.emitter, "x0");
            if result_value_ty == PhpType::Mixed {
                abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
            }
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            let already_hash = ctx.next_label("array_to_hash_already_hash");
            let convert = ctx.next_label("array_to_hash_convert");
            let done = ctx.next_label("array_to_hash_done");
            ctx.load_value_to_reg(array, "rax")?;
            abi::emit_push_reg(ctx.emitter, "rax");
            abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
            ctx.emitter.instruction("cmp rax, 3");                              // check whether the source is already associative hash storage
            ctx.emitter.instruction(&format!("je {}", already_hash));           // reuse already-promoted hashes without reinterpreting them as indexed arrays
            ctx.emitter.instruction("cmp rax, 2");                              // check whether the source is still indexed-array storage
            ctx.emitter.instruction(&format!("je {}", convert));                // convert indexed arrays to hash storage
            ctx.emitter.label(&already_hash);
            abi::emit_pop_reg(ctx.emitter, "rax");
            if result_value_ty == PhpType::Mixed {
                ctx.emitter.instruction("mov rdi, rax");                        // pass the existing hash to the Mixed-entry conversion helper
                abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
            }
            ctx.emitter.instruction(&format!("jmp {}", done));                  // finish after reusing an existing hash payload
            ctx.emitter.label(&convert);
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_push_reg(ctx.emitter, "rdi");
            abi::emit_load_int_immediate(ctx.emitter, "rdi", 16);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", runtime_value_tag(&PhpType::Mixed) as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_new");
            ctx.emitter.instruction("mov rsi, rax");                            // pass the empty temporary hash as the right union operand
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_push_reg(ctx.emitter, "rsi");
            abi::emit_call_label(ctx.emitter, "__rt_array_hash_union");
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rax, rdi");                            // release the empty temporary hash after the union copy
            abi::emit_call_label(ctx.emitter, "__rt_decref_hash");
            abi::emit_pop_reg(ctx.emitter, "rax");
            if result_value_ty == PhpType::Mixed {
                ctx.emitter.instruction("mov rdi, rax");                        // pass the promoted hash to the Mixed-entry conversion helper
                abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
            }
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers an indexed-array element read with PHP null-sentinel fallback on misses.
pub(super) fn lower_array_get(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let index = expect_operand(inst, 1)?;
    let elem_ty = indexed_array_element_type(&ctx.value_php_type(array)?, inst)?;
    require_array_get_result(&elem_ty, inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_array_get_aarch64(ctx, inst, array, index, &elem_ty),
        Arch::X86_64 => lower_array_get_x86_64(ctx, inst, array, index, &elem_ty),
    }
}

/// Lowers an indexed-array element write through target-aware runtime helpers.
pub(super) fn lower_array_set(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let index = expect_operand(inst, 1)?;
    let value = expect_operand(inst, 2)?;
    let elem_ty = indexed_array_element_type(&ctx.value_php_type(array)?, inst)?;
    let raw_value_ty = ctx.value_php_type(value)?.codegen_repr();
    let value_ty = effective_array_set_value_type(&elem_ty, &raw_value_ty, inst)?;
    require_integer_like_index(ctx.value_php_type(index)?, inst)?;
    let source_local = source_load_local_slot(ctx, array)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_array_set_aarch64(ctx, array, index, value, &raw_value_ty, &value_ty)?,
        Arch::X86_64 => lower_array_set_x86_64(ctx, array, index, value, &raw_value_ty, &value_ty)?,
    }
    ctx.store_result_value(array)?;
    if let Some(slot) = source_local {
        ctx.store_value_to_local(slot, array)?;
    }
    Ok(())
}

/// Lowers an indexed-array append through the runtime helper for the value type.
pub(super) fn lower_array_push(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let value = expect_operand(inst, 1)?;
    let array_ty = ctx.value_php_type(array)?;
    require_indexed_array(array_ty.clone(), inst)?;
    let elem_ty = indexed_array_element_type(&array_ty, inst)?;
    let source_local = source_load_local_slot(ctx, array)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_array_push_aarch64(ctx, array, value, &elem_ty)?,
        Arch::X86_64 => lower_array_push_x86_64(ctx, array, value, &elem_ty)?,
    }
    ctx.store_result_value(array)?;
    if let Some(slot) = source_local {
        ctx.store_value_to_local(slot, array)?;
    }
    Ok(())
}

/// Lowers PHP indexed-array union through the shared runtime helper.
pub(super) fn lower_array_union(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let left = expect_operand(inst, 0)?;
    let right = expect_operand(inst, 1)?;
    require_indexed_array(ctx.value_php_type(left)?, inst)?;
    require_indexed_array(ctx.value_php_type(right)?, inst)?;
    require_indexed_array(inst.result_php_type.codegen_repr(), inst)?;
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
    abi::emit_call_label(ctx.emitter, "__rt_array_union");
    store_if_result(ctx, inst)
}

/// Lowers indexed+associative array union through the shared hash runtime helper.
pub(super) fn lower_array_hash_union(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let left = expect_operand(inst, 0)?;
    let right = expect_operand(inst, 1)?;
    require_indexed_array(ctx.value_php_type(left)?, inst)?;
    require_assoc_union_hash_operand(ctx.value_php_type(right)?, inst)?;
    let result_value_ty = require_array_to_hash_result(&inst.result_php_type.codegen_repr(), inst)?;
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
    abi::emit_call_label(ctx.emitter, "__rt_array_hash_union");
    convert_hash_union_result_to_mixed_if_needed(ctx, &result_value_ty);
    store_if_result(ctx, inst)
}

/// Lowers an indexed-array element read for AArch64 targets.
fn lower_array_get_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    index: ValueId,
    elem_ty: &PhpType,
) -> Result<()> {
    let array_reg = abi::symbol_scratch_reg(ctx.emitter);
    let len_reg = abi::secondary_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(index, result_reg)?;
    ctx.load_value_to_reg(array, array_reg)?;
    let null_label = ctx.next_label("array_get_null");
    let done_label = ctx.next_label("array_get_done");

    ctx.emitter.instruction(&format!("cmp {}, #0", result_reg));                // check whether the indexed-array offset is negative
    ctx.emitter.instruction(&format!("b.lt {}", null_label));                   // negative indexed-array offsets read as null
    abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, 0);
    ctx.emitter.instruction(&format!("cmp {}, {}", result_reg, len_reg));       // compare the requested offset against the indexed-array length
    ctx.emitter.instruction(&format!("b.ge {}", null_label));                   // out-of-range indexed-array offsets read as null
    emit_array_get_in_bounds_aarch64(ctx, array_reg, result_reg, elem_ty)?;
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the null fallback after a successful indexed-array read
    ctx.emitter.label(&null_label);
    emit_array_get_null_fallback(ctx, elem_ty);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers an indexed-array element write for AArch64 targets.
fn lower_array_set_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    index: ValueId,
    value: ValueId,
    raw_value_ty: &PhpType,
    value_ty: &PhpType,
) -> Result<()> {
    if matches!(value_ty, PhpType::Mixed) {
        return lower_mixed_array_set_aarch64(ctx, array, index, value, raw_value_ty);
    }
    ctx.load_value_to_reg(array, "x0")?;
    ctx.load_value_to_reg(index, "x1")?;
    match value_ty {
        PhpType::Int | PhpType::Bool | PhpType::Float => {
            ctx.load_value_to_reg(value, "x2")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_int");
        }
        PhpType::Callable => {
            ctx.load_value_to_reg(value, "x0")?;
            abi::emit_incref_if_refcounted(ctx.emitter, value_ty);
            ctx.emitter.instruction("mov x2, x0");                              // pass an array-owned callable descriptor to the indexed-array setter
            ctx.load_value_to_reg(array, "x0")?;
            ctx.load_value_to_reg(index, "x1")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_int");
        }
        PhpType::Str => {
            ctx.load_string_value_to_regs(value, "x2", "x3")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_str");
        }
        other if other.is_refcounted() => {
            ctx.load_value_to_reg(value, "x2")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_refcounted");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_set value PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Lowers an indexed-array element read for x86_64 targets.
fn lower_array_get_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    index: ValueId,
    elem_ty: &PhpType,
) -> Result<()> {
    let array_reg = abi::symbol_scratch_reg(ctx.emitter);
    let len_reg = abi::secondary_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(array, array_reg)?;
    ctx.load_value_to_reg(index, result_reg)?;
    let null_label = ctx.next_label("array_get_null");
    let done_label = ctx.next_label("array_get_done");

    ctx.emitter.instruction(&format!("cmp {}, 0", result_reg));                 // check whether the indexed-array offset is negative
    ctx.emitter.instruction(&format!("jl {}", null_label));                     // negative indexed-array offsets read as null
    abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, 0);
    ctx.emitter.instruction(&format!("cmp {}, {}", result_reg, len_reg));       // compare the requested offset against the indexed-array length
    ctx.emitter.instruction(&format!("jge {}", null_label));                    // out-of-range indexed-array offsets read as null
    emit_array_get_in_bounds_x86_64(ctx, array_reg, result_reg, elem_ty)?;
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the null fallback after a successful indexed-array read
    ctx.emitter.label(&null_label);
    emit_array_get_null_fallback(ctx, elem_ty);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers an indexed-array element write for x86_64 targets.
fn lower_array_set_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    index: ValueId,
    value: ValueId,
    raw_value_ty: &PhpType,
    value_ty: &PhpType,
) -> Result<()> {
    if matches!(value_ty, PhpType::Mixed) {
        return lower_mixed_array_set_x86_64(ctx, array, index, value, raw_value_ty);
    }
    ctx.load_value_to_reg(array, "rdi")?;
    ctx.load_value_to_reg(index, "rsi")?;
    match value_ty {
        PhpType::Int | PhpType::Bool | PhpType::Float => {
            ctx.load_value_to_reg(value, "rdx")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_int");
        }
        PhpType::Callable => {
            ctx.load_value_to_reg(value, "rax")?;
            abi::emit_incref_if_refcounted(ctx.emitter, value_ty);
            ctx.emitter.instruction("mov rdx, rax");                            // pass an array-owned callable descriptor to the indexed-array setter
            ctx.load_value_to_reg(array, "rdi")?;
            ctx.load_value_to_reg(index, "rsi")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_int");
        }
        PhpType::Str => {
            ctx.load_string_value_to_regs(value, "rdx", "rcx")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_str");
        }
        other if other.is_refcounted() => {
            ctx.load_value_to_reg(value, "rdx")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_refcounted");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_set value PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Emits the in-bounds indexed-array payload load for AArch64.
fn emit_array_get_in_bounds_aarch64(
    ctx: &mut FunctionContext<'_>,
    array_reg: &str,
    index_reg: &str,
    elem_ty: &PhpType,
) -> Result<()> {
    match elem_ty {
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, index_reg, 0x7fff_ffff_ffff_fffe);
        }
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            ctx.emitter.instruction(&format!("add {}, {}, #24", array_reg, array_reg)); // skip the indexed-array header to reach element payloads
            ctx.emitter.instruction(&format!("ldr {}, [{}, {}, lsl #3]", index_reg, array_reg, index_reg)); // load the selected pointer-sized indexed-array element
        }
        PhpType::Float => {
            ctx.emitter.instruction(&format!("add {}, {}, #24", array_reg, array_reg)); // skip the indexed-array header to reach float payloads
            ctx.emitter.instruction(&format!("ldr d0, [{}, {}, lsl #3]", array_reg, index_reg)); // load the selected indexed-array float element
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            ctx.emitter.instruction(&format!("lsl {}, {}, #4", index_reg, index_reg)); // scale the string-array offset by the pointer-plus-length slot size
            ctx.emitter.instruction(&format!("add {}, {}, {}", array_reg, array_reg, index_reg)); // move to the selected string slot within the indexed array
            ctx.emitter.instruction(&format!("add {}, {}, #24", array_reg, array_reg)); // skip the indexed-array header before loading the string slot
            abi::emit_load_from_address(ctx.emitter, ptr_reg, array_reg, 0);
            abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, 8);
        }
        other if other.is_refcounted() => {
            ctx.emitter.instruction(&format!("add {}, {}, #24", array_reg, array_reg)); // skip the indexed-array header to reach pointer payloads
            ctx.emitter.instruction(&format!("ldr {}, [{}, {}, lsl #3]", index_reg, array_reg, index_reg)); // load the selected refcounted indexed-array element
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_get element PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Emits the in-bounds indexed-array payload load for x86_64.
fn emit_array_get_in_bounds_x86_64(
    ctx: &mut FunctionContext<'_>,
    array_reg: &str,
    index_reg: &str,
    elem_ty: &PhpType,
) -> Result<()> {
    match elem_ty {
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, index_reg, 0x7fff_ffff_ffff_fffe);
        }
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            ctx.emitter.instruction(&format!("lea {}, [{} + 24]", array_reg, array_reg)); // skip the indexed-array header to reach element payloads
            ctx.emitter.instruction(&format!("mov {}, QWORD PTR [{} + {} * 8]", index_reg, array_reg, index_reg)); // load the selected pointer-sized indexed-array element
        }
        PhpType::Float => {
            ctx.emitter.instruction(&format!("lea {}, [{} + 24]", array_reg, array_reg)); // skip the indexed-array header to reach float payloads
            ctx.emitter.instruction(&format!("movsd xmm0, QWORD PTR [{} + {} * 8]", array_reg, index_reg)); // load the selected indexed-array float element
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            ctx.emitter.instruction(&format!("shl {}, 4", index_reg));          // scale the string-array offset by the pointer-plus-length slot size
            ctx.emitter.instruction(&format!("add {}, {}", array_reg, index_reg)); // move to the selected string slot within the indexed array
            ctx.emitter.instruction(&format!("add {}, 24", array_reg));         // skip the indexed-array header before loading the string slot
            abi::emit_load_from_address(ctx.emitter, ptr_reg, array_reg, 0);
            abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, 8);
        }
        other if other.is_refcounted() => {
            ctx.emitter.instruction(&format!("lea {}, [{} + 24]", array_reg, array_reg)); // skip the indexed-array header to reach pointer payloads
            ctx.emitter.instruction(&format!("mov {}, QWORD PTR [{} + {} * 8]", index_reg, array_reg, index_reg)); // load the selected refcounted indexed-array element
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_get element PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Emits the null/miss fallback in the result shape expected by the array element type.
fn emit_array_get_null_fallback(ctx: &mut FunctionContext<'_>, elem_ty: &PhpType) {
    match elem_ty {
        PhpType::Float => match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("fmov d0, xzr");                        // materialize a stable zero float for an out-of-bounds array read
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("xorpd xmm0, xmm0");                    // materialize a stable zero float for an out-of-bounds array read
            }
        },
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, ptr_reg, 0);
            abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
        }
        _ => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
        }
    }
}

/// Lowers an indexed-array append for AArch64 targets.
fn lower_array_push_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    value: ValueId,
    elem_ty: &PhpType,
) -> Result<()> {
    let value_ty = ctx.value_php_type(value)?;
    if array_push_value_needs_mixed_box(elem_ty, &value_ty) {
        return lower_mixed_array_push_aarch64(ctx, array, value, &value_ty);
    }
    match value_ty {
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_reg(value, "x1")?;
            ctx.load_value_to_reg(array, "x9")?;
            ctx.emitter.instruction("mov x0, x9");                              // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::Callable => {
            ctx.load_value_to_reg(value, "x0")?;
            abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);
            ctx.emitter.instruction("mov x1, x0");                              // pass an array-owned callable descriptor to the append helper
            ctx.load_value_to_reg(array, "x9")?;
            ctx.emitter.instruction("mov x0, x9");                              // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::Float => {
            ctx.load_value_to_reg(value, "x1")?;
            ctx.load_value_to_reg(array, "x9")?;
            ctx.emitter.instruction("mov x0, x9");                              // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::Str => {
            ctx.load_string_value_to_regs(value, "x1", "x2")?;
            ctx.load_value_to_reg(array, "x9")?;
            ctx.emitter.instruction("mov x0, x9");                              // pass the indexed-array receiver to the string append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        }
        other if other.is_refcounted() => {
            ctx.load_value_to_reg(value, "x1")?;
            ctx.load_value_to_reg(array, "x9")?;
            ctx.emitter.instruction("mov x0, x9");                              // pass the indexed-array receiver to the refcounted append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_push for PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Lowers an indexed-array append for x86_64 targets.
fn lower_array_push_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    value: ValueId,
    elem_ty: &PhpType,
) -> Result<()> {
    let value_ty = ctx.value_php_type(value)?;
    if array_push_value_needs_mixed_box(elem_ty, &value_ty) {
        return lower_mixed_array_push_x86_64(ctx, array, value, &value_ty);
    }
    match value_ty {
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_reg(array, "r11")?;
            ctx.load_value_to_reg(value, "rsi")?;
            ctx.emitter.instruction("mov rdi, r11");                            // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::Callable => {
            ctx.load_value_to_reg(value, "rax")?;
            abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);
            ctx.emitter.instruction("mov rsi, rax");                            // pass an array-owned callable descriptor to the append helper
            ctx.load_value_to_reg(array, "r11")?;
            ctx.emitter.instruction("mov rdi, r11");                            // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::Float => {
            ctx.load_value_to_reg(array, "r11")?;
            ctx.load_value_to_reg(value, "rsi")?;
            ctx.emitter.instruction("mov rdi, r11");                            // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::Str => {
            ctx.load_value_to_reg(array, "r11")?;
            ctx.load_string_value_to_regs(value, "rsi", "rdx")?;
            ctx.emitter.instruction("mov rdi, r11");                            // pass the indexed-array receiver to the string append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        }
        other if other.is_refcounted() => {
            ctx.load_value_to_reg(array, "r11")?;
            ctx.load_value_to_reg(value, "rsi")?;
            ctx.emitter.instruction("mov rdi, r11");                            // pass the indexed-array receiver to the refcounted append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_push for PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Returns true when an append into a Mixed array must box a concrete value first.
fn array_push_value_needs_mixed_box(elem_ty: &PhpType, value_ty: &PhpType) -> bool {
    matches!(elem_ty.codegen_repr(), PhpType::Mixed)
        && !matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
}

/// Boxes a concrete AArch64 value and appends the owned Mixed cell to a Mixed array.
fn lower_mixed_array_push_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    box_value_for_mixed_container(ctx, value, value_ty)?;
    abi::emit_push_reg(ctx.emitter, "x0");
    ctx.load_value_to_reg(array, "x9")?;
    ctx.emitter.instruction("mov x1, x0");                                      // pass the boxed Mixed payload to the refcounted append helper
    ctx.emitter.instruction("mov x0, x9");                                      // pass the indexed-array receiver to the refcounted append helper
    abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
    emit_release_pushed_refcounted_temp_after_array_push(ctx.emitter, &PhpType::Mixed);
    Ok(())
}

/// Boxes a concrete x86_64 value and appends the owned Mixed cell to a Mixed array.
fn lower_mixed_array_push_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    box_value_for_mixed_container(ctx, value, value_ty)?;
    abi::emit_push_reg(ctx.emitter, "rax");
    ctx.load_value_to_reg(array, "r11")?;
    ctx.emitter.instruction("mov rsi, rax");                                    // pass the boxed Mixed payload to the refcounted append helper
    ctx.emitter.instruction("mov rdi, r11");                                    // pass the indexed-array receiver to the refcounted append helper
    abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
    emit_release_pushed_refcounted_temp_after_array_push(ctx.emitter, &PhpType::Mixed);
    Ok(())
}

/// Boxes or retains a value, then stores it into a Mixed indexed array on AArch64.
fn lower_mixed_array_set_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    index: ValueId,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    if matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        ctx.load_value_to_result(value)?;
        abi::emit_incref_if_refcounted(ctx.emitter, value_ty);
    } else {
        box_value_for_mixed_container(ctx, value, value_ty)?;
    }
    abi::emit_push_reg(ctx.emitter, "x0");
    ctx.load_value_to_reg(array, "x0")?;
    ctx.load_value_to_reg(index, "x1")?;
    abi::emit_pop_reg(ctx.emitter, "x2");
    abi::emit_call_label(ctx.emitter, "__rt_array_set_mixed");
    Ok(())
}

/// Boxes or retains a value, then stores it into a Mixed indexed array on x86_64.
fn lower_mixed_array_set_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    index: ValueId,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    if matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        ctx.load_value_to_result(value)?;
        abi::emit_incref_if_refcounted(ctx.emitter, value_ty);
    } else {
        box_value_for_mixed_container(ctx, value, value_ty)?;
    }
    abi::emit_push_reg(ctx.emitter, "rax");
    ctx.load_value_to_reg(array, "rdi")?;
    ctx.load_value_to_reg(index, "rsi")?;
    abi::emit_pop_reg(ctx.emitter, "rdx");
    abi::emit_call_label(ctx.emitter, "__rt_array_set_mixed");
    Ok(())
}

/// Boxes a value for a Mixed array, consuming owned producers when possible.
fn box_value_for_mixed_container(
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

/// Returns the PHP element type for an indexed-array operand.
fn indexed_array_element_type(array_ty: &PhpType, inst: &Instruction) -> Result<PhpType> {
    match array_ty {
        PhpType::Array(elem_ty) => Ok(elem_ty.codegen_repr()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            inst.op.name(),
            other
        ))),
    }
}

/// Resolves the runtime value type that an indexed array write can store.
fn effective_array_set_value_type(
    elem_ty: &PhpType,
    value_ty: &PhpType,
    inst: &Instruction,
) -> Result<PhpType> {
    let elem_ty = elem_ty.codegen_repr();
    let value_ty = value_ty.codegen_repr();
    if matches!(elem_ty, PhpType::Mixed) {
        return Ok(PhpType::Mixed);
    }
    if matches!(elem_ty, PhpType::Never | PhpType::Void) {
        return require_supported_array_set_value(value_ty, inst);
    }
    if elem_ty == value_ty {
        return require_supported_array_set_value(value_ty, inst);
    }
    Err(CodegenIrError::unsupported(format!(
        "array_set element PHP type {:?} with value PHP type {:?}",
        elem_ty, value_ty
    )))
}

/// Rejects indexed-array write payload types that do not have Phase 04 storage lowering yet.
fn require_supported_array_set_value(value_ty: PhpType, inst: &Instruction) -> Result<PhpType> {
    if matches!(
        value_ty,
        PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Float | PhpType::Str
    ) {
        return Ok(value_ty);
    }
    if value_ty.is_refcounted() {
        return Ok(value_ty);
    }
    Err(CodegenIrError::unsupported(format!(
        "{} value PHP type {:?}",
        inst.op.name(),
        value_ty
    )))
}

/// Verifies that an indexed-array write uses an integer-like offset value.
fn require_integer_like_index(index_ty: PhpType, inst: &Instruction) -> Result<()> {
    if matches!(index_ty.codegen_repr(), PhpType::Int | PhpType::Bool | PhpType::Callable) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} index PHP type {:?}",
        inst.op.name(),
        index_ty
    )))
}

/// Rejects array-get result shapes that do not match the lowered array element type.
fn require_array_get_result(elem_ty: &PhpType, inst: &Instruction) -> Result<()> {
    let result_ty = inst.result_php_type.codegen_repr();
    if matches!(elem_ty, PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Float | PhpType::Str)
        && result_ty == *elem_ty
    {
        return Ok(());
    }
    if elem_ty.is_refcounted() && result_ty == *elem_ty {
        return Ok(());
    }
    if matches!(elem_ty, PhpType::Void | PhpType::Never)
        && matches!(result_ty, PhpType::Void | PhpType::Never)
    {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_get element PHP type {:?} with result PHP type {:?}",
        elem_ty, inst.result_php_type
    )))
}

/// Verifies that `array_to_mixed` produces an indexed array with boxed Mixed slots.
fn require_array_to_mixed_result(result_ty: &PhpType, inst: &Instruction) -> Result<()> {
    match result_ty {
        PhpType::Array(elem_ty) if elem_ty.codegen_repr() == PhpType::Mixed => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} result PHP type {:?}",
            inst.op.name(),
            other
        ))),
    }
}

/// Verifies that `array_to_hash` produces associative-array storage.
fn require_array_to_hash_result(result_ty: &PhpType, inst: &Instruction) -> Result<PhpType> {
    match result_ty {
        PhpType::AssocArray { value, .. } => Ok(value.codegen_repr()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} result PHP type {:?}",
            inst.op.name(),
            other
        ))),
    }
}

/// Verifies that a cross-array union operand uses associative hash storage.
fn require_assoc_union_hash_operand(ty: PhpType, inst: &Instruction) -> Result<()> {
    if matches!(ty.codegen_repr(), PhpType::AssocArray { .. }) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} hash operand PHP type {:?}",
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

/// Returns the stack/local slot loaded by an array operand when it came from `load_local`.
fn source_load_local_slot(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<LocalSlotId>> {
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

/// Returns the runtime element-slot width for an indexed-array PHP type.
fn array_element_size(ty: &PhpType) -> Result<i64> {
    match ty {
        PhpType::Array(elem) if matches!(elem.codegen_repr(), PhpType::Str | PhpType::Never) => {
            Ok(16)
        }
        PhpType::Array(_) => Ok(8),
        other => Err(CodegenIrError::unsupported(format!(
            "array_new result PHP type {:?}",
            other
        ))),
    }
}

/// Verifies that an array opcode receives an indexed array.
fn require_indexed_array(ty: PhpType, inst: &Instruction) -> Result<()> {
    if matches!(ty, PhpType::Array(_)) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for PHP type {:?}",
        inst.op.name(),
        ty
    )))
}

/// Returns the capacity immediate attached to an array allocation.
fn expect_capacity(inst: &Instruction) -> Result<u32> {
    match inst.immediate {
        Some(Immediate::Capacity(capacity)) => Ok(capacity),
        _ => Err(CodegenIrError::invalid_module(format!(
            "{} missing capacity immediate",
            inst.op.name()
        ))),
    }
}
