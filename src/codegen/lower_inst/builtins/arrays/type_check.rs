//! Purpose:
//! Lowers the fused packed-array runtime-tag predicate selected by AST fusion.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_01`.
//!
//! Key details:
//! - Operands are the packed array, an integer index, and a 16-bit runtime-tag mask.
//! - The kernel reads the stored tag directly and falls back to the generic boxed read.

use crate::codegen::context::FunctionContext;
use crate::codegen::platform::Arch;
use crate::codegen::{abi, Result};
use crate::ir::Instruction;

use super::{expect_operand, store_if_result};

/// Lowers `array_element_type_check(array, index, mask)` into the fused runtime kernel.
pub(crate) fn lower_array_element_type_check(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let index = expect_operand(inst, 1)?;
    let mask = expect_operand(inst, 2)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            ctx.load_value_to_reg(index, "x1")?;
            ctx.load_value_to_reg(mask, "x2")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
            ctx.load_value_to_reg(index, "rsi")?;
            ctx.load_value_to_reg(mask, "rdx")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_element_type_check");
    store_if_result(ctx, inst)
}
