//! Purpose:
//! Lowers binary numeric PHP builtins for the EIR backend.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::math`.
//!
//! Key details:
//! - Preserves PHP source evaluation order before arranging libc/ABI argument
//!   registers for integer division and floating-point helpers.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::Instruction;
use crate::types::PhpType;

use super::super::super::super::context::FunctionContext;
use super::super::{expect_operand, store_if_result};

/// Lowers `intdiv()` for concrete integer-like numeric operands.
pub(in crate::codegen_ir::lower_inst::builtins) fn lower_intdiv(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "intdiv", 2)?;
    let zero_label = ctx.next_label("intdiv_zero");
    let done_label = ctx.next_label("intdiv_done");
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_numeric_as_int(ctx, lhs, "intdiv")?;
            abi::emit_push_reg(ctx.emitter, "x0");
            load_numeric_as_int(ctx, rhs, "intdiv")?;
            abi::emit_pop_reg(ctx.emitter, "x1");
            ctx.emitter.instruction(&format!("cbz x0, {}", zero_label));        // branch to the fatal path when the divisor is zero
            ctx.emitter.instruction("sdiv x0, x1, x0");                         // divide the saved dividend by the current divisor
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the fatal path after successful integer division
        }
        Arch::X86_64 => {
            load_numeric_as_int(ctx, lhs, "intdiv")?;
            abi::emit_push_reg(ctx.emitter, "rax");
            load_numeric_as_int(ctx, rhs, "intdiv")?;
            abi::emit_pop_reg(ctx.emitter, "r11");
            ctx.emitter.instruction("test rax, rax");                           // check whether the divisor is zero
            ctx.emitter.instruction(&format!("je {}", zero_label));             // branch to the fatal path when the divisor is zero
            ctx.emitter.instruction("mov r10, rax");                            // preserve the divisor before idiv uses rax
            ctx.emitter.instruction("mov rax, r11");                            // move the saved dividend into the idiv accumulator
            ctx.emitter.instruction("cqo");                                     // sign-extend the dividend across rdx:rax
            ctx.emitter.instruction("idiv r10");                                // divide the saved dividend by the preserved divisor
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the fatal path after successful integer division
        }
    }
    emit_intdiv_zero_fatal(ctx, &zero_label);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers `fdiv()` for concrete integer-like and floating operands.
pub(in crate::codegen_ir::lower_inst::builtins) fn lower_fdiv(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "fdiv", 2)?;
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    super::load_numeric_as_float(ctx, lhs, "fdiv")?;
    abi::emit_push_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
    super::load_numeric_as_float(ctx, rhs, "fdiv")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_float_reg(ctx.emitter, "d1");
            ctx.emitter.instruction("fdiv d0, d1, d0");                         // compute dividend divided by divisor in the result register
        }
        Arch::X86_64 => {
            abi::emit_pop_float_reg(ctx.emitter, "xmm1");
            ctx.emitter.instruction("divsd xmm1, xmm0");                        // compute dividend divided by divisor in the scratch register
            ctx.emitter.instruction("movsd xmm0, xmm1");                        // move the floating quotient into the result register
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `fmod()` for concrete integer-like and floating operands.
pub(in crate::codegen_ir::lower_inst::builtins) fn lower_fmod(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "fmod", 2)?;
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    super::load_numeric_as_float(ctx, lhs, "fmod")?;
    abi::emit_push_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
    super::load_numeric_as_float(ctx, rhs, "fmod")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_float_reg(ctx.emitter, "d1");
            ctx.emitter.instruction("fdiv d2, d1, d0");                         // compute dividend divided by divisor for fmod truncation
            ctx.emitter.instruction("frintz d2, d2");                           // truncate the quotient toward zero
            ctx.emitter.instruction("fmsub d0, d2, d0, d1");                    // compute dividend minus truncated quotient times divisor
        }
        Arch::X86_64 => {
            abi::emit_pop_float_reg(ctx.emitter, "xmm1");
            ctx.emitter.instruction("movapd xmm2, xmm0");                       // preserve the divisor while ordering libc fmod arguments
            ctx.emitter.instruction("movapd xmm0, xmm1");                       // move the dividend into the first libc fmod argument
            ctx.emitter.instruction("movapd xmm1, xmm2");                       // move the divisor into the second libc fmod argument
            ctx.emitter.bl_c("fmod");
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `pow()` for concrete integer-like and floating operands.
pub(in crate::codegen_ir::lower_inst::builtins) fn lower_pow(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "pow", 2)?;
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    super::load_numeric_as_float(ctx, lhs, "pow")?;
    abi::emit_push_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
    super::load_numeric_as_float(ctx, rhs, "pow")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fmov d1, d0");                             // move the exponent into the second libc pow argument
            abi::emit_pop_float_reg(ctx.emitter, "d0");
            ctx.emitter.bl_c("pow");
        }
        Arch::X86_64 => {
            abi::emit_pop_float_reg(ctx.emitter, "xmm1");
            ctx.emitter.instruction("movapd xmm2, xmm0");                       // preserve the exponent while ordering libc pow arguments
            ctx.emitter.instruction("movapd xmm0, xmm1");                       // move the base into the first libc pow argument
            ctx.emitter.instruction("movapd xmm1, xmm2");                       // move the exponent into the second libc pow argument
            ctx.emitter.bl_c("pow");
        }
    }
    store_if_result(ctx, inst)
}

/// Emits the legacy fatal diagnostic for `intdiv()` division by zero.
fn emit_intdiv_zero_fatal(ctx: &mut FunctionContext<'_>, zero_label: &str) {
    ctx.emitter.label(zero_label);
    let (err_label, err_len) = ctx.data.add_string(b"Fatal error: division by zero\n");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // select stderr as the fatal diagnostic destination
            ctx.emitter.adrp("x1", &err_label);
            ctx.emitter.add_lo12("x1", "x1", &err_label);
            ctx.emitter.instruction(&format!("mov x2, #{}", err_len));          // pass the fatal diagnostic byte length to write()
            ctx.emitter.syscall(4);
            ctx.emitter.instruction("mov x0, #1");                              // select process exit code 1 after the fatal diagnostic
            ctx.emitter.syscall(1);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("lea rsi, [rip + {}]", err_label)); //pass the fatal diagnostic buffer to write()
            ctx.emitter.instruction(&format!("mov edx, {}", err_len));          // pass the fatal diagnostic byte length to write()
            ctx.emitter.instruction("mov edi, 2");                              // select stderr as the fatal diagnostic destination
            ctx.emitter.instruction("mov eax, 1");                              // select Linux write syscall
            ctx.emitter.instruction("syscall");                                 // write the fatal division-by-zero diagnostic
            ctx.emitter.instruction("mov edi, 1");                              // select process exit code 1 after the fatal diagnostic
            ctx.emitter.instruction("mov eax, 60");                             // select Linux exit syscall
            ctx.emitter.instruction("syscall");                                 // terminate after reporting division by zero
        }
    }
}

/// Loads a numeric operand and normalizes values into the integer result register.
fn load_numeric_as_int(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    name: &str,
) -> Result<()> {
    match ctx.load_value_to_result(value)?.codegen_repr() {
        PhpType::Int | PhpType::Bool => Ok(()),
        PhpType::TaggedScalar => {
            crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            Ok(())
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            Ok(())
        }
        PhpType::Float => {
            abi::emit_float_result_to_int_result(ctx.emitter);
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name, other
        ))),
    }
}
