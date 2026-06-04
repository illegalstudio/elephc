//! Purpose:
//! Lowers compiler-extension pointer builtins for the EIR backend.
//! Covers raw null materialization, null tests, and byte-offset address arithmetic.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - Pointer values are raw machine addresses in the integer result register.
//! - These builtins do not allocate, box, retain, or release PHP runtime values.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::Instruction;
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};

/// Lowers `ptr_null()` by materializing the raw null pointer sentinel.
pub(super) fn lower_ptr_null(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "ptr_null", 0)?;
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    store_if_result(ctx, inst)
}

/// Lowers `ptr_is_null(pointer)` by comparing the raw pointer address to zero.
pub(super) fn lower_ptr_is_null(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "ptr_is_null", 1)?;
    let pointer = expect_operand(inst, 0)?;
    require_pointer(ctx.load_value_to_result(pointer)?, "ptr_is_null")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // compare the raw pointer payload against the null address
            ctx.emitter.instruction("cset x0, eq");                             // return true only when the pointer payload is null
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // compare the raw pointer payload against the null address
            ctx.emitter.instruction("sete al");                                 // materialize the null test result in the low byte
            ctx.emitter.instruction("movzx rax, al");                           // widen the null test result to the integer result register
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `ptr_offset(pointer, offset)` by adding a byte offset to a raw address.
pub(super) fn lower_ptr_offset(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "ptr_offset", 2)?;
    let pointer = expect_operand(inst, 0)?;
    let offset = expect_operand(inst, 1)?;
    require_pointer(ctx.load_value_to_result(pointer)?, "ptr_offset")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_integer_offset(ctx.load_value_to_result(offset)?, "ptr_offset")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x10, x0");                             // preserve the byte offset while restoring the base pointer
            abi::emit_pop_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("add x0, x0, x10");                         // compute the derived raw pointer address
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, rax");                            // preserve the byte offset while restoring the base pointer
            abi::emit_pop_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("add rax, r10");                            // compute the derived raw pointer address
        }
    }
    store_if_result(ctx, inst)
}

/// Verifies a pointer builtin received the expected number of operands.
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

/// Verifies a pointer builtin operand has a pointer representation.
fn require_pointer(ty: PhpType, name: &str) -> Result<()> {
    match ty.codegen_repr() {
        PhpType::Pointer(_) => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} for pointer PHP type {:?}",
            name,
            other
        ))),
    }
}

/// Verifies `ptr_offset()` received an integer-like byte offset operand.
fn require_integer_offset(ty: PhpType, name: &str) -> Result<()> {
    match ty.codegen_repr() {
        PhpType::Int | PhpType::Bool => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} offset PHP type {:?}",
            name,
            other
        ))),
    }
}
