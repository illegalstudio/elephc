//! Purpose:
//! Lowers random integer math builtins for the EIR backend.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::math`.
//!
//! Key details:
//! - Range arguments are evaluated by AST-to-EIR in PHP source order; this module
//!   reloads the SSA slots and preserves the lower bound across runtime helper calls.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::super::context::FunctionContext;
use super::super::{expect_operand, store_if_result};

/// Lowers `rand()` and `mt_rand()` with either zero args or an inclusive range.
pub(crate) fn lower_rand(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    match inst.operands.len() {
        0 => abi::emit_call_label(ctx.emitter, "__rt_random_u32"),
        2 => lower_random_range(ctx, inst, name)?,
        count => {
            return Err(CodegenIrError::invalid_module(format!(
                "{} expected 0 or 2 args, got {}",
                name, count
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `random_int()` over an inclusive integer range.
pub(crate) fn lower_random_int(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "random_int", 2)?;
    lower_random_range(ctx, inst, "random_int")?;
    store_if_result(ctx, inst)
}

/// Emits the shared inclusive-range lowering for random integer builtins.
fn lower_random_range(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    let min = expect_operand(inst, 0)?;
    let max = expect_operand(inst, 1)?;
    load_numeric_as_int(ctx, min, name)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    load_numeric_as_int(ctx, max, name)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_aarch64_random_range(ctx),
        Arch::X86_64 => emit_x86_64_random_range(ctx),
    }
}

/// Emits the AArch64 range normalization and runtime call.
fn emit_aarch64_random_range(ctx: &mut FunctionContext<'_>) -> Result<()> {
    abi::emit_pop_reg(ctx.emitter, "x9");
    ctx.emitter.instruction("sub x0, x0, x9");                                  // compute the inclusive range width as max - min
    ctx.emitter.instruction("add x0, x0, #1");                                  // convert the width to the exclusive upper bound for the random helper
    abi::emit_push_reg(ctx.emitter, "x9");
    abi::emit_call_label(ctx.emitter, "__rt_random_uniform");
    abi::emit_pop_reg(ctx.emitter, "x9");
    ctx.emitter.instruction("add x0, x0, x9");                                  // shift the sampled offset back into the caller-visible range
    Ok(())
}

/// Emits the x86_64 range normalization and runtime call.
fn emit_x86_64_random_range(ctx: &mut FunctionContext<'_>) -> Result<()> {
    abi::emit_pop_reg(ctx.emitter, "r9");
    ctx.emitter.instruction("sub rax, r9");                                     // compute the inclusive range width as max - min
    ctx.emitter.instruction("add rax, 1");                                      // convert the width to the exclusive upper bound for the random helper
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the exclusive upper bound to the random helper
    abi::emit_call_label(ctx.emitter, "__rt_random_uniform");
    ctx.emitter.instruction("add rax, r9");                                     // shift the sampled offset back into the caller-visible range
    Ok(())
}

/// Loads a numeric range operand and normalizes values into the integer result register.
fn load_numeric_as_int(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    name: &str,
) -> Result<()> {
    match ctx.load_value_to_result(value)?.codegen_repr() {
        PhpType::Int | PhpType::Bool => Ok(()),
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
