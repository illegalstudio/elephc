//! Purpose:
//! Lowers EIR buffer allocation and direct buffer opcodes for the ASM backend.
//! Covers the scalar buffer allocation path needed before indexed reads/writes land.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - Buffers are heap headers whose first words match the legacy runtime layout:
//!   logical length, element stride, then contiguous zero-initialized payload.
//! - This module delegates allocation and length checks to target-aware runtime helpers.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::{Immediate, Instruction, LocalSlotId, Op, ValueDef, ValueId};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};

/// Lowers `BufferNew` by passing length and element stride to `__rt_buffer_new`.
pub(super) fn lower_buffer_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "buffer_new", 1)?;
    let len = expect_operand(inst, 0)?;
    let result_ty = result_buffer_type(inst)?;
    let stride = buffer_stride(ctx, &result_ty)?;
    require_int(ctx.load_value_to_result(len)?, "buffer_new length")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov x1, #{}", stride));           // pass the fixed element stride to the buffer allocation helper
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov rdi, {}", stride));           // pass the fixed element stride while keeping the length in rax
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_buffer_new");
    store_if_result(ctx, inst)
}

/// Lowers `buffer_len(buffer)` by delegating live-header validation to the runtime.
pub(super) fn lower_buffer_len(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "buffer_len", 1)?;
    let buffer = expect_operand(inst, 0)?;
    require_buffer(ctx.load_value_to_result(buffer)?, "buffer_len")?;
    abi::emit_call_label(ctx.emitter, "__rt_buffer_len");
    store_if_result(ctx, inst)
}

/// Lowers `buffer_free(buffer)` by freeing the header and nulling the source local slot.
pub(super) fn lower_buffer_free(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "buffer_free", 1)?;
    let buffer = expect_operand(inst, 0)?;
    let slot = source_load_local_slot(ctx, buffer)?.ok_or_else(|| {
        CodegenIrError::unsupported("buffer_free argument that is not a local load")
    })?;
    require_buffer(ctx.load_value_to_result(buffer)?, "buffer_free")?;
    abi::emit_call_label(ctx.emitter, "__rt_heap_free");
    let offset = ctx.local_offset(slot)?;
    abi::emit_store_zero_to_local_slot(ctx.emitter, offset);
    emit_void_result(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `BufferGet` by checking the buffer header and loading the addressed scalar element.
pub(super) fn lower_buffer_get(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "buffer_get", 2)?;
    let buffer = expect_operand(inst, 0)?;
    let index = expect_operand(inst, 1)?;
    let buffer_ty = ctx.load_value_to_result(buffer)?;
    let elem_ty = require_buffer(buffer_ty, "buffer_get")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_int(ctx.load_value_to_result(index)?, "buffer_get index")?;
    let address_reg = materialize_checked_element_address(ctx)?;
    load_element_value(ctx, &elem_ty, address_reg)?;
    store_if_result(ctx, inst)
}

/// Lowers `BufferSet` by checking the buffer header and storing the addressed scalar element.
pub(super) fn lower_buffer_set(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "buffer_set", 3)?;
    let buffer = expect_operand(inst, 0)?;
    let index = expect_operand(inst, 1)?;
    let value = expect_operand(inst, 2)?;
    let buffer_ty = ctx.load_value_to_result(buffer)?;
    let elem_ty = require_buffer(buffer_ty, "buffer_set")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_int(ctx.load_value_to_result(index)?, "buffer_set index")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    materialize_set_value(ctx, value, &elem_ty)?;
    let address_reg = materialize_checked_element_address_for_set(ctx)?;
    store_element_value(ctx, &elem_ty, address_reg)?;
    store_if_result(ctx, inst)
}

/// Returns the declared buffer type for a `BufferNew` result.
fn result_buffer_type(inst: &Instruction) -> Result<PhpType> {
    match inst.result {
        Some(_) => match inst.result_php_type.codegen_repr() {
            PhpType::Buffer(elem) => Ok(PhpType::Buffer(elem)),
            other => Err(CodegenIrError::unsupported(format!(
                "buffer_new result PHP type {:?}",
                other
            ))),
        },
        None => Err(CodegenIrError::invalid_module(
            "buffer_new instruction missing result".to_string(),
        )),
    }
}

/// Returns the fixed runtime stride for supported buffer element types.
fn buffer_stride(ctx: &FunctionContext<'_>, buffer_ty: &PhpType) -> Result<usize> {
    match buffer_ty.codegen_repr() {
        PhpType::Buffer(elem) => match elem.codegen_repr() {
            PhpType::Int
            | PhpType::Float
            | PhpType::Bool
            | PhpType::Pointer(_)
            | PhpType::Resource(_) => Ok(8),
            PhpType::Packed(name) => ctx
                .module
                .packed_class_infos
                .get(&name)
                .map(|info| info.total_size)
                .ok_or_else(|| CodegenIrError::unsupported(format!(
                    "unknown packed buffer element type {}",
                    name
                ))),
            other => Err(CodegenIrError::unsupported(format!(
                "buffer_new element PHP type {:?}",
                other
            ))),
        },
        other => Err(CodegenIrError::unsupported(format!(
            "buffer_new PHP type {:?}",
            other
        ))),
    }
}

