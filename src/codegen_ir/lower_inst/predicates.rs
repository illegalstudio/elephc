//! Purpose:
//! Lowers scalar predicate EIR opcodes such as null checks and PHP truthiness.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - PHP string truthiness is special: only `""` and `"0"` are false.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};
use crate::codegen_ir::{CodegenIrError, Result};

/// Lowers scalar PHP truthiness into a concrete boolean integer result.
pub(super) fn lower_is_truthy(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)? {
        PhpType::Bool | PhpType::Int => {
            ctx.load_value_to_result(value)?;
            emit_int_result_nonzero_bool(ctx);
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            emit_float_result_nonzero_bool(ctx);
        }
        PhpType::Str => emit_string_truthiness(ctx, value)?,
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "{} for PHP type {:?}",
                inst.op.name(),
                other
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers scalar null checks into a concrete boolean integer result.
pub(super) fn lower_is_null(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let is_null = matches!(ctx.value_php_type(value)?, PhpType::Void | PhpType::Never);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), i64::from(is_null));
    store_if_result(ctx, inst)
}

/// Emits an integer nonzero check into the canonical integer result register.
pub(super) fn emit_int_result_nonzero_bool(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // compare the integer truthiness operand against zero
            ctx.emitter.instruction("cset x0, ne");                             // materialize nonzero integer truthiness as boolean 1
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // compare the integer truthiness operand against zero
            ctx.emitter.instruction("setne al");                                // materialize nonzero integer truthiness in the low byte
            ctx.emitter.instruction("movzx rax, al");                           // widen the truthiness byte into the integer result register
        }
    }
}

/// Emits a float nonzero check into the canonical integer result register.
pub(super) fn emit_float_result_nonzero_bool(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fcmp d0, #0.0");                           // compare the float truthiness operand against zero
            ctx.emitter.instruction("cset x0, ne");                             // materialize nonzero float truthiness as boolean 1
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("xorpd xmm1, xmm1");                        // materialize a zero float register for comparison
            ctx.emitter.instruction("ucomisd xmm0, xmm1");                      // compare the float truthiness operand against zero
            ctx.emitter.instruction("setne al");                                // materialize nonzero float truthiness in the low byte
            ctx.emitter.instruction("movzx rax, al");                           // widen the truthiness byte into the integer result register
        }
    }
}

/// Emits PHP string truthiness, where `""` and `"0"` are false.
pub(super) fn emit_string_truthiness(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let true_label = ctx.next_label("str_truthy_true");
    let done_label = ctx.next_label("str_truthy_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_string_value_to_regs(value, "x1", "x2")?;
            ctx.emitter.instruction("mov x0, #0");                              // default string truthiness to false
            ctx.emitter.instruction(&format!("cbz x2, {}", done_label));        // empty strings are false
            ctx.emitter.instruction("cmp x2, #1");                              // detect the one-byte special case for string "0"
            ctx.emitter.instruction(&format!("b.ne {}", true_label));           // multi-byte non-empty strings are truthy
            ctx.emitter.instruction("ldrb w10, [x1]");                          // load the only byte for the PHP string "0" check
            ctx.emitter.instruction("cmp w10, #48");                            // compare the byte against ASCII '0'
            ctx.emitter.instruction("cset x0, ne");                             // one-byte strings are truthy unless that byte is '0'
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the generic truthy case
            ctx.emitter.label(&true_label);
            ctx.emitter.instruction("mov x0, #1");                              // mark non-empty non-"0" strings as truthy
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.load_string_value_to_regs(value, "r10", "r11")?;
            ctx.emitter.instruction("mov rax, 0");                              // default string truthiness to false
            ctx.emitter.instruction("test r11, r11");                           // test whether the string length is zero
            ctx.emitter.instruction(&format!("je {}", done_label));             // empty strings are false
            ctx.emitter.instruction("cmp r11, 1");                              // detect the one-byte special case for string "0"
            ctx.emitter.instruction(&format!("jne {}", true_label));            // multi-byte non-empty strings are truthy
            ctx.emitter.instruction("movzx ecx, BYTE PTR [r10]");               // load the only byte for the PHP string "0" check
            ctx.emitter.instruction("cmp ecx, 48");                             // compare the byte against ASCII '0'
            ctx.emitter.instruction("setne al");                                // one-byte strings are truthy unless that byte is '0'
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the generic truthy case
            ctx.emitter.label(&true_label);
            ctx.emitter.instruction("mov rax, 1");                              // mark non-empty non-"0" strings as truthy
            ctx.emitter.label(&done_label);
        }
    }
    Ok(())
}
