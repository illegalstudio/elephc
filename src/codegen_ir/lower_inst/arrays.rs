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

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::{Immediate, Instruction, LocalSlotId, Op, ValueDef, ValueId};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};
use crate::codegen_ir::{CodegenIrError, Result};

/// Lowers indexed-array allocation through the shared runtime constructor.
pub(super) fn lower_array_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let capacity = expect_capacity(inst)?.max(4);
    let elem_size = array_element_size(&inst.result_php_type.codegen_repr())?;
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
    let value_ty = effective_array_set_value_type(&elem_ty, &ctx.value_php_type(value)?, inst)?;
    require_integer_like_index(ctx.value_php_type(index)?, inst)?;
    let source_local = source_load_local_slot(ctx, array)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_array_set_aarch64(ctx, array, index, value, &value_ty)?,
        Arch::X86_64 => lower_array_set_x86_64(ctx, array, index, value, &value_ty)?,
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
    require_indexed_array(ctx.value_php_type(array)?, inst)?;
    let source_local = source_load_local_slot(ctx, array)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_array_push_aarch64(ctx, array, value)?,
        Arch::X86_64 => lower_array_push_x86_64(ctx, array, value)?,
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
    value_ty: &PhpType,
) -> Result<()> {
    ctx.load_value_to_reg(array, "x0")?;
    ctx.load_value_to_reg(index, "x1")?;
    match value_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Float => {
            ctx.load_value_to_reg(value, "x2")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_int");
        }
        PhpType::Str => {
            ctx.load_string_value_to_regs(value, "x2", "x3")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_str");
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
    value_ty: &PhpType,
) -> Result<()> {
    ctx.load_value_to_reg(array, "rdi")?;
    ctx.load_value_to_reg(index, "rsi")?;
    match value_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Float => {
            ctx.load_value_to_reg(value, "rdx")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_int");
        }
        PhpType::Str => {
            ctx.load_string_value_to_regs(value, "rdx", "rcx")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_set_str");
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
) -> Result<()> {
    match ctx.value_php_type(value)? {
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            ctx.load_value_to_reg(value, "x1")?;
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
            ctx.load_value_to_reg(value, "x0")?;
            abi::emit_incref_if_refcounted(ctx.emitter, &other);
            ctx.load_value_to_reg(array, "x9")?;
            ctx.emitter.instruction("mov x1, x0");                              // pass the retained heap payload to the refcounted append helper
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
) -> Result<()> {
    ctx.load_value_to_reg(array, "r11")?;
    match ctx.value_php_type(value)? {
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            ctx.load_value_to_reg(value, "rsi")?;
            ctx.emitter.instruction("mov rdi, r11");                            // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::Float => {
            ctx.load_value_to_reg(value, "rsi")?;
            ctx.emitter.instruction("mov rdi, r11");                            // pass the indexed-array receiver to the append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        }
        PhpType::Str => {
            ctx.load_string_value_to_regs(value, "rsi", "rdx")?;
            ctx.emitter.instruction("mov rdi, r11");                            // pass the indexed-array receiver to the string append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        }
        other if other.is_refcounted() => {
            ctx.load_value_to_reg(value, "rax")?;
            abi::emit_push_reg(ctx.emitter, "r11");
            abi::emit_incref_if_refcounted(ctx.emitter, &other);
            abi::emit_pop_reg(ctx.emitter, "r11");
            ctx.emitter.instruction("mov rsi, rax");                            // pass the retained heap payload to the refcounted append helper
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
    Err(CodegenIrError::unsupported(format!(
        "array_get element PHP type {:?} with result PHP type {:?}",
        elem_ty, inst.result_php_type
    )))
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