/// Verifies a value is represented as a runtime buffer header pointer.
fn require_buffer(ty: PhpType, name: &str) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Buffer(elem_ty) => Ok(*elem_ty),
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name,
            other
        ))),
    }
}

/// Returns the local slot loaded by a buffer operand when it came from `load_local`.
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

/// Materializes a checked payload address for a buffer read.
fn materialize_checked_element_address(ctx: &mut FunctionContext<'_>) -> Result<&'static str> {
    let buffer_reg = abi::symbol_scratch_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x10, x0");                             // preserve the requested index while restoring the buffer header pointer
            abi::emit_pop_reg(ctx.emitter, buffer_reg);
            emit_checked_address_arm64(ctx, buffer_reg, "x10");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, rax");                            // preserve the requested index while restoring the buffer header pointer
            abi::emit_pop_reg(ctx.emitter, buffer_reg);
            emit_checked_address_x86_64(ctx, buffer_reg, "r10");
        }
    }
    Ok(buffer_reg)
}

/// Materializes a checked payload address for a buffer write with value/index slots preserved.
fn materialize_checked_element_address_for_set(ctx: &mut FunctionContext<'_>) -> Result<&'static str> {
    let buffer_reg = abi::symbol_scratch_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x10, [sp]");                           // reload the requested buffer index from the preserved stack slot
            ctx.emitter.instruction("ldr x9, [sp, #16]");                       // reload the buffer header pointer from the preserved stack slot
            emit_checked_address_arm64(ctx, buffer_reg, "x10");
            abi::emit_release_temporary_stack(ctx.emitter, 32);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                // reload the requested buffer index from the preserved stack slot
            ctx.emitter.instruction("mov r11, QWORD PTR [rsp + 16]");           // reload the buffer header pointer from the preserved stack slot
            emit_checked_address_x86_64(ctx, buffer_reg, "r10");
            abi::emit_release_temporary_stack(ctx.emitter, 32);
        }
    }
    Ok(buffer_reg)
}

/// Emits ARM64 null, bounds, and payload-address checks for a buffer element.
fn emit_checked_address_arm64(ctx: &mut FunctionContext<'_>, buffer_reg: &str, index_reg: &str) {
    let uaf_ok = ctx.next_label("buffer_uaf_ok");
    let non_negative = ctx.next_label("buffer_index_non_negative");
    let bounds_ok = ctx.next_label("buffer_index_in_bounds");
    ctx.emitter.instruction(&format!("cbnz {}, {}", buffer_reg, uaf_ok));       // continue only when the buffer header pointer is live
    ctx.emitter.instruction("b __rt_buffer_use_after_free");                    // abort on use after buffer_free() nulled the local
    ctx.emitter.label(&uaf_ok);
    ctx.emitter.instruction(&format!("cmp {}, #0", index_reg));                 // reject negative buffer indexes before touching the payload
    ctx.emitter.instruction(&format!("b.ge {}", non_negative));                 // continue once the requested index is non-negative
    ctx.emitter.instruction("b __rt_buffer_bounds_fail");                       // abort immediately on a negative buffer index
    ctx.emitter.label(&non_negative);
    abi::emit_load_from_address(ctx.emitter, "x11", buffer_reg, 0);
    ctx.emitter.instruction(&format!("cmp {}, x11", index_reg));                // compare the requested index against the logical buffer length
    ctx.emitter.instruction(&format!("b.lo {}", bounds_ok));                    // continue once the requested index is within bounds
    ctx.emitter.instruction("b __rt_buffer_bounds_fail");                       // abort immediately on an out-of-range buffer index
    ctx.emitter.label(&bounds_ok);
    abi::emit_load_from_address(ctx.emitter, "x11", buffer_reg, 8);
    ctx.emitter.instruction(&format!("add {}, {}, #16", buffer_reg, buffer_reg)); //skip the buffer header to reach the contiguous payload base
    ctx.emitter.instruction(&format!("madd {}, {}, x11, {}", buffer_reg, index_reg, buffer_reg)); //compute payload base + index * stride
}

