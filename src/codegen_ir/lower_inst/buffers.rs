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
use crate::ir::Instruction;
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};

/// Lowers `BufferNew` by passing length and element stride to `__rt_buffer_new`.
pub(super) fn lower_buffer_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "buffer_new", 1)?;
    let len = expect_operand(inst, 0)?;
    let result_ty = result_buffer_type(inst)?;
    let stride = buffer_stride(&result_ty)?;
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

/// Returns the fixed runtime stride for supported scalar buffer element types.
fn buffer_stride(buffer_ty: &PhpType) -> Result<usize> {
    match buffer_ty.codegen_repr() {
        PhpType::Buffer(elem) => match elem.codegen_repr() {
            PhpType::Int
            | PhpType::Float
            | PhpType::Bool
            | PhpType::Pointer(_)
            | PhpType::Resource(_) => Ok(8),
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
fn require_buffer(ty: PhpType, name: &str) -> Result<()> {
    match ty.codegen_repr() {
        PhpType::Buffer(_) => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name,
            other
        ))),
    }
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
