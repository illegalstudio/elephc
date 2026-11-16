//! Purpose:
//! Lowers the fused packed-int map+sum and filter-count kernels selected by AST fusion.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_01`.
//!
//! Key details:
//! - Both kernels take the array pointer and one literal integer parameter.
//! - The map+sum kernel returns a fresh boxed Mixed cell; the filter-count kernel
//!   returns a raw PHP int.

use crate::codegen::context::FunctionContext;
use crate::codegen::platform::Arch;
use crate::codegen::{abi, Result};
use crate::ir::Instruction;

use super::{expect_operand, store_if_result};

/// Lowers `array_map_sum_add(array, offset)` into the fused runtime kernel.
pub(crate) fn lower_array_map_sum_add(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_two_operand_kernel(ctx, inst, "__rt_array_map_sum_add")
}

/// Lowers the raw-int observation of the fused map+sum kernel.
pub(crate) fn lower_array_map_sum_add_int(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_two_operand_kernel(ctx, inst, "__rt_array_map_sum_add_int")
}

/// Lowers the one-time checked probe for the loop-invariant raw map+sum cache.
pub(crate) fn lower_array_map_sum_probe_int(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_two_operand_kernel(ctx, inst, "__rt_array_map_sum_probe_int")
}

/// Lowers the cached raw map+sum dispatch with a sentinel fallback to the exact kernel.
pub(crate) fn lower_array_map_sum_add_int_cached(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let offset = expect_operand(inst, 1)?;
    let probe = expect_operand(inst, 2)?;
    let general = ctx.next_label("array_map_sum_cached_general");
    let done = ctx.next_label("array_map_sum_cached_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            ctx.load_value_to_reg(offset, "x1")?;
            ctx.load_value_to_reg(probe, "x2")?;
            abi::emit_load_int_immediate(ctx.emitter, "x3", i64::MIN);
            ctx.emitter.instruction("cmp x2, x3");                              // did the probe produce an exact map+sum?
            ctx.emitter.instruction(&format!("b.eq {general}"));                // otherwise use the exact raw kernel
            ctx.emitter.instruction("mov x0, x2");                              // return the cached exact result
            ctx.emitter.instruction(&format!("b {done}"));                      // skip the raw kernel
            ctx.emitter.label(&general);
            crate::codegen::lower_inst::core_builtins::prepare_deferred_call_site(ctx, inst)?;
            ctx.load_value_to_reg(array, "x0")?;
            ctx.load_value_to_reg(offset, "x1")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_map_sum_add_int");
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
            ctx.load_value_to_reg(offset, "rsi")?;
            ctx.load_value_to_reg(probe, "rdx")?;
            ctx.emitter.instruction("mov rax, 0x8000000000000000");             // sentinel for an invalid probe
            ctx.emitter.instruction("cmp rdx, rax");                            // did the probe produce an exact map+sum?
            ctx.emitter.instruction(&format!("je {general}"));                  // otherwise use the exact raw kernel
            ctx.emitter.instruction("mov rax, rdx");                            // return the cached exact result
            ctx.emitter.instruction(&format!("jmp {done}"));                    // skip the raw kernel
            ctx.emitter.label(&general);
            crate::codegen::lower_inst::core_builtins::prepare_deferred_call_site(ctx, inst)?;
            ctx.load_value_to_reg(array, "rdi")?;
            ctx.load_value_to_reg(offset, "rsi")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_map_sum_add_int");
            ctx.emitter.label(&done);
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers the declared-int map+sum overflow preflight.
pub(crate) fn lower_array_map_sum_add_safe(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_two_operand_kernel(ctx, inst, "__rt_array_map_sum_add_safe")
}

/// Lowers `array_filter_count_mod(array, divisor)` into the fused runtime kernel.
pub(crate) fn lower_array_filter_count_mod(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_two_operand_kernel(ctx, inst, "__rt_array_filter_count_mod")
}

/// Loads the array pointer and literal parameter into the target argument registers.
fn lower_two_operand_kernel(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    symbol: &str,
) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let parameter = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            ctx.load_value_to_reg(parameter, "x1")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
            ctx.load_value_to_reg(parameter, "rsi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, symbol);
    store_if_result(ctx, inst)
}