/// Emits x86_64 null, bounds, and payload-address checks for a buffer element.
fn emit_checked_address_x86_64(ctx: &mut FunctionContext<'_>, buffer_reg: &str, index_reg: &str) {
    let uaf_ok = ctx.next_label("buffer_uaf_ok");
    let non_negative = ctx.next_label("buffer_index_non_negative");
    let bounds_ok = ctx.next_label("buffer_index_in_bounds");
    ctx.emitter.instruction(&format!("test {}, {}", buffer_reg, buffer_reg));   // check whether the buffer header pointer is live
    ctx.emitter.instruction(&format!("jne {}", uaf_ok));                        // continue only when the buffer local was not nulled
    ctx.emitter.instruction("jmp __rt_buffer_use_after_free");                  // abort on use after buffer_free() nulled the local
    ctx.emitter.label(&uaf_ok);
    ctx.emitter.instruction(&format!("cmp {}, 0", index_reg));                  // reject negative buffer indexes before touching the payload
    ctx.emitter.instruction(&format!("jge {}", non_negative));                  // continue once the requested index is non-negative
    ctx.emitter.instruction("jmp __rt_buffer_bounds_fail");                     // abort immediately on a negative buffer index
    ctx.emitter.label(&non_negative);
    abi::emit_load_from_address(ctx.emitter, "rcx", buffer_reg, 0);
    ctx.emitter.instruction(&format!("cmp {}, rcx", index_reg));                // compare the requested index against the logical buffer length
    ctx.emitter.instruction(&format!("jl {}", bounds_ok));                      // continue once the requested index is within bounds
    ctx.emitter.instruction("jmp __rt_buffer_bounds_fail");                     // abort immediately on an out-of-range buffer index
    ctx.emitter.label(&bounds_ok);
    abi::emit_load_from_address(ctx.emitter, "rcx", buffer_reg, 8);
    ctx.emitter.instruction(&format!("imul {}, rcx", index_reg));               // scale the requested index by the element stride in bytes
    ctx.emitter.instruction(&format!("add {}, 16", buffer_reg));                // skip the buffer header to reach the contiguous payload base
    ctx.emitter.instruction(&format!("add {}, {}", buffer_reg, index_reg));     // compute payload base + index * stride
}

/// Loads a scalar element from the checked payload address.
fn load_element_value(ctx: &mut FunctionContext<'_>, elem_ty: &PhpType, address_reg: &str) -> Result<()> {
    match elem_ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_load_from_address(ctx.emitter, abi::float_result_reg(ctx.emitter), address_reg, 0);
            Ok(())
        }
        PhpType::Int | PhpType::Bool | PhpType::Pointer(_) | PhpType::Resource(_) => {
            abi::emit_load_from_address(ctx.emitter, abi::int_result_reg(ctx.emitter), address_reg, 0);
            Ok(())
        }
        PhpType::Packed(_) => {
            let result_reg = abi::int_result_reg(ctx.emitter);
            if result_reg != address_reg {
                match ctx.emitter.target.arch {
                    Arch::AArch64 => {
                        ctx.emitter.instruction(&format!("mov {}, {}", result_reg, address_reg)); //return the checked packed-element address as the packed receiver pointer
                    }
                    Arch::X86_64 => {
                        ctx.emitter.instruction(&format!("mov {}, {}", result_reg, address_reg)); //return the checked packed-element address as the packed receiver pointer
                    }
                }
            }
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "buffer_get element PHP type {:?}",
            other
        ))),
    }
}

/// Materializes a scalar value suitable for direct storage into a buffer element.
fn materialize_set_value(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    elem_ty: &PhpType,
) -> Result<()> {
    let value_ty = ctx.load_value_to_result(value)?;
    match (elem_ty.codegen_repr(), value_ty.codegen_repr()) {
        (PhpType::Float, PhpType::Float) => Ok(()),
        (PhpType::Int, PhpType::Int | PhpType::Bool)
        | (PhpType::Bool, PhpType::Int | PhpType::Bool)
        | (PhpType::Pointer(_), PhpType::Pointer(_))
        | (PhpType::Resource(_), PhpType::Resource(_)) => Ok(()),
        (expected, actual) => Err(CodegenIrError::unsupported(format!(
            "buffer_set element {:?} from PHP type {:?}",
            expected,
            actual
        ))),
    }
}

/// Stores the current scalar result into the checked payload address.
fn store_element_value(ctx: &mut FunctionContext<'_>, elem_ty: &PhpType, address_reg: &str) -> Result<()> {
    match elem_ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_store_to_address(ctx.emitter, abi::float_result_reg(ctx.emitter), address_reg, 0);
            Ok(())
        }
        PhpType::Int | PhpType::Bool | PhpType::Pointer(_) | PhpType::Resource(_) => {
            abi::emit_store_to_address(ctx.emitter, abi::int_result_reg(ctx.emitter), address_reg, 0);
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "buffer_set element PHP type {:?}",
            other
        ))),
    }
}

/// Materializes the EIR void/null sentinel for a `buffer_free()` result.
fn emit_void_result(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0x7fff_ffff_ffff_fffe,
    );
}

/// Verifies a value is a concrete integer.
fn require_int(ty: PhpType, name: &str) -> Result<()> {
    match ty.codegen_repr() {
        PhpType::Int => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} PHP type {:?}",
            name,
            other
        ))),
    }
}

/// Verifies a buffer opcode received the expected number of operands.
fn ensure_arg_count(inst: &Instruction, name: &str, expected: usize) -> Result<()> {
    if inst.operands.len() == expected {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "{} expected {} args, got {}",
        name,
        expected,
        inst.operands.len()
    )))
}
